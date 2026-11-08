#!/usr/bin/env python3
"""Self-test for tools/stage/check, on small git repositories it makes.

    python3 -I -B tools/stage/test_check.py

Each test makes a bare repository that stands in for GitHub, and two clones of
it: one where the steering session works on integration, and one that merges
a stage pull request into main and pushes it, as GitHub does. The tests run
the script in the first clone and read its exit status and its message.
"""

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "check"


class StageCheck(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = Path(self._tmp.name)
        # The tests read no git configuration of this machine's, and sign
        # nothing.
        self.env = dict(
            os.environ,
            GIT_CONFIG_GLOBAL=os.devnull,
            GIT_CONFIG_NOSYSTEM="1",
            GIT_AUTHOR_NAME="Test",
            GIT_AUTHOR_EMAIL="test@example.invalid",
            GIT_COMMITTER_NAME="Test",
            GIT_COMMITTER_EMAIL="test@example.invalid",
        )
        self.remote = self.tmp / "remote.git"
        self.git(self.tmp, "init", "-q", "--bare", "-b", "main", str(self.remote))
        self.work = self.tmp / "work"
        self.git(self.tmp, "clone", "-q", str(self.remote), str(self.work))
        self.commit(self.work, "first")
        self.git(self.work, "push", "-q", "origin", "HEAD:main")
        self.git(self.work, "switch", "-q", "-c", "integration")
        self.commit(self.work, "batch one")

    def tearDown(self):
        self._tmp.cleanup()

    def git(self, where, *args):
        done = subprocess.run(["git", *args], cwd=where, env=self.env, capture_output=True, text=True)
        self.assertEqual(done.returncode, 0, f"git {' '.join(args)}: {done.stderr}")
        return done.stdout.strip()

    def commit(self, where, message):
        self.git(where, "commit", "-q", "--allow-empty", "-m", message)
        return self.git(where, "rev-parse", "HEAD")

    def merge_stage_on_github(self):
        """Push integration to stage, and merge stage into main in a clone
        of its own, as GitHub merges a stage pull request with a merge
        commit."""
        self.git(self.work, "push", "-q", "origin", "integration:stage")
        github = self.tmp / "github"
        self.git(self.tmp, "clone", "-q", "-b", "main", str(self.remote), str(github))
        self.git(github, "merge", "-q", "--no-ff", "-m", "Merge pull request from vtium/stage", "origin/stage")
        self.git(github, "push", "-q", "origin", "main")

    def check(self, *args):
        return subprocess.run([str(SCRIPT), *args], cwd=self.work, env=self.env, capture_output=True, text=True)

    def test_passes_when_integration_descends_from_main(self):
        done = self.check()
        self.assertEqual(done.returncode, 0, done.stderr)
        self.assertIn("descends from origin/main", done.stdout)

    def test_fails_after_a_stage_merge_until_main_is_merged_back(self):
        self.merge_stage_on_github()
        # The work clone has not fetched since main moved, so its origin/main
        # still names the first commit. The check fetches before it compares.
        self.commit(self.work, "batch two")
        done = self.check()
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        self.assertIn("does not descend from origin/main", done.stderr)
        self.assertIn("Merge origin/main into integration", done.stderr)

        self.git(self.work, "merge", "-q", "--no-ff", "-m", "Merge main into integration", "origin/main")
        done = self.check()
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_checks_the_commit_it_is_given(self):
        before = self.git(self.work, "rev-parse", "HEAD")
        self.merge_stage_on_github()
        self.git(self.work, "fetch", "-q", "origin")
        self.git(self.work, "merge", "-q", "--no-ff", "-m", "Merge main into integration", "origin/main")
        self.assertEqual(self.check("HEAD").returncode, 0)
        done = self.check(before)
        self.assertEqual(done.returncode, 1, done.stdout + done.stderr)
        self.assertIn(before[:12], done.stderr)

    def test_cannot_answer_for_a_name_that_is_no_commit(self):
        done = self.check("no-such-branch")
        self.assertEqual(done.returncode, 2, done.stdout + done.stderr)
        self.assertIn("no-such-branch names no commit", done.stderr)

    def test_cannot_answer_when_the_fetch_fails(self):
        done = self.check("--remote", "nowhere")
        self.assertEqual(done.returncode, 2, done.stdout + done.stderr)
        self.assertIn("cannot fetch main from nowhere", done.stderr)


if __name__ == "__main__":
    unittest.main()
