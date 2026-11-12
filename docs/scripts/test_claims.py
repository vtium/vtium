#!/usr/bin/env python3
"""Self-test for docs/scripts/claims, on small git checkouts it makes.

    python3 -I -B docs/scripts/test_claims.py

Each test writes a figure or a page, its claim file and the code the claims
rest on into a new git checkout in a scratch directory, adds them to git's
index, runs the script on that checkout with --root, and reads its exit
status and the lines it prints.
"""

import subprocess
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "claims"

RUST = """\
//! A module.
pub mod inner;
pub(crate) struct Painter {
    jobs: u32,
}
impl Painter {
    pub fn start(name: &str) -> Painter {
        Painter { jobs: 0 }
    }
}
pub enum Exit {
    Signal,
    TooQuick { count: u32 },
    Hangup(u8),
}
pub const LEAVE_WAIT: u64 = 100;
static mut COUNT: u32 = 0;
pub const fn quick() -> u64 { 2 }
pub trait Vt {
    fn wait(&mut self);
}
type Handler = u32;
"""

KEYS = """\
pub const DISPLAY_WAIT: Key = Key {
    name: "display-wait",
};
"""

UNIT = """\
[Service]
Type=notify
Restart=on-failure
"""

HEADER = "element\tfile\tsymbol\n"


def claims(*rows):
    return HEADER + "".join("\t".join(r) + "\n" for r in rows)


class Claims(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        self.tree({
            "src/app.rs": RUST,
            "crates/vtium-settings/src/keys.rs": KEYS,
            "install/vtium@.service": UNIT,
            "notes.txt": "the font helper idles out\n",
            "docs/figures/fig.svg": "<svg/>\n",
            "docs/concepts/page.md": "# Page\n",
        })

    def tearDown(self):
        self._tmp.cleanup()

    def tree(self, files):
        """Write FILES, a map of path to text, and add them to git."""
        for name, body in files.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body, encoding="utf-8")
        subprocess.run(["git", "-C", str(self.root), "add", "--", *files], check=True)

    def run_claims(self, *args):
        return subprocess.run(["python3", "-I", "-B", str(SCRIPT), "--root", str(self.root), *args],
                              capture_output=True, text=True, timeout=60)

    def findings(self, p):
        """The lines the script printed before its totals."""
        return p.stdout.splitlines()[:-1]

    def assertHolds(self, *rows, name="docs/figures/fig.claims.tsv"):
        self.tree({name: claims(*rows)})
        p = self.run_claims()
        self.assertEqual((p.returncode, self.findings(p)), (0, []), p.stdout + p.stderr)

    def assertFails(self, text, *lines, name="docs/figures/fig.claims.tsv"):
        self.tree({name: text})
        p = self.run_claims()
        self.assertEqual((p.returncode, self.findings(p)), (1, list(lines)), p.stdout + p.stderr)

    # Rust symbols

    def test_each_kind_of_rust_definition_holds(self):
        self.assertHolds(*[("a box", "src/app.rs", s) for s in
                           ["inner", "Painter", "Exit", "LEAVE_WAIT", "COUNT", "quick", "Vt", "Handler"]])

    def test_a_member_holds_as_a_fn_or_a_variant(self):
        self.assertHolds(("the thread", "src/app.rs", "Painter::start"),
                         ("a state", "src/app.rs", "Exit::Signal"),
                         ("a state", "src/app.rs", "Exit::TooQuick"),
                         ("a state", "src/app.rs", "Exit::Hangup"),
                         ("a trait method", "src/app.rs", "Vt::wait"))

    def test_a_renamed_fn_fails_with_the_claim_file_and_row(self):
        self.assertFails(claims(("the loop", "src/app.rs", "Painter"), ("the thread", "src/app.rs", "Painter::spawn")),
                         "docs/figures/fig.claims.tsv:3: src/app.rs: no fn, const or variant 'spawn' for 'Painter'")

    def test_a_name_that_appears_only_in_use_or_a_comment_fails(self):
        self.tree({"src/other.rs": "use crate::app::Painter;\n// fn Painter\nlet x = Painter::start(\"a\");\n"})
        self.assertFails(claims(("the thread", "src/other.rs", "Painter")),
                         "docs/figures/fig.claims.tsv:2: src/other.rs: no definition of 'Painter'")

    def test_a_member_of_a_type_the_file_never_names_fails(self):
        self.assertFails(claims(("the thread", "src/app.rs", "Head::start")),
                         "docs/figures/fig.claims.tsv:2: src/app.rs: no definition or impl of 'Head'")

    def test_a_member_in_an_impl_of_a_type_defined_elsewhere_holds(self):
        self.tree({"src/input.rs": "impl Terminal {\n    pub fn key(&mut self) -> bool { true }\n}\n"})
        self.assertHolds(("encodes the key", "src/input.rs", "Terminal::key"))

    def test_a_symbol_that_is_not_a_rust_name_fails(self):
        self.assertFails(claims(("the loop", "src/app.rs", "fn start")),
                         "docs/figures/fig.claims.tsv:2: src/app.rs: 'fn start' is not a Rust name or TYPE::NAME")

    # The key table, unit files and other files

    def test_a_key_holds_by_its_name_and_a_removed_key_fails(self):
        self.assertHolds(("display-wait", "crates/vtium-settings/src/keys.rs", "key display-wait"),
                         ("its const", "crates/vtium-settings/src/keys.rs", "DISPLAY_WAIT"))
        self.assertFails(claims(("the wait", "crates/vtium-settings/src/keys.rs", "key display-hold")),
                         "docs/figures/fig.claims.tsv:2: crates/vtium-settings/src/keys.rs: no key 'display-hold' in the key table")

    def test_a_unit_directive_and_its_exact_line(self):
        self.assertHolds(("notify", "install/vtium@.service", "Type"),
                         ("restart", "install/vtium@.service", "Restart=on-failure"))
        self.assertFails(claims(("restart", "install/vtium@.service", "Restart=always"),
                                ("the store", "install/vtium@.service", "FileDescriptorStoreMax")),
                         "docs/figures/fig.claims.tsv:2: install/vtium@.service: no line 'Restart=always'",
                         "docs/figures/fig.claims.tsv:3: install/vtium@.service: no directive 'FileDescriptorStoreMax='")

    def test_another_file_holds_on_plain_text(self):
        self.assertHolds(("idle", "notes.txt", "idles out"))
        self.assertFails(claims(("idle", "notes.txt", "exits at once")),
                         "docs/figures/fig.claims.tsv:2: notes.txt: no text 'exits at once'")

    # Missing files and malformed rows

    def test_a_deleted_file_fails(self):
        self.assertFails(claims(("the loop", "src/gone.rs", "Painter")),
                         "docs/figures/fig.claims.tsv:2: src/gone.rs is missing")

    def test_a_claim_file_whose_figure_or_page_is_missing_fails(self):
        self.assertHolds(("a page topic", "src/app.rs", "Painter"), name="docs/concepts/page.claims.tsv")
        self.assertFails(claims(("a box", "src/app.rs", "Painter")),
                         "docs/figures/lost.claims.tsv:0: describes nothing: lost.svg or lost.md is missing beside it",
                         name="docs/figures/lost.claims.tsv")

    def test_a_malformed_row_fails_and_the_rest_are_still_checked(self):
        self.assertFails(HEADER + "a box\tsrc/app.rs\n" + "a box\tsrc/app.rs\t\n" + "a box\tsrc/app.rs\tGone\n",
                         "docs/figures/fig.claims.tsv:2: a row has three fields, none empty; this one has 2",
                         "docs/figures/fig.claims.tsv:3: a row has three fields, none empty; this one has 3",
                         "docs/figures/fig.claims.tsv:4: src/app.rs: no definition of 'Gone'")

    def test_a_missing_header_fails(self):
        self.assertFails("a box\tsrc/app.rs\tPainter\n",
                         "docs/figures/fig.claims.tsv:1: the first row is not the header element, file, symbol")

    def test_a_claim_file_with_no_claim_fails(self):
        self.assertFails(HEADER + "# nothing yet\n",
                         "docs/figures/fig.claims.tsv:1: it holds no claim")

    def test_comments_and_blank_lines_are_skipped(self):
        self.tree({"docs/figures/fig.claims.tsv": "# the figure's claims\n\n" + claims(("a box", "src/app.rs", "Painter"))})
        p = self.run_claims()
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)

    # What it reads, and its exit codes

    def test_with_no_claim_file_it_exits_2(self):
        p = self.run_claims()
        self.assertEqual(p.returncode, 2, p.stdout + p.stderr)

    def test_an_untracked_claim_file_is_not_read(self):
        (self.root / "docs/figures/fig.claims.tsv").write_text(claims(("a box", "src/app.rs", "Gone")))
        self.assertEqual(self.run_claims().returncode, 2)

    def test_a_file_argument_reads_only_that_file(self):
        self.tree({"docs/figures/fig.claims.tsv": claims(("a box", "src/app.rs", "Gone")),
                   "docs/concepts/page.claims.tsv": claims(("a topic", "src/app.rs", "Painter"))})
        p = self.run_claims("docs/concepts/page.claims.tsv")
        self.assertEqual(p.returncode, 0, p.stdout + p.stderr)
        self.assertEqual(p.stdout.splitlines()[-1], "claims: 1 claims in 1 claim files hold")

    def test_a_failure_ends_with_its_totals(self):
        self.tree({"docs/figures/fig.claims.tsv": claims(("a box", "src/app.rs", "Gone"))})
        p = self.run_claims()
        self.assertEqual(p.stdout.splitlines()[-1], "claims: 1 problem(s) in 1 claim file(s)")


if __name__ == "__main__":
    unittest.main()
