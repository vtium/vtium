#!/usr/bin/env python3
"""Self-test for tools/fuzz/run, without nightly, cargo-fuzz or a real fuzz run.

    python3 -I -B tools/fuzz/test_run.py

The parsing tests read libFuzzer's output captured from real runs in
samples/: ok.txt from vtium-parse's target running for 5 s, and crash.txt from
a scratch target that panics on purpose. The end-to-end tests run the script
in a throwaway git checkout, with a stand-in `cargo` first on PATH that prints
those samples.
"""

import importlib.machinery
import importlib.util
import json
import os
import subprocess
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
RUN = HERE / "run"
SAMPLES = HERE / "samples"

loader = importlib.machinery.SourceFileLoader("fuzzrun", str(RUN))
spec = importlib.util.spec_from_loader("fuzzrun", loader)
fuzzrun = importlib.util.module_from_spec(spec)
loader.exec_module(fuzzrun)

CRASH_NAME = "crash-ee5b4780c05f42762e7b61b58439b45c6b5b485f"

GIT_ENV = {"GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_NOSYSTEM": "1"}


def git(root, *args):
    return subprocess.run(
        ["git", "-C", str(root), "-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", *args],
        check=True, capture_output=True, text=True, env={**os.environ, **GIT_ENV},
    ).stdout.strip()


# The stand-in for `cargo +nightly fuzz build|run TARGET ...`. A run prints a
# sample, the crash sample for the target named boom with its input written
# where -artifact_prefix says, and records what it was given in FAKE_RECORD.
FAKE_CARGO = textwrap.dedent("""\
    #!/usr/bin/env python3
    import json, os, re, sys
    from pathlib import Path
    args = sys.argv[1:]
    assert args[:2] == ["+nightly", "fuzz"], args
    verb, target = args[2], args[3]
    if verb == "build":
        sys.exit(3 if target == "broken" else 0)
    corpus = Path(args[4])
    opts = dict(a.lstrip("-").split("=", 1) for a in args[6:])
    with open(os.environ["FAKE_RECORD"], "a") as rec:
        rec.write(json.dumps({"cwd": os.getcwd(), "target": target, "corpus": str(corpus),
                              "seeds": sorted(p.name for p in corpus.iterdir()), "opts": opts}) + "\\n")
    samples = Path(os.environ["FAKE_SAMPLES"])
    if target == "boom":
        prefix = opts["artifact_prefix"]
        Path(prefix, "%(crash)s").write_bytes(b"vt~")
        text = (samples / "crash.txt").read_text()
        text = re.sub(r"Test unit written to \\S+", "Test unit written to " + prefix + "%(crash)s", text)
        sys.stdout.write(text)
        sys.exit(1)
    (corpus / "grown").write_bytes(b"new")
    sys.stdout.write((samples / "ok.txt").read_text())
""") % {"crash": CRASH_NAME}


def manifest(*targets):
    bins = "".join(f'\n[[bin]]\nname = "{t}"\npath = "fuzz_targets/{t}.rs"\n' for t in targets)
    return f'[package]\nname = "x-fuzz"\nversion = "0.0.0"\n[package.metadata]\ncargo-fuzz = true\n{bins}'


class Checkout:
    """A throwaway git checkout with crates a (boom, x), b (y) and c (broken)."""

    def __init__(self, tmp):
        self.root = Path(tmp) / "repo"
        for crate, targets in {"a": ["x", "boom"], "b": ["y"], "c": ["broken"]}.items():
            fuzz = self.root / "crates" / crate / "fuzz"
            fuzz.mkdir(parents=True)
            (fuzz / "Cargo.toml").write_text(manifest(*targets))
        self.seeds = self.root / "crates" / "a" / "fuzz" / "seeds" / "x"
        self.seeds.mkdir(parents=True)
        (self.seeds / "one.bin").write_bytes(b"\x01")
        (self.seeds / "two.bin").write_bytes(b"\x02\x02")
        (self.root / ".gitignore").write_text("/target\n")
        git(self.root, "init", "-q")
        git(self.root, "add", ".")
        git(self.root, "commit", "-q", "-m", "init")
        self.head = git(self.root, "rev-parse", "HEAD")
        self.bin = Path(tmp) / "bin"
        self.bin.mkdir()
        cargo = self.bin / "cargo"
        cargo.write_text(FAKE_CARGO)
        cargo.chmod(0o755)
        self.record = Path(tmp) / "record.jsonl"

    def run(self, **env):
        full = {**os.environ, **GIT_ENV, "PATH": f"{self.bin}{os.pathsep}{os.environ['PATH']}",
                "VTIUM_FUZZ_ROOT": str(self.root), "FAKE_SAMPLES": str(SAMPLES), "FAKE_RECORD": str(self.record)}
        full.pop("FUZZ_TARGETS", None)
        full.pop("FUZZ_TIME", None)
        full.update(env)
        return subprocess.run([sys.executable, "-I", "-B", str(RUN)], env=full, capture_output=True, text=True)

    def rows(self, crate):
        path = self.root / "crates" / crate / "fuzz" / "runs.tsv"
        if not path.exists():
            return None
        lines = path.read_text().splitlines()
        return lines[0], [line.split("\t") for line in lines[1:]]

    def records(self):
        if not self.record.exists():
            return []
        return [json.loads(line) for line in self.record.read_text().splitlines()]


class ParseStats(unittest.TestCase):
    def test_an_ok_run(self):
        s = fuzzrun.parse_stats((SAMPLES / "ok.txt").read_text())
        self.assertEqual(s.runs, 189087)
        self.assertEqual(s.seconds, 6)
        self.assertEqual(s.corpus, 1185)
        self.assertEqual(s.cov, 841)
        self.assertIsNone(s.artifact)

    def test_a_crash(self):
        s = fuzzrun.parse_stats((SAMPLES / "crash.txt").read_text())
        self.assertEqual(s.runs, 10272)
        self.assertIsNone(s.seconds)
        self.assertEqual(s.corpus, 5)
        self.assertEqual(s.cov, 22)
        self.assertEqual(s.artifact, f"/src/vtium/target/fuzz-artifacts/scratch/boom/{CRASH_NAME}")
        self.assertEqual(s.artifact_kind, "crash")

    def test_a_slow_input_is_no_failure(self):
        text = "Test unit written to /t/slow-unit-abc\n#9\tDONE   cov: 3 ft: 4 corp: 2/2b\nDone 9 runs in 1 second(s)\n"
        s = fuzzrun.parse_stats(text)
        self.assertIsNone(s.artifact)
        self.assertEqual((s.runs, s.corpus, s.cov), (9, 2, 3))

    def test_output_without_figures(self):
        s = fuzzrun.parse_stats("error: no such command: `fuzz`\n")
        self.assertEqual((s.runs, s.seconds, s.corpus, s.cov, s.artifact), (None,) * 5)

    def test_result(self):
        root = Path("/src/vtium")
        crash = fuzzrun.parse_stats((SAMPLES / "crash.txt").read_text())
        self.assertEqual(fuzzrun.result_of(1, crash, root), f"crash target/fuzz-artifacts/scratch/boom/{CRASH_NAME}")
        self.assertEqual(fuzzrun.result_of(1, fuzzrun.Stats(), root), "failed with exit status 1")
        self.assertEqual(fuzzrun.result_of(0, fuzzrun.parse_stats((SAMPLES / "ok.txt").read_text()), root), "ok")


class Rows(unittest.TestCase):
    def test_row_format(self):
        row = fuzzrun.format_row(["2026-11-03T04:00:00Z", "abc", "parse", "61", "189087", "1185", "841", "ok", "x86_64 32 CPUs"])
        self.assertEqual(row, "2026-11-03T04:00:00Z\tabc\tparse\t61\t189087\t1185\t841\tok\tx86_64 32 CPUs\n")
        with self.assertRaises(ValueError):
            fuzzrun.format_row(["a\tb"])
        self.assertEqual(fuzzrun.figure(None), "-")

    def test_header_once_then_appends(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "runs.tsv"
            fuzzrun.append_row(path, "1\n")
            fuzzrun.append_row(path, "2\n")
            self.assertEqual(path.read_text(), fuzzrun.HEADER + "1\n2\n")
            self.assertTrue(fuzzrun.HEADER.startswith("# ") and fuzzrun.HEADER.count("\n") == 1)
            # A file whose last line lost its newline gets one before the row.
            path.write_text(fuzzrun.HEADER + "1")
            fuzzrun.append_row(path, "2\n")
            self.assertEqual(path.read_text(), fuzzrun.HEADER + "1\n2\n")

    def test_host(self):
        self.assertRegex(fuzzrun.host(), r"^\S+ \d+ CPUs$")


class Dirty(unittest.TestCase):
    def test_dirty_detection(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = Checkout(tmp)
            self.assertEqual(fuzzrun.commit_of(c.root), c.head)
            # An untracked file and a changed runs.tsv leave the tree clean.
            (c.root / "scratch.txt").write_text("x")
            (c.root / "crates" / "a" / "fuzz" / "runs.tsv").write_text("# h\n")
            git(c.root, "add", "crates/a/fuzz/runs.tsv")
            git(c.root, "commit", "-q", "-m", "rows")
            head = git(c.root, "rev-parse", "HEAD")
            with open(c.root / "crates" / "a" / "fuzz" / "runs.tsv", "a") as f:
                f.write("row\n")
            self.assertEqual(fuzzrun.commit_of(c.root), head)
            # A changed tracked file, staged or not, marks it dirty.
            manifest_path = c.root / "crates" / "b" / "fuzz" / "Cargo.toml"
            manifest_path.write_text(manifest_path.read_text() + "\n")
            self.assertEqual(fuzzrun.commit_of(c.root), head + "-dirty")
            git(c.root, "add", "crates/b/fuzz/Cargo.toml")
            self.assertEqual(fuzzrun.commit_of(c.root), head + "-dirty")


class Select(unittest.TestCase):
    found = {"a": ["boom", "x"], "b": ["y"]}

    def test_all_by_default(self):
        self.assertEqual(fuzzrun.select(self.found, ""), [("a", "boom"), ("a", "x"), ("b", "y")])
        self.assertEqual(fuzzrun.select(self.found, "  "), [("a", "boom"), ("a", "x"), ("b", "y")])

    def test_named_targets_in_order(self):
        self.assertEqual(fuzzrun.select(self.found, "b:y a:x"), [("b", "y"), ("a", "x")])
        self.assertEqual(fuzzrun.select(self.found, "a"), [("a", "boom"), ("a", "x")])
        self.assertEqual(fuzzrun.select(self.found, "a:x a"), [("a", "x"), ("a", "boom")])

    def test_unknown_names(self):
        for spec in ["z", "z:y", "a:y", "a:"]:
            with self.subTest(spec=spec), self.assertRaises(fuzzrun.UsageError):
                fuzzrun.select(self.found, spec)

    def test_seconds(self):
        self.assertEqual(fuzzrun.seconds_from(""), 60)
        self.assertEqual(fuzzrun.seconds_from("5"), 5)
        for bad in ["0", "-1", "1.5", "x"]:
            with self.subTest(bad=bad), self.assertRaises(fuzzrun.UsageError):
                fuzzrun.seconds_from(bad)

    def test_discover(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = Checkout(tmp)
            self.assertEqual(fuzzrun.discover(c.root), {"a": ["boom", "x"], "b": ["y"], "c": ["broken"]})


class EndToEnd(unittest.TestCase):
    def test_a_crash_logs_the_others_and_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = Checkout(tmp)
            seeds_before = sorted((p.name, p.read_bytes()) for p in c.seeds.iterdir())
            p = c.run(FUZZ_TIME="7")
            self.assertEqual(p.returncode, 1, p.stderr)

            header, rows = c.rows("a")
            self.assertEqual(header + "\n", fuzzrun.HEADER)
            self.assertEqual([r[2] for r in rows], ["boom", "x"])
            boom, x = rows
            self.assertEqual(len(boom), 9)
            self.assertRegex(boom[0], r"^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\dZ$")
            self.assertEqual(boom[1:3] + boom[4:8], [c.head, "boom", "10272", "5", "22", f"crash target/fuzz-artifacts/a/boom/{CRASH_NAME}"])
            self.assertEqual(x[1:8], [c.head, "x", "6", "189087", "1185", "841", "ok"])
            self.assertEqual(x[8], fuzzrun.host())
            self.assertEqual(c.rows("b")[1][0][1:8], [c.head, "y", "6", "189087", "1185", "841", "ok"])
            # A target whose build fails gets no row.
            self.assertIsNone(c.rows("c"))
            self.assertTrue((c.root / "target" / "fuzz-artifacts" / "a" / "boom" / CRASH_NAME).is_file())
            self.assertIn(f"cargo +nightly fuzz run boom {c.root / 'target' / 'fuzz-artifacts' / 'a' / 'boom' / CRASH_NAME}", p.stderr)
            self.assertIn("2 of 4 target(s) failed: a:boom c:broken", p.stderr)

            recs = {r["target"]: r for r in c.records()}
            self.assertEqual(sorted(recs), ["boom", "x", "y"])
            for r in recs.values():
                self.assertEqual(r["opts"]["max_total_time"], "7")
                self.assertFalse(Path(r["corpus"]).is_relative_to(c.root), r["corpus"])
                self.assertFalse(Path(r["corpus"]).exists(), "the corpus directory is left behind")
            self.assertEqual(recs["x"]["cwd"], str(c.root / "crates" / "a" / "fuzz"))
            # The seeds reach the corpus by copy, and the copies leave the seeds alone.
            self.assertEqual(len(recs["x"]["seeds"]), 2)
            self.assertEqual(recs["y"]["seeds"], [])
            self.assertEqual(sorted((p.name, p.read_bytes()) for p in c.seeds.iterdir()), seeds_before)

    def test_only_the_named_targets_run_and_rows_append(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = Checkout(tmp)
            p = c.run(FUZZ_TARGETS="b:y")
            self.assertEqual(p.returncode, 0, p.stderr)
            self.assertEqual([r["target"] for r in c.records()], ["y"])
            self.assertEqual(c.records()[0]["opts"]["max_total_time"], "60")
            self.assertIsNone(c.rows("a"))
            # The rows the first run appended leave the second run's commit clean.
            p = c.run(FUZZ_TARGETS="b")
            self.assertEqual(p.returncode, 0, p.stderr)
            header, rows = c.rows("b")
            self.assertEqual(header + "\n", fuzzrun.HEADER)
            self.assertEqual([r[1] for r in rows], [c.head, c.head])

    def test_a_dirty_tree_is_marked(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = Checkout(tmp)
            (c.root / "crates" / "b" / "fuzz" / "Cargo.toml").write_text(manifest("y") + "\n")
            self.assertEqual(c.run(FUZZ_TARGETS="b:y").returncode, 0)
            self.assertEqual(c.rows("b")[1][0][1], c.head + "-dirty")

    def test_a_malformed_request_runs_nothing(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = Checkout(tmp)
            for env in [{"FUZZ_TARGETS": "a:nope"}, {"FUZZ_TIME": "soon"}]:
                with self.subTest(env=env):
                    p = c.run(**env)
                    self.assertEqual(p.returncode, 2)
                    self.assertEqual(c.records(), [])
                    self.assertIsNone(c.rows("a"))


if __name__ == "__main__":
    unittest.main()
