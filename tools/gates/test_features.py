#!/usr/bin/env python3
"""Self-test for tools/gates/features and tools/gates/minspec-size, without
cargo.

    python3 -I -B tools/gates/test_features.py

The feature check's plan is read from stand-in cargo metadata. The size report
runs on stand-in binaries one byte under, at and one byte over the budget that
tools/tier3/t3_impl.py sets.
"""

import importlib.machinery
import importlib.util
import os
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load(name):
    loader = importlib.machinery.SourceFileLoader(name.replace("-", "_"), str(HERE / name))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


features = load("features")
size = load("minspec-size")


def package(name, where, feats):
    return {"name": name, "manifest_path": f"/w/{where}/{name}/Cargo.toml", "features": feats}


class Plan(unittest.TestCase):
    def test_each_crate_with_features_gets_no_feature_and_each_feature_alone(self):
        meta = {"workspace_root": "/w", "packages": [
            package("b", "crates", {"x": [], "y": ["x"]}),
            package("a", "crates", {"default": [], "z": []}),
            package("c", "crates", {}),
            package("t", "tools", {"draw": []}),
        ]}
        self.assertEqual(features.plan(meta), [
            ("a", "no features", []),
            ("a", "--features z", ["--features", "z"]),
            ("b", "no features", []),
            ("b", "--features x", ["--features", "x"]),
            ("b", "--features y", ["--features", "y"]),
        ])

    def test_a_crate_with_default_features_turns_them_off_and_also_runs_them(self):
        meta = {"workspace_root": "/w", "packages": [
            package("f", "crates", {"default": ["u"], "u": [], "v": []}),
        ]}
        self.assertEqual(features.plan(meta), [
            ("f", "no features", ["--no-default-features"]),
            ("f", "its default features", []),
            ("f", "--features u", ["--no-default-features", "--features", "u"]),
            ("f", "--features v", ["--no-default-features", "--features", "v"]),
        ])

    def test_each_run_is_clippy_and_rustdoc_with_its_flags(self):
        self.assertEqual(features.commands("a", ["--features", "z"]), [
            ("clippy", ["cargo", "clippy", "-p", "a", "--features", "z", "--all-targets", "--", "-D", "warnings"]),
            ("rustdoc", ["cargo", "doc", "--no-deps", "--lib", "-p", "a", "--features", "z"]),
        ])


class Size(unittest.TestCase):
    def report(self, n):
        with tempfile.TemporaryDirectory() as tmp:
            binary = Path(tmp) / "vtium"
            with open(binary, "wb") as f:
                f.truncate(n)
            return subprocess.run(["python3", "-I", "-B", str(HERE / "minspec-size"), str(binary)],
                                  capture_output=True, text=True, env=dict(os.environ))

    def test_the_budget_is_t3_impls(self):
        text = (HERE.parent / "tier3" / "t3_impl.py").read_text()
        self.assertIn(f"BINARY_BUDGET = {size.budget()}\n", text)

    def test_at_and_under_the_budget_the_report_warns_of_nothing(self):
        for n, margin in ((size.budget(), "0 bytes"), (size.budget() - 1, "1 byte")):
            p = self.report(n)
            self.assertEqual(p.returncode, 0, p.stderr)
            self.assertIn(f"is {n:,} bytes, {margin} under tier 3's budget", p.stdout)
            self.assertNotIn("warning", p.stdout)

    def test_over_the_budget_the_report_warns_and_still_exits_0(self):
        p = self.report(size.budget() + 1)
        self.assertEqual(p.returncode, 0, p.stderr)
        self.assertIn("1 byte over tier 3's budget", p.stdout)
        self.assertIn("warning: the minspec binary is 1 byte over tier 3's budget, so make tier3-minimal fails on it.",
                      p.stdout)

    def test_a_missing_binary_exits_2(self):
        p = subprocess.run(["python3", "-I", "-B", str(HERE / "minspec-size"), "/nonexistent/vtium"],
                           capture_output=True, text=True)
        self.assertEqual(p.returncode, 2)
        self.assertIn("cannot report the minspec binary's size", p.stderr)


if __name__ == "__main__":
    unittest.main()
