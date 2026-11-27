#!/usr/bin/env python3
"""Self-test for tools/gates/run, with stand-in gates in place of the gate line.

    python3 -I -B tools/gates/test_run.py

Each test runs the script with a GATE_LINE of small shell commands that leave
marks in a scratch directory, and reads the marks, the logs, the table and the
exit status. No test reads the clock: where one gate waits for another, the
wait is bounded only so that a broken script fails the test instead of hanging
it.
"""

import os
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
RUN = HERE / "run"
# The longest a stand-in gate waits for another. It only makes a broken test
# fail instead of hanging.
WAIT_BOUND = 120


def wait_for(path):
    """A shell command that waits until PATH exists, and fails if it never
    does."""
    tries = WAIT_BOUND * 10
    return (f"i=0; while [ ! -e {path} ] && [ $i -lt {tries} ]; do sleep 0.1; i=$((i + 1)); done; "
            f"[ -e {path} ]")


class Gates(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.dir = self.tmp / "gates"

    def tearDown(self):
        self._tmp.cleanup()

    def run_gates(self, *gates, jobs="8", args=(), **extra):
        env = dict(os.environ, GATE_LINE="\n".join(gates), **extra)
        env.pop("CARGO_TARGET_DIR", None)
        return subprocess.run(["python3", "-I", "-B", str(RUN), "--jobs", jobs, "--dir", str(self.dir), *args],
                              env=env, cwd=self.tmp, capture_output=True, text=True, timeout=WAIT_BOUND * 2)

    def table(self, p):
        """The table's rows, between its heading and the line after it."""
        lines = p.stdout.splitlines()
        start = lines.index("exit  gate") + 1
        return [line for line in lines[start:] if not line.startswith(("log of ", "gates: "))]

    def test_gates_run_at_once_and_the_table_keeps_the_gate_lines_order(self):
        # The first gate passes only once the third has run, which it can do
        # only while the first is still running.
        first = wait_for(self.tmp / "third")
        p = self.run_gates(first, "echo two", ": > third", jobs="3")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.table(p), [f"0  {first}", "0  echo two", "0  : > third"])
        self.assertEqual(p.stdout.splitlines()[-1], "gates: all passed")

    def test_jobs_1_runs_the_gates_one_after_another_in_order(self):
        p = self.run_gates("[ ! -e b ] && : > a", "[ -e a ] && [ ! -e c ] && : > b", "[ -e b ] && : > c", jobs="1")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertTrue((self.tmp / "c").exists())

    def test_a_failed_gate_stops_none_and_fails_the_run(self):
        for jobs in ("1", "8"):
            with self.subTest(jobs=jobs):
                for mark in ("ran", "last"):
                    (self.tmp / mark).unlink(missing_ok=True)
                p = self.run_gates("echo the first fails; exit 1", ": > ran", "exit 3", ": > last", jobs=jobs)
                self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
                self.assertTrue((self.tmp / "ran").exists(), "the gate after a failed one never ran")
                self.assertTrue((self.tmp / "last").exists(), "the last gate, after a second failure, never ran")
                self.assertEqual(self.table(p), ["1  echo the first fails; exit 1", "0  : > ran", "3  exit 3",
                                                 "0  : > last"])
                logs = self.dir / "logs"
                self.assertIn(f"log of echo the first fails; exit 1: {logs}/01-echo-the-first-fails-exit-1.log",
                              p.stdout)
                self.assertIn(f"log of exit 3: {logs}/03-exit-3.log", p.stdout)
                self.assertNotIn("log of : > ran", p.stdout)
                self.assertNotIn("log of : > last", p.stdout)
                self.assertEqual(p.stderr.splitlines()[-1], "gates: FAILED")
                self.assertNotIn("all passed", p.stdout)

    def test_a_gate_killed_by_a_signal_reports_128_and_its_number(self):
        p = self.run_gates("kill -TERM $$")
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertEqual(self.table(p), ["143  kill -TERM $$"])

    def test_each_gate_has_a_target_and_build_directory_of_its_own(self):
        gates = [f'echo "$CARGO_TARGET_DIR $CARGO_BUILD_BUILD_DIR" > {n}' for n in ("one", "two", "three")]
        p = self.run_gates(*gates, CARGO_BUILD_BUILD_DIR=str(self.tmp / "shared"))
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        dirs = [(self.tmp / n).read_text().split() for n in ("one", "two", "three")]
        own = [str(self.dir / f"echo-cargo-target-dir-cargo-build-build-dir-{n}") for n in ("one", "two", "three")]
        self.assertEqual(dirs, [[d, d] for d in own])

    def test_a_build_root_holds_the_cargo_directories_and_dir_the_logs(self):
        root = self.tmp / "root"
        p = self.run_gates('echo "$CARGO_TARGET_DIR $CARGO_BUILD_BUILD_DIR" > one', args=("--build-root", str(root)))
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        own = str(root / "echo-cargo-target-dir-cargo-build-build-dir-one")
        self.assertEqual((self.tmp / "one").read_text().split(), [own, own])
        self.assertTrue((self.dir / "logs" / "01-echo-cargo-target-dir-cargo-build-build-dir-one.log").exists())

    def test_the_run_removes_the_cargo_directories_no_gate_owns(self):
        root = self.tmp / "root"

        def cargo_dir(name):
            (root / name).mkdir(parents=True)
            (root / name / "CACHEDIR.TAG").write_text("Signature: 8a477f597d28d172789f06886806bc55\n")
            return root / name

        cargo_dir("true")
        gone = cargo_dir("make-old-gate")
        cargo_dir("minspec-size")
        cargo_dir("Not_A-Gate")
        (root / "not-cargo").mkdir()
        (root / "notes").write_text("kept\n")
        (root / "linked").symlink_to(cargo_dir("Elsewhere"))
        p = self.run_gates("true", "false", args=("--build-root", str(root), "--keep", "minspec-size"))
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertFalse(gone.exists(), "the directory of a gate the run does not list stays")
        self.assertIn(f"gates: removed {gone}, which no gate of this run builds in", p.stdout)
        self.assertEqual(sorted(e.name for e in root.iterdir()),
                         ["Elsewhere", "Not_A-Gate", "linked", "minspec-size", "not-cargo", "notes", "true"])

    def test_the_longest_gates_of_the_last_run_start_first(self):
        # Two jobs. The second and third gates each leave a mark and wait for
        # the other's, and the first passes only if both marks are there when
        # it starts: the run passes only if the two longest gates of the last
        # run start first. A gate with no recorded time would start before
        # them all.
        gates = ["[ -e a ] && [ -e b ]", f": > a; {wait_for(self.tmp / 'b')}", f": > b; {wait_for(self.tmp / 'a')}"]
        self.dir.mkdir()
        (self.dir / "durations.tsv").write_text(f"1\t{gates[0]}\n90\t{gates[1]}\n80\t{gates[2]}\n")
        p = self.run_gates(*gates, jobs="2")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.table(p), [f"0  {g}" for g in gates])
        recorded = (self.dir / "durations.tsv").read_text().splitlines()
        self.assertEqual([line.split("\t", 1)[1] for line in recorded], gates)

    def test_each_gates_output_goes_to_its_log_alone(self):
        p = self.run_gates("echo out of the first; echo its error >&2", "echo out of the second")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        logs = self.dir / "logs"
        self.assertEqual((logs / "01-echo-out-of-the-first-echo-its-error-2.log").read_text(),
                         "out of the first\nits error\n")
        self.assertEqual((logs / "02-echo-out-of-the-second.log").read_text(), "out of the second\n")
        printed = (p.stdout + p.stderr).splitlines()
        for line in ("out of the first", "its error", "out of the second"):
            self.assertNotIn(line, printed)
        self.assertIn(f"==> echo out of the second (log: {logs}/02-echo-out-of-the-second.log)", p.stdout)

    def test_jobs_1_shows_the_output_as_well(self):
        p = self.run_gates("echo out of the first", "echo out of the second", jobs="1")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        lines = p.stdout.splitlines()
        self.assertEqual(lines[:2], ["==> echo out of the first", "out of the first"])
        self.assertEqual((self.dir / "logs" / "02-echo-out-of-the-second.log").read_text(), "out of the second\n")

    def test_a_log_is_replaced_at_each_run(self):
        self.run_gates("echo first run")
        self.run_gates("echo first run; echo again")
        logs = sorted((self.dir / "logs").iterdir())
        self.assertEqual([l.name for l in logs], ["01-echo-first-run-echo-again.log", "01-echo-first-run.log"])
        self.run_gates("echo first run")
        self.assertEqual((self.dir / "logs" / "01-echo-first-run.log").read_text(), "first run\n")

    def test_a_gate_reads_nothing(self):
        p = self.run_gates("[ \"$(readlink /proc/self/fd/0)\" = /dev/null ]")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)

    def test_an_empty_list_or_a_bad_jobs_runs_nothing(self):
        p = self.run_gates()
        self.assertEqual(p.returncode, 2)
        self.assertIn("gates: the gate list is empty", p.stderr)
        for jobs in ("0", "two", "-1"):
            with self.subTest(jobs=jobs):
                p = self.run_gates(": > ran", jobs=jobs)
                self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
                self.assertFalse((self.tmp / "ran").exists())

    def test_two_gates_of_one_name_are_refused(self):
        p = self.run_gates(": > ran", ":  >  ran")
        self.assertEqual(p.returncode, 2)
        self.assertIn("two gates have the name 'ran'", p.stderr)
        self.assertFalse((self.tmp / "ran").exists())


class Make(unittest.TestCase):
    """make gates hands the script JOBS, half the cores and at least 8 unless
    set, a log directory under the target directory in use, and
    VTIUM_GATES_DIR as the build root: on the build volume when it exists and
    the caller set no CARGO_TARGET_DIR, and the log directory otherwise. Each
    test names BUILD_VOLUME, so none depends on this machine's volume."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.volume = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def dry_run(self, *args, **env):
        environ = {k: v for k, v in os.environ.items()
                   if k not in ("MAKEFLAGS", "MFLAGS", "JOBS", "CARGO_TARGET_DIR", "VTIUM_GATES_DIR")}
        environ.update(env)
        p = subprocess.run(["make", "--no-print-directory", "-n", "-C", str(ROOT), "gates", *args],
                           env=environ, capture_output=True, text=True, timeout=WAIT_BOUND)
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        return p.stdout

    def test_jobs_is_half_the_cores_and_at_least_8_unless_set(self):
        jobs = max(8, os.cpu_count() // 2)
        self.assertIn(f"tools/gates/run --jobs {jobs} --dir {ROOT}/target/gates ", self.dry_run())
        self.assertIn("tools/gates/run --jobs 4 ", self.dry_run("JOBS=4"))
        self.assertIn("tools/gates/run --jobs 1 ", self.dry_run("JOBS=1"))
        self.assertIn("tools/gates/run --jobs 3 ", self.dry_run(JOBS="3"))

    def test_the_gates_build_on_the_volume_under_the_checkouts_name(self):
        root = f"{self.volume}/vtium/{ROOT.name}/gates"
        out = self.dry_run(f"BUILD_VOLUME={self.volume}")
        self.assertIn(f"--dir {ROOT}/target/gates --build-root {root} --keep minspec-size;", out)
        self.assertIn(f"CARGO_TARGET_DIR={root}/minspec-size CARGO_BUILD_BUILD_DIR={root}/minspec-size ", out)

    def test_without_the_volume_the_gates_build_in_the_target_directory(self):
        out = self.dry_run(f"BUILD_VOLUME={self.volume}/missing")
        self.assertIn(f"--dir {ROOT}/target/gates --build-root {ROOT}/target/gates --keep", out)

    def test_the_directory_follows_cargo_target_dir(self):
        volume = f"BUILD_VOLUME={self.volume}"
        self.assertIn("--dir /elsewhere/build/gates --build-root /elsewhere/build/gates ",
                      self.dry_run(volume, CARGO_TARGET_DIR="/elsewhere/build"))
        self.assertIn(f"--dir {ROOT}/relative/gates --build-root {ROOT}/relative/gates ",
                      self.dry_run(volume, "CARGO_TARGET_DIR=relative"))

    def test_vtium_gates_dir_wins(self):
        volume = f"BUILD_VOLUME={self.volume}"
        self.assertIn("--build-root /chosen/gates ", self.dry_run(volume, VTIUM_GATES_DIR="/chosen/gates"))
        out = self.dry_run(volume, "VTIUM_GATES_DIR=/chosen/gates", CARGO_TARGET_DIR="/elsewhere/build")
        self.assertIn("--dir /elsewhere/build/gates --build-root /chosen/gates ", out)


class SizeBuild(unittest.TestCase):
    """After the gates, make gates builds the minspec binary that tier 3
    measures, whatever FEATURES and CARGO_TARGET_DIR make's command line names,
    and only when the gates ran. A stand-in for make records where the build
    would go and with which features: it runs the real make with -n, which
    prints the build without running it, after printing the two variables as
    the sub-make sees them."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        self.stub = self.tmp / "make"
        self.stub.write_text("#!/bin/sh\n"
                             f"touch {self.tmp}/size-build-ran\n"
                             "exec make \"$@\" -n "
                             "--eval='$(info size build: target=$(CARGO_TARGET_DIR) "
                             "build=$(CARGO_BUILD_BUILD_DIR) features=[$(FEATURES)])'\n")
        self.stub.chmod(0o755)

    def tearDown(self):
        self._tmp.cleanup()

    def gates(self, *args):
        environ = {k: v for k, v in os.environ.items()
                   if k not in ("MAKEFLAGS", "MFLAGS", "JOBS", "CARGO_TARGET_DIR", "FEATURES",
                                "VTIUM_GATES_DIR")}
        return subprocess.run(["make", "--no-print-directory", "-C", str(ROOT), "gates",
                               f"MAKE={self.stub}", *args],
                              env=environ, capture_output=True, text=True, timeout=WAIT_BOUND)

    def test_the_size_build_ignores_features_and_cargo_target_dir_from_the_command_line(self):
        target = self.tmp / "build"
        p = self.gates("GATE_LINE=true", "JOBS=1", f"CARGO_TARGET_DIR={target}", "FEATURES=images bidi")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        log = (target / "gates" / "logs" / "minspec-size.log").read_text()
        own = f"{target}/gates/minspec-size"
        self.assertIn(f"size build: target={own} build={own} features=[]\n", log)

    def test_a_usage_error_skips_the_size_build(self):
        for args in (("GATE_LINE=true", "JOBS=0"), ("GATE_LINE=",)):
            with self.subTest(args=args):
                p = self.gates(*args, f"CARGO_TARGET_DIR={self.tmp / 'build'}")
                self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
                self.assertNotIn("make minspec", p.stdout)
                self.assertFalse((self.tmp / "size-build-ran").exists())


if __name__ == "__main__":
    unittest.main()
