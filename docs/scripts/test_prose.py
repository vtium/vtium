#!/usr/bin/env python3
"""Self-test for docs/scripts/prose, on small git checkouts it makes.

    python3 -I -B docs/scripts/test_prose.py

Each test writes pages into a new git checkout in a scratch directory, adds
them to git's index, runs the script on that checkout with --root, and reads
its exit status and the lines it prints.
"""

import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "prose"


def words(n, word="word"):
    """A sentence of N words."""
    return " ".join([word] * n) + "."


SHORT = " ".join([words(10)] * 6)


class Prose(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)

    def tearDown(self):
        self._tmp.cleanup()

    def tree(self, files, track=True):
        """Write FILES, a map of path to text, and add them to git."""
        for name, body in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body, encoding="utf-8")
        if track:
            subprocess.run(["git", "-C", str(self.root), "add", "--", *files], check=True)

    def run_prose(self, *args):
        return subprocess.run(["python3", "-I", "-B", str(SCRIPT), "--root", str(self.root), *args],
                              capture_output=True, text=True, timeout=60)

    def findings(self, p):
        """The lines the script printed before its totals."""
        return p.stdout.splitlines()[:-1]

    def assertPasses(self, files):
        self.tree(files)
        p = self.run_prose("--check")
        self.assertEqual((p.returncode, self.findings(p)), (0, []), p.stdout + p.stderr)

    def assertFails(self, files, *lines):
        self.tree(files)
        p = self.run_prose("--check")
        self.assertEqual((p.returncode, self.findings(p)), (1, list(lines)), p.stdout + p.stderr)

    # Long sentences and the page's average

    def test_a_clean_page_passes(self):
        self.assertPasses({"docs/page.md": f"# Page\n\n{SHORT}\n\n- {words(12)}\n- {words(30)}\n"})

    def test_a_sentence_over_30_words_fails_with_its_line_and_count(self):
        self.assertFails({"docs/page.md": f"# Page\n\n{SHORT}\n\n{SHORT} {words(31, 'long')}\n"},
                         f"docs/page.md:5: 31 words: {words(31, 'long')}")

    def test_without_check_it_reports_and_exits_0(self):
        self.tree({"docs/page.md": f"{SHORT} {words(31, 'long')}\n"})
        p = self.run_prose()
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(self.findings(p), [f"docs/page.md:1: 31 words: {words(31, 'long')}"])
        self.assertEqual(p.stdout.splitlines()[-1],
                         "prose: 1 page read: 1 sentence over 30 words in 1 page; 0 pages average over 20 words a sentence")

    def test_a_page_whose_average_is_over_20_fails(self):
        self.assertFails({"README.md": f"{words(21)} {words(21)}\n"},
                         "README.md: average 21.0 words a sentence over 2 sentences, above 20")
        self.assertPasses({"README.md": f"{words(20)} {words(20)}\n"})

    def test_a_sentence_under_4_words_does_not_count_toward_the_average(self):
        self.assertFails({"README.md": f"{words(22)}\n\n- {words(3)}\n- {words(3)}\n- {words(3)}\n"},
                         "README.md: average 22.0 words a sentence over 1 sentence, above 20")
        self.assertPasses({"README.md": f"{words(22)}\n\n- {words(4)}\n- {words(4)}\n"})

    def test_a_sentence_in_a_wrapped_paragraph_is_reported_at_its_first_line(self):
        self.assertFails({"docs/page.md": f"{SHORT} word word\nword {words(30, 'long')}\n"},
                         f"docs/page.md:1: 33 words: word word word {words(30, 'long')}")

    # What counts as prose

    def test_code_blocks_are_not_prose(self):
        long = words(40)
        self.assertPasses({"docs/page.md": (
            f"{SHORT}\n\n```sh\n{long}\n```\n\n~~~\n{long}\n~~~\n\n"
            f"1. a step\n\n    ```\n    {long}\n    ```\n\n````md\n```\n{long}\n```\n{long}\n````\n\n{SHORT}\n")})

    # Indented code blocks, as CommonMark defines them: a block whose lines are
    # indented four spaces or a tab past the column where its container's
    # text starts. That column is 0 on the page and the item's content column
    # in a list item.

    def test_an_indented_code_block_after_a_blank_line_is_not_prose(self):
        long = words(40)
        self.assertPasses({"docs/page.md": f"{SHORT}\n\n    {long}\n    {long}\n\n{SHORT}\n"})
        self.assertPasses({"docs/page.md": f"{SHORT}\n\n\t{long}\n\n{SHORT}\n"})
        self.assertPasses({"docs/page.md": f"{SHORT}\n\n    $ make docs\n    - {long}\n\n      {long}\n\n{SHORT}\n"})

    def test_a_fence_inside_an_indented_code_block_opens_no_fence(self):
        long = words(31, "long")
        self.assertFails({"docs/page.md": f"{SHORT}\n\n    ```\n\n{SHORT} {long}\n"},
                         f"docs/page.md:5: 31 words: {long}")

    def test_a_paragraph_line_indented_four_spaces_continues_the_paragraph(self):
        self.assertFails({"docs/page.md": f"{SHORT} word word\n    word {words(30, 'long')}\n"},
                         f"docs/page.md:1: 33 words: word word word {words(30, 'long')}")
        self.assertFails({"docs/page.md": f"- {words(5)} word word\n      word {words(30, 'long')}\n"},
                         f"docs/page.md:1: 33 words: word word word {words(30, 'long')}")
        # Indented four spaces, a line that would start a heading or a
        # fence on its own still continues the paragraph.
        self.assertFails({"docs/page.md": f"{SHORT} word word\n    # word {words(30, 'long')}\n"},
                         f"docs/page.md:1: 33 words: word word # word {words(30, 'long')}")
        self.assertFails({"docs/page.md": f"{SHORT} word word\n    ```\nword {words(30, 'long')}\n"},
                         f"docs/page.md:1: 33 words: word word ``` word {words(30, 'long')}")

    def test_text_at_a_list_items_content_column_is_prose(self):
        long = words(31, "long")
        for page, line in [(f"- {words(5)}\n\n  {long}\n", 3), (f"1.  {words(5)}\n\n    {long}\n", 3),
                           (f"- {words(5)}\n  - {words(5)}\n\n    {long}\n", 4), (f"10. {words(5)}\n\n    {long}\n", 3),
                           (f"1.  {words(5)}\n\n       {long}\n", 3)]:
            with self.subTest(page=page):
                self.assertFails({"docs/page.md": page}, f"docs/page.md:{line}: 31 words: {long}")

    def test_a_block_four_columns_past_a_list_items_content_column_is_code(self):
        long = words(40)
        for page in [f"- {words(5)}\n\n      {long}\n", f"1. {words(5)}\n\n       {long}\n",
                     f"- {words(5)}\n  - {words(5)}\n\n        {long}\n", f"- {words(5)}\n\n\t  {long}\n"]:
            with self.subTest(page=page):
                self.assertPasses({"docs/page.md": page})

    def test_an_item_whose_text_starts_five_columns_past_its_marker_starts_with_code(self):
        long = words(31, "long")
        self.assertFails({"docs/page.md": f"{SHORT}\n\n-     {words(40)}\n\n  {long}\n"}, f"docs/page.md:5: 31 words: {long}")

    def test_a_list_item_ends_at_a_line_left_of_its_content_column(self):
        long = words(40)
        self.assertPasses({"docs/page.md": f"- {words(5)}\n\n{SHORT}\n\n    {long}\n"})
        self.assertFails({"docs/page.md": f"- {words(5)}\n  - {words(5)}\n\n  {words(31, 'long')}\n"},
                         f"docs/page.md:4: 31 words: {words(31, 'long')}")

    def test_a_fenced_block_inside_a_list_item_is_not_prose(self):
        long = words(31, "long")
        self.assertFails({"docs/page.md": f"- {words(5)}\n\n  ```\n  {words(40)}\n  ```\n\n  {long}\n"},
                         f"docs/page.md:7: 31 words: {long}")

    def test_an_unclosed_code_block_runs_to_the_end_of_the_page(self):
        self.assertPasses({"docs/page.md": f"{SHORT}\n\n```\n{words(40)}\n"})

    def test_tables_are_not_prose(self):
        self.assertPasses({"docs/page.md": f"{SHORT}\n\n| a | b |\n|---|---|\n| {words(40)} | {words(40)} |\n"})

    def test_headings_quotes_comments_and_frontmatter_are_not_prose(self):
        long = words(40)
        self.assertPasses({"docs/page.md": (
            f"---\ntitle: {long}\n---\n# {long}\n\n## {long}\n\n> {long}\n> {long}\n\n"
            f"<!-- {long}\n{long} -->\n\n<div>\n\n[ref]: https://example.com/{'x' * 40}\n\n---\n\n{SHORT}\n")})

    def test_each_list_item_is_its_own_sentence(self):
        self.assertPasses({"docs/page.md": "".join(f"- {' '.join(['word'] * 20)}\n" for _ in range(4))})
        self.assertPasses({"docs/page.md": "".join(f"{n}. {' '.join(['word'] * 20)}\n" for n in range(1, 5))})
        self.assertFails({"docs/page.md": f"- {words(10)}\n- {words(31, 'long')}\n  {words(5)}\n- {words(10)}\n"},
                         f"docs/page.md:2: 31 words: {words(31, 'long')}")

    def test_unpunctuated_list_items_are_sentences_of_their_own(self):
        item = " ".join(["word"] * 20)
        self.assertPasses({"docs/page.md": (
            f"A paragraph that leads in:\n- {item}\n- {item}\n\n1) {item}\n2) {item}\n\n"
            f"- {item}\n  - {item}\n  - {item}\n- `code` {item}\n- `code` {item}\n\n"
            f"* [x] {item}\n* [ ] {item}\n\n10. {item}\n11. {item}\n")})

    def test_a_sentence_ends_inside_inline_code(self):
        half = " ".join(["word"] * 15)
        self.assertPasses({"docs/page.md": f"{SHORT} {half} `Press Enter to switch to VT 12.` Word {half}.\n"})
        self.assertPasses({"docs/page.md": f"{SHORT} {half} `Is it on?` Word {half}.\n"})
        self.assertPasses({"docs/page.md": f"{SHORT} {half} (`Done!`) Word {half}.\n"})
        self.assertPasses({"docs/page.md": f"{SHORT} {half} `Done.` `make gates` runs {half}.\n"})
        self.assertPasses({"docs/page.md": f"{SHORT} {half} `Done.` vtium then {half}.\n"})

    def test_a_full_stop_inside_inline_code_before_a_lowercase_word_ends_no_sentence(self):
        half = " ".join(["word"] * 15)
        for after in ["and", "(ADR-1200"]:
            with self.subTest(after=after):
                sentence = f"{half} `Done.` {after} {half}."
                self.assertFails({"docs/page.md": f"{SHORT} {sentence}\n"}, f"docs/page.md:1: 32 words: {sentence}")

    def test_a_sentence_that_ends_inside_inline_code_is_reported_whole(self):
        long = " ".join(["long"] * 30)
        self.assertFails({"docs/page.md": f"{SHORT} {long} `Stop here.` Word {words(10)}\n"},
                         f"docs/page.md:1: 31 words: {long} `Stop here.`")

    def test_inline_code_counts_as_one_word(self):
        code = "`" + " ".join(["arg"] * 20) + ". Stop.`"
        sentence = " ".join(["word"] * 29) + f" {code}."
        self.assertPasses({"docs/page.md": f"{SHORT} {sentence}\n"})
        sentence = " ".join(["word"] * 30) + f" {code}."
        self.assertFails({"docs/page.md": f"{SHORT} {sentence}\n"}, f"docs/page.md:1: 31 words: {sentence}")

    def test_a_link_counts_its_text_and_not_its_url(self):
        link = "[the page](https://example.com/a b c d e f g h i j k l m n o p q r s t u v w x y z)"
        self.assertPasses({"docs/page.md": f"{SHORT} {' '.join(['word'] * 28)} {link}.\n"})
        self.assertFails({"docs/page.md": f"{SHORT} {' '.join(['word'] * 29)} {link}.\n"},
                         f"docs/page.md:1: 31 words: {' '.join(['word'] * 29)} the page.")

    def test_an_image_counts_nothing(self):
        image = "![" + " ".join(["alt"] * 30) + "](picture.png)"
        self.assertPasses({"docs/page.md": f"{SHORT} {words(20)} {image}\n"})

    def test_a_linked_image_counts_nothing(self):
        badges = "".join(f"[![badge {n}](https://example.com/{n}.svg)](https://example.com/{n})\n" for n in range(40))
        self.assertPasses({"README.md": f"# Title\n\n{badges}\n{SHORT}\n"})
        self.tree({"README.md": f"# Title\n\n{badges}\n{SHORT}\n"})
        self.assertEqual(self.run_prose("--table").stdout.splitlines()[-2], "   10.0      10         6       0  README.md")

    def test_dashes_separate_words(self):
        sentence = " ".join(["word"] * 15) + "—" + " ".join(["word"] * 16) + "."
        self.assertFails({"docs/page.md": f"{SHORT} {sentence}\n"}, f"docs/page.md:1: 31 words: {sentence}")

    def test_abbreviations_end_no_sentence(self):
        half = " ".join(["word"] * 15)
        for abbreviation in ["e.g.", "i.e.", "cf.", "vs.", "E.g.", "etc."]:
            with self.subTest(abbreviation=abbreviation):
                sentence = f"{half} {abbreviation} {half}."
                self.assertFails({"docs/page.md": f"{SHORT} {sentence}\n"},
                                 f"docs/page.md:1: 31 words: {sentence}")

    def test_a_sentence_ends_after_a_closing_quote_or_bracket(self):
        self.assertPasses({"docs/page.md": f'{words(20)}" {words(20)}) {words(20)}\n'})
        self.assertPasses({"docs/page.md": f"{words(20)}? {words(20)}! {' '.join(['word'] * 15)} etc. Word {words(15)}\n"})

    # The pages it reads

    def test_only_the_pages_people_read_are_read(self):
        long = f"{words(40)}\n"
        self.tree({"notes.md": long, "docs/architecture/term/ADR-1000-fixture.md": long, "docs/acceptance/a.md": long,
                   "docs/sub/page.md": long, "crates/x/src/README.md": long, "crates/x/fuzz/README.md": long,
                   "docs/page.txt": long, "README.md": long, "docs/page.md": long, "docs/concepts/c.md": long,
                   "crates/x/README.md": long})
        self.tree({"docs/untracked.md": long}, track=False)
        p = self.run_prose("--check")
        self.assertEqual(p.returncode, 1, p.stdout + p.stderr)
        read = sorted({line.split(":")[0] for line in self.findings(p)})
        self.assertEqual(read, ["README.md", "crates/x/README.md", "docs/concepts/c.md", "docs/page.md"], p.stdout)

    def test_a_generated_page_is_not_read(self):
        self.assertPasses({"docs/config.md": f"# Config\n\n<!-- Generated from keys.rs; do not edit. -->\n\n{words(40)}\n",
                           "docs/modes.md": f"# Modes\n\nGenerated from the mode table. {words(40)}\n",
                           "docs/page.md": f"{SHORT}\n"})
        self.assertFails({"docs/late.md": f"# Late\n\n\n\n\n\nGenerated from the mode table. {words(40)}\n"},
                         f"docs/late.md:7: 40 words: {words(40)}",
                         "docs/late.md: average 22.5 words a sentence over 2 sentences, above 20")

    def test_a_root_with_no_page_exits_2(self):
        p = self.run_prose()
        self.assertEqual(p.returncode, 2, p.stdout + p.stderr)
        with tempfile.TemporaryDirectory() as plain:
            p = subprocess.run(["python3", "-I", "-B", str(SCRIPT), "--root", plain], capture_output=True, text=True, timeout=60)
            self.assertEqual(p.returncode, 2, p.stdout + p.stderr)

    def test_file_arguments_read_only_those_files(self):
        self.tree({"docs/a.md": f"{words(40)}\n", "docs/b.md": f"{SHORT}\n", "notes/c.md": f"{words(40)}\n"})
        p = self.run_prose("--check", str(self.root / "docs/b.md"))
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        p = self.run_prose("--check", str(self.root / "notes/c.md"))
        self.assertEqual((p.returncode, self.findings(p)[0]), (1, f"notes/c.md:1: 40 words: {words(40)}"), p.stdout)

    def test_table_prints_a_row_for_each_page(self):
        self.tree({"docs/a.md": f"{SHORT} {words(31)}\n", "docs/b.md": f"{SHORT}\n"})
        p = self.run_prose("--table")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        lines = p.stdout.splitlines()
        self.assertEqual(lines[-4:-1], [
            "average longest sentences over 30  page",
            "   13.0      31         7       1  docs/a.md",
            "   10.0      10         6       0  docs/b.md",
        ])

    # The allow list

    def allow(self, *entries):
        return {"docs/scripts/prose-allow.tsv": "# PAGE, SENTENCE, REASON\n\n" + "".join(e + "\n" for e in entries)}

    def test_an_allowed_sentence_passes(self):
        long = words(31, "long")
        self.assertPasses({"docs/page.md": f"{SHORT} {long}\n",
                           **self.allow(f"docs/page.md\t{long}\tit quotes a message vtium prints")})
        p = self.run_prose()
        self.assertEqual(p.stdout.splitlines()[-1],
                         "prose: 1 page read: 0 sentences over 30 words in 0 pages, 1 allowed; 0 pages average over 20 words a sentence")

    def test_an_entry_allows_only_its_own_page(self):
        long = words(31, "long")
        self.assertFails({"docs/page.md": f"{SHORT}\n", "docs/other.md": f"{SHORT} {long}\n",
                          **self.allow(f"docs/page.md\t{long}\tit quotes a message")},
                         f"docs/other.md:1: 31 words: {long}",
                         "docs/scripts/prose-allow.tsv:3: this entry's sentence is not a sentence over 30 words in docs/page.md, so the entry can go")

    def test_an_entry_with_no_reason_fails(self):
        long = words(31, "long")
        self.assertFails({"docs/page.md": f"{SHORT} {long}\n", **self.allow(f"docs/page.md\t{long}\t ")},
                         f"docs/page.md:1: 31 words: {long}",
                         "docs/scripts/prose-allow.tsv:3: this entry gives no reason why its sentence may stay over 30 words")

    def test_an_entry_without_three_fields_fails(self):
        self.assertFails({"docs/page.md": f"{SHORT}\n", **self.allow("docs/page.md\tA sentence.")},
                         "docs/scripts/prose-allow.tsv:3: an entry has three fields, PAGE, SENTENCE and REASON, separated by tabs, and this one has 2")

    def test_a_stale_entry_fails_only_when_its_page_was_read(self):
        files = {"docs/page.md": f"{SHORT}\n", "docs/other.md": f"{SHORT}\n",
                 **self.allow(f"docs/page.md\t{words(31)}\tit was long once")}
        self.assertFails(files, "docs/scripts/prose-allow.tsv:3: this entry's sentence is not a sentence over 30 words in docs/page.md, so the entry can go")
        p = self.run_prose("--check", str(self.root / "docs/other.md"))
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)


if __name__ == "__main__":
    unittest.main()
