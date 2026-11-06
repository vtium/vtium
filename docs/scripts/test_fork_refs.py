#!/usr/bin/env python3
"""Self-test for docs/scripts/fork-refs, on small git checkouts it makes.

    python3 -I -B docs/scripts/test_fork_refs.py

Each test writes files into a new git checkout in a scratch directory, adds
them to git's index, runs the script on that checkout with --root, and reads
its exit status and the `file:line:` lines it prints.
"""

import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "fork-refs"


class ForkRefs(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)

    def tearDown(self):
        self._tmp.cleanup()

    def tree(self, files, track=True):
        """Write FILES, a map of path to text or bytes, and add them to git."""
        for name, body in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            if isinstance(body, bytes):
                path.write_bytes(body)
            else:
                path.write_text(body, encoding="utf-8")
        if track:
            subprocess.run(["git", "-C", str(self.root), "add", "--", *files], check=True)

    def run_check(self, *files):
        return subprocess.run(["python3", "-I", "-B", str(SCRIPT), "--root", str(self.root), *files],
                              capture_output=True, text=True, timeout=60)

    def places(self, p):
        """The `file:line` of each hit the script printed."""
        return [":".join(line.split(":")[:2]) for line in p.stdout.splitlines()]

    def assertClean(self, files):
        self.tree(files)
        p = self.run_check()
        self.assertEqual((p.returncode, p.stdout.splitlines()[-1:]), (0, [f"fork-refs: {len(self.listed())} files name no C fork"]), p.stdout + p.stderr)

    def assertFound(self, files, *places):
        self.tree(files)
        p = self.run_check()
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        self.assertEqual(self.places(p), list(places), p.stdout)

    def listed(self):
        p = subprocess.run(["git", "-C", str(self.root), "ls-files"], capture_output=True, text=True, check=True)
        return p.stdout.splitlines()

    # The tree

    def test_a_clean_tree_passes(self):
        self.assertClean({
            "README.md": "# VTium\n\nvtium answers to TERM=kmscon, the released kmscon's name.\n",
            "COPYING": "Copyright (c) the kmscon and libtsm contributors\n",
            "src/lib.rs": "// libtsm's parser table, ABC reference, libc fork()\n// the C\n\n// reference stays two paragraphs apart\n",
            "crates/x/README.md": "# x\n\n## What vtium does\n\nkmscon#N is the form of a fork issue.\n",
        })

    def test_a_root_where_git_lists_no_file_exits_2(self):
        p = self.run_check()
        self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
        with tempfile.TemporaryDirectory() as plain:
            p = subprocess.run(["python3", "-I", "-B", str(SCRIPT), "--root", plain], capture_output=True, text=True, timeout=60)
            self.assertEqual(p.returncode, 2, p.stdout + p.stderr)

    def test_only_tracked_text_files_are_read(self):
        self.tree({"notes.txt": "the C reference\n"}, track=False)
        self.assertClean({"blob.bin": b"\0the C reference\n", "ok.txt": "vtium\n"})

    def test_file_arguments_read_only_those_files(self):
        self.tree({"a.txt": "the C fork\n", "b.txt": "vtium\n"})
        p = self.run_check(str(self.root / "b.txt"))
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        p = self.run_check(str(self.root / "a.txt"))
        self.assertEqual((p.returncode, self.places(p)), (1, ["a.txt:1"]), p.stdout)

    # What it looks for

    def test_each_phrase_fails_with_its_file_and_line(self):
        for phrase in ["C reference", "C fork", "kmscon fork", "libtsm fork", "c REFERENCE", "Kmscon Fork",
                       "C reference's", "C references", "C forks", "C  reference"]:
            with self.subTest(phrase=phrase):
                self.assertFound({"docs/page.md": f"# Page\n\nvtium does this, as the {phrase} did.\n"}, "docs/page.md:3")

    def test_the_printed_line_names_what_it_found(self):
        self.tree({"a.rs": "x\n// as the C\n// reference did\n"})
        p = self.run_check()
        self.assertEqual(p.stdout.splitlines(), ["a.rs:2: found 'C reference': // as the C"])

    def test_a_phrase_split_across_a_line_break_fails_at_its_first_line(self):
        for text in ["// as the C\n// reference did\n", "//! as the C\n//! reference did\n",
                     "    /// as the C \n    /// reference did\n", "# as the C\n# reference did\n",
                     " * as the C\n * reference did\n", "-- as the kmscon\n-- fork did\n",
                     "; as the libtsm\n; fork did\n", "> the C\n> fork\n", "as the C\nreference did\n"]:
            with self.subTest(text=text):
                self.assertFound({"a.txt": "first\n" + text}, "a.txt:2")

    def test_words_that_only_contain_a_phrase_pass(self):
        self.assertClean({"a.txt": "ABC reference\nlibc fork()\nthe C\n\nreference\nkmscon forked\nC referee\n"})

    def test_each_repository_fails(self):
        for text in ["aaronsb/kmscon", "aaronsb/libtsm", "AaronSB/KMSCON",
                     "https://github.com/aaronsb/libtsm/pull/3", "`git clone aaronsb/kmscon.git`"]:
            with self.subTest(text=text):
                self.assertFound({"a.md": f"x\n{text}\n"}, "a.md:2")

    def test_each_fork_issue_form_fails(self):
        for text in ["kmscon#12", "libtsm#79", "Libtsm#7", "(kmscon#3)"]:
            with self.subTest(text=text):
                self.assertFound({"a.md": f"x\nsee {text}\n"}, "a.md:2")

    def test_other_repositories_and_plain_names_pass(self):
        self.assertClean({"a.md": "kmscon/kmscon, `kmscon/libtsm`, vtium/vtium#47, kmscon, libtsm, kmscon#N\n"})

    def test_each_departure_heading_fails_in_a_crate_readme(self):
        for heading in ["## Departures", "## Golden differences", "## C behaviour kept", "## C behavior kept",
                        "## C notes", "### Departures from xterm", "## departures"]:
            with self.subTest(heading=heading):
                self.assertFound({"crates/vtium-x/README.md": f"# vtium-x\n\n{heading}\n\ntext\n"}, "crates/vtium-x/README.md:3")

    def test_the_beyond_heading_fails_once_in_a_crate_readme(self):
        self.tree({"crates/vtium-x/README.md": "# vtium-x\n\n## Beyond the C reference\n"})
        p = self.run_check()
        self.assertEqual((p.returncode, p.stdout.splitlines()), (1, [
            "crates/vtium-x/README.md:3: found '## Beyond the C reference': ## Beyond the C reference",
            "crates/vtium-x/README.md:3: found 'C reference': ## Beyond the C reference",
        ]))

    def test_the_headings_pass_outside_a_crate_readme_and_away_from_a_heading(self):
        text = "# x\n\n## Departures\n\n## Golden differences\n\n## C notes\n\nDepartures, C notes\n"
        self.assertClean({"README.md": text, "docs/crates.md": text, "crates/vtium-x/NOTES.md": text,
                          "crates/vtium-x/README.md": "# x\n\nDepartures from a plan\n#### What departs\n"})

    # The skip list

    def test_agents_and_claude_md_are_skipped_at_the_root_only(self):
        text = "the C reference, aaronsb/kmscon, libtsm#79\n"
        self.assertFound({"AGENTS.md": text, "CLAUDE.md": text, "docs/AGENTS.md": text, "crates/x/CLAUDE.md": text},
                         "crates/x/CLAUDE.md:1", "crates/x/CLAUDE.md:1", "crates/x/CLAUDE.md:1",
                         "docs/AGENTS.md:1", "docs/AGENTS.md:1", "docs/AGENTS.md:1")

    def test_the_records_keep_the_phrases_and_their_body_still_fails_on_the_forms(self):
        self.assertFound({"docs/architecture/term/a-record.md":
                          "---\nkind: decision\n---\n\n# the C reference\n\nthe C fork kept aaronsb/kmscon and libtsm#79\n",
                          "docs/architecture/INDEX.md": "- names no C fork\n"},
                         "docs/architecture/term/a-record.md:7", "docs/architecture/term/a-record.md:7")

    def test_the_records_frontmatter_said_and_via_lines_are_skipped(self):
        self.assertFound({"docs/architecture/platform/b-record.md":
                          "---\nbasis:\n  - operator: aaronsb\n    said: \"The aaronsb/kmscon fork's commits\"\n"
                          "    via: operator, on the aaronsb/libtsm fork and kmscon#3\n"
                          "  - upstream: \"aaronsb/kmscon b1f7a7c5\"\n---\n\nbody\nsaid: aaronsb/kmscon\n"},
                         "docs/architecture/platform/b-record.md:6", "docs/architecture/platform/b-record.md:10")

    def test_said_and_via_outside_the_records_are_not_skipped(self):
        self.assertFound({"docs/notes.md": "---\nsaid: aaronsb/kmscon\n---\n"}, "docs/notes.md:2")

    def test_the_d8_line_of_adr_1503_is_skipped_and_nothing_past_it(self):
        line = "`docs/scripts/fork-refs` fails when a committed file names the fork: `aaronsb/kmscon` and `aaronsb/libtsm`\n"
        self.assertFound({"docs/architecture/project/ADR-1503-x.md": f"# x\n\n{line}\nsee aaronsb/kmscon\n",
                          "docs/architecture/project/ADR-1502-x.md": line},
                         "docs/architecture/project/ADR-1502-x.md:1", "docs/architecture/project/ADR-1502-x.md:1",
                         "docs/architecture/project/ADR-1503-x.md:5")

    def test_a_ported_files_copyright_notice_is_skipped_and_nothing_past_it(self):
        self.assertFound({"crates/vtium-font/src/sprite.rs":
                          "//! Copyright (c) 2026 the kmscon fork authors\n//! the kmscon fork drew it\n",
                          "src/other.c": " * Copyright (c) 2025-2026 the kmscon fork authors\n",
                          "src/more.rs": "// Copyright (c) 2026 the kmscon fork authors and aaronsb/kmscon\n"},
                         "crates/vtium-font/src/sprite.rs:2", "src/more.rs:1", "src/more.rs:1")

    def test_the_font_box_line_of_copying_is_skipped_and_nothing_past_it(self):
        line = "adapted from Ghostty's sprite font, by way of the C fork's font_box.c, under\n"
        self.assertFound({"COPYING": f"x\n{line}the C reference's entry\n", "LICENSE": line},
                         "COPYING:3", "LICENSE:1")

    def test_the_script_and_its_tests_are_skipped_and_nothing_beside_them(self):
        text = "the C reference\n"
        self.assertFound({"docs/scripts/fork-refs": text, "docs/scripts/test_fork_refs.py": text,
                          "docs/scripts/issue-refs": text}, "docs/scripts/issue-refs:1")


if __name__ == "__main__":
    unittest.main()
