"""Tier 3's runner: the variants run one at a time or in a pool of worker
processes, under the run lock, and their results assembled into the report.

GL variants share the host's render node and never run together. A lock on
fork/build/tier3.lock refuses a second concurrent run.
"""

import contextlib
import fcntl
import json
import multiprocessing
import multiprocessing.connection
import os
import signal
import sys
import time
import uuid

import t3_impl
import t3_qemu
import t3_scenario
from t3_common import LOCK, REPORT, RUNDIR, rel

# Default seconds a variant's worker may run before it is stopped (--deadline).
# Every wait in the scenario has its own timeout well inside this; it catches
# a hang nothing else does.
VARIANT_DEADLINE = 1800


class Terminated(BaseException):
    """Raised by SIGTERM, so finally blocks stop the VMs. Not an Exception:
    nothing that reports a variant's own failure may swallow it."""


def raise_on_sigterm(log):
    """Make SIGTERM raise Terminated in this process."""
    def terminate(signum, frame):
        log("received signal %d" % signum)
        raise Terminated("terminated by signal %d" % signum)
    signal.signal(signal.SIGTERM, terminate)


def _worker(variant, impl, log, conn):
    """A worker process: reports each QEMU it starts and stops, then its
    result."""
    raise_on_sigterm(log.labelled(variant))
    t3_qemu.on_qemu = lambda event, pid: conn.send((event, pid))
    try:
        conn.send(("result", t3_scenario.variant_job(variant, impl, log)))
    except Terminated:
        sys.exit(128 + signal.SIGTERM)
    finally:
        conn.close()


class Job:
    """One variant's worker process and what it has reported."""

    def __init__(self, ctx, variant, impl, log):
        self.variant = variant
        self.log = log
        self.conn, child = ctx.Pipe(duplex=False)
        self.proc = ctx.Process(target=_worker, args=(variant, impl, log, child),
                                name="t3-" + variant)
        self.proc.start()
        child.close()
        self.start = time.time()
        self.qemu = set()  # QEMUs started and not yet reported stopped
        self.result = None

    def drain(self):
        try:
            while self.conn.poll():
                kind, value = self.conn.recv()
                if kind == "start":
                    self.qemu.add(value)
                elif kind == "stop":
                    self.qemu.discard(value)
                else:
                    self.result = value
        except (EOFError, OSError):  # the worker is gone
            pass

    def stop(self, why):
        """Stop the worker: SIGTERM lets it stop its VM first."""
        if self.proc.is_alive():
            self.log("%s: %s; sending SIGTERM to worker pid %d"
                     % (self.variant, why, self.proc.pid))
            self.proc.terminate()
            self.proc.join(60)
        if self.proc.is_alive():
            self.log("%s: worker pid %d did not exit; sending SIGKILL"
                     % (self.variant, self.proc.pid))
            self.proc.kill()

    def finish(self):
        """Reap the worker and SIGKILL any QEMU of this variant it left."""
        self.proc.join()
        self.drain()
        self.conn.close()
        sock = os.fsencode(os.path.join(RUNDIR, self.variant, "qmp.sock"))
        for pid in self.qemu:
            try:
                with open("/proc/%d/cmdline" % pid, "rb") as f:
                    if sock not in f.read():
                        continue  # gone, or the PID now names another process
                self.log("%s: killing qemu pid %d, left by its worker" % (self.variant, pid))
                os.kill(pid, signal.SIGKILL)
            except OSError:
                pass


def run_concurrently(variants, jobs, impl, deadline, results, log):
    """Fill `results` with {variant: variant_job's result}, running at most
    `jobs` variants at once.

    Each variant runs in its own process, so one variant's compare (pure
    Python, holding the GIL) never stalls another's captures, and a worker
    that dies fails its own variant only. A worker still running `deadline`
    seconds after it started is stopped. Variants on virgl share the host's
    render node, so at most one of them runs at a time. They start in the
    requested order. If this process is interrupted, every worker is stopped
    before the interruption goes on.
    """
    ctx = multiprocessing.get_context("spawn")
    pending, running = list(variants), []
    try:
        while pending or running:
            for variant in list(pending):
                if len(running) >= jobs:
                    break
                if (t3_scenario.uses_virgl(variant)
                        and any(t3_scenario.uses_virgl(j.variant) for j in running)):
                    continue
                pending.remove(variant)
                running.append(Job(ctx, variant, impl, log))
            multiprocessing.connection.wait(
                [j.conn for j in running] + [j.proc.sentinel for j in running], timeout=5)
            for job in list(running):
                alive = job.proc.is_alive()
                job.drain()
                overdue = alive and time.time() - job.start > deadline
                if alive and not overdue:
                    continue
                if overdue:
                    job.stop("no result within the %d s deadline" % deadline)
                job.finish()
                running.remove(job)
                if job.result is not None:
                    results[job.variant] = job.result
                elif overdue:
                    results[job.variant] = t3_scenario.failed_variant(
                        job.variant, "no result within the %d s deadline; worker stopped"
                        % deadline, impl, log)
                else:
                    results[job.variant] = t3_scenario.failed_variant(
                        job.variant, "worker exited with code %s before reporting"
                        % job.proc.exitcode, impl, log)
    finally:
        if running:  # interrupted: a second SIGTERM must not cut the cleanup short
            signal.signal(signal.SIGTERM, signal.SIG_IGN)
        for job in running:
            job.stop("run interrupted")
            job.finish()


def run_variants(variants, jobs, impl, deadline, log):
    """Run every variant, one at a time in this process or up to `jobs` at
    once in workers: ({variant: (case entries, metadata)}, the interruption
    or None).

    An interrupted run (SIGTERM, Ctrl+C) has stopped its VMs: the variants
    that finished keep their results, the rest fail with the interruption.
    """
    results, interrupted = {}, None
    try:
        if jobs == 1:
            for v in variants:
                # A signal during a variant is logged with its name, as a
                # worker's is.
                raise_on_sigterm(log.labelled(v))
                results[v] = t3_scenario.variant_job(v, impl, log)
        else:
            run_concurrently(variants, jobs, impl, deadline, results, log)
    except (Terminated, KeyboardInterrupt) as e:
        interrupted = e
    if jobs == 1:
        raise_on_sigterm(log)
    for v in variants:
        if v not in results:
            results[v] = t3_scenario.failed_variant(
                v, "run interrupted: %s" % (interrupted or "no result"), impl, log)
    log("%d variant(s) done, %d at a time" % (len(variants), jobs))
    return results, interrupted


@contextlib.contextmanager
def run_lock():
    """Hold fork/build/tier3.lock, or exit if another run holds it: two runs
    would share the package, the work directory, the report and the render
    node."""
    os.makedirs(os.path.dirname(LOCK), exist_ok=True)
    with open(LOCK, "a+") as f:
        try:
            fcntl.flock(f, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            f.seek(0)
            raise SystemExit("another tier 3 run (pid %s) holds %s"
                             % (f.read().strip() or "unknown", rel(LOCK)))
        f.truncate(0)
        f.write("%d\n" % os.getpid())
        f.flush()
        yield


# What a run was built from and ran on, passed by fork/tier3.mk.
TOOL_ENV = {"kmscon": "T3_KMSCON_REV", "libtsm": "T3_LIBTSM_REV",
            "kmscon_repo": "T3_KMSCON_REPO", "libtsm_repo": "T3_LIBTSM_REPO", "image_base": "DEV_IMAGE_BASE",
            "archive_date": "DEV_ARCHIVE_DATE", "guest_image": "T3_IMAGE",
            "guest_image_sha256": "T3_IMAGE_SHA256", "guest_archive": "T3_ARCHIVE"}


def run_tools(impl):
    """The commits and pins behind a run; null where the caller gave none.
    A vtium run has no libtsm, so its libtsm fields are null, and kmscon's
    name the checkout whose harness and goldens judged it."""
    tools = {k: os.environ.get(v) or None for k, v in TOOL_ENV.items()}
    if impl["name"] != t3_impl.C:
        tools["libtsm"] = tools["libtsm_repo"] = None
    return tools


def write_report(variants, results, impl):
    """Write the run's report and return its case entries."""
    # The report lists variants in the requested order, whatever order
    # they finished in, so a concurrent run's report matches a serial one's.
    cases = [c for v in variants for c in results[v][0]]
    meta = {v: results[v][1] for v in variants}
    # A fresh id per run lets a comparison tell two runs from one run
    # saved twice.
    report = {"tier": 3, "run_id": uuid.uuid4().hex, "tools": run_tools(impl),
              "implementation": dict(t3_impl.describe(impl),
                                     consoles=t3_scenario.consoles(meta)),
              "cases": cases, "variants": meta}
    os.makedirs(os.path.dirname(REPORT), exist_ok=True)
    with open(REPORT, "w") as f:
        json.dump(report, f, indent=2)
        f.write("\n")
    return cases
