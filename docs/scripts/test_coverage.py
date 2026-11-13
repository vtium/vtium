"""Tests of docs/scripts/coverage's reading of tier 1 scenarios (ADR-1504 D7), and of how
--update runs cargo to regenerate tools/conformance/COVERAGE.md.

Run with `python3 -I -B docs/scripts/test_coverage.py`.
"""

import contextlib
import importlib.machinery
import importlib.util
import io
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


def load():
    loader = importlib.machinery.SourceFileLoader('coverage_script', str(HERE / 'coverage'))
    spec = importlib.util.spec_from_loader('coverage_script', loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


coverage = load()

TAG = '# cov' + 'ers: DEC-1, input.legacy\n'


class Scenarios(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = Path(self.dir.name)
        (self.root / 'tools/conformance/scenarios').mkdir(parents=True)

    def tearDown(self):
        self.dir.cleanup()

    def write(self, rel, text):
        path = self.root / rel
        path.write_text(text, encoding='utf-8')
        return path

    def scan(self, rel, text, provisional=()):
        found, errors = [], []
        coverage.scan_scenario(self.write(rel, text), rel, found, errors, set(provisional))
        return found, errors

    def test_a_reviewed_scenarios_tag_names_its_file(self):
        rel = 'tools/conformance/scenarios/key-cursor.scenario'
        found, errors = self.scan(rel, '# spec: x\n' + TAG + 'size 4 1\n')
        self.assertEqual(errors, [])
        self.assertEqual(found, [('DEC-1', rel, f'{rel}:2'), ('input.legacy', rel, f'{rel}:2')])

    def test_a_provisional_scenarios_tag_is_read_but_does_not_count(self):
        rel = 'tools/conformance/scenarios/key-cursor.scenario'
        found, errors = self.scan(rel, '# spec: x\n' + TAG + 'size 4 1\n', provisional={'key-cursor'})
        self.assertEqual(errors, [])
        self.assertEqual(found, [('DEC-1', None, f'{rel}:2'), ('input.legacy', None, f'{rel}:2')])

    def test_a_capability_scenario_carries_no_tag(self):
        rel = 'tools/conformance/scenarios/da1.scenario'
        found, errors = self.scan(rel, '# spec: x\n# capability: da1\n' + TAG + 'size 4 1\n')
        self.assertEqual(found, [])
        self.assertEqual(len(errors), 1)
        self.assertIn('a capability scenario carries no covers tag', errors[0])

    def test_the_provisional_list_gives_its_names_and_skips_its_comments(self):
        self.write(coverage.SCENARIO_PROVISIONAL,
                   '# NAME<TAB>REASON\nkey-cursor\tnew\n\nmouse-sgr\tnew\n')
        errors = []
        self.assertEqual(coverage.provisional_scenarios(self.root, errors), {'key-cursor', 'mouse-sgr'})
        self.assertEqual(errors, [])
        self.assertEqual(coverage.provisional_scenarios(self.root / 'none', errors), set())

    def test_a_provisional_line_without_a_tab_and_a_reason_is_refused(self):
        self.write(coverage.SCENARIO_PROVISIONAL, 'key-cursor\nmouse-sgr\t \npaste-plain\tnew\n')
        errors = []
        self.assertEqual(coverage.provisional_scenarios(self.root, errors), {'paste-plain'})
        self.assertEqual([e.split(': ')[0] for e in errors],
                         [f'{coverage.SCENARIO_PROVISIONAL}:1', f'{coverage.SCENARIO_PROVISIONAL}:2'])

    def test_the_trees_scenarios_read_without_error(self):
        found, errors = [], []
        counts = coverage.scan(found, errors)
        self.assertEqual(errors, [])
        self.assertGreater(counts['.scenario'], 0)
        listed = coverage.provisional_scenarios(coverage.ROOT, errors)
        self.assertEqual(errors, [])
        for _, test, where in found:
            if where.startswith('tools/conformance/scenarios/'):
                name = Path(where.split(':')[0]).stem
                self.assertEqual(test is None, name in listed, where)


# A stand-in for cargo. Like the real test, it writes COVERAGE.md only when
# VTIUM_CONFORMANCE_BLESS is set, and then prints cargo's line for one passed test.
BLESS = (
    "import os, sys\n"
    "if os.environ.get('VTIUM_CONFORMANCE_BLESS'):\n"
    "    open('tools/conformance/COVERAGE.md', 'w').write('# Coverage\\n')\n"
    "print('test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 42 filtered out')\n"
    "sys.exit(0 if os.environ.get('VTIUM_CONFORMANCE_BLESS') else 101)\n"
)


class CoverageMap(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.root = Path(self.dir.name)
        (self.root / 'tools/conformance').mkdir(parents=True)
        self.map = self.root / coverage.COVERAGE_MAP
        self.map.write_text('stale\n', encoding='utf-8')

    def tearDown(self):
        self.dir.cleanup()

    def run_with(self, script):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = coverage.write_coverage_map(self.root, [sys.executable, '-c', script])
        return code, out.getvalue(), err.getvalue()

    def test_update_runs_the_test_with_the_bless_variable_and_names_the_rewritten_file(self):
        code, out, _ = self.run_with(BLESS)
        self.assertEqual(code, 0)
        self.assertEqual(self.map.read_text(encoding='utf-8'), '# Coverage\n')
        self.assertIn(f'coverage: rewrote {coverage.COVERAGE_MAP}', out)

    def test_an_unchanged_map_is_not_reported_as_rewritten(self):
        self.map.write_text('# Coverage\n', encoding='utf-8')
        code, out, _ = self.run_with(BLESS)
        self.assertEqual(code, 0)
        self.assertNotIn('rewrote', out)

    def test_a_failing_cargo_fails_update_and_its_output_is_shown(self):
        code, _, err = self.run_with("import sys; sys.stderr.write('error[E0425]: x\\n'); sys.exit(101)")
        self.assertEqual(code, 1)
        self.assertIn('error[E0425]: x', err)
        self.assertIn('cargo exited with status 101', err)
        self.assertEqual(self.map.read_text(encoding='utf-8'), 'stale\n')

    def test_a_cargo_run_that_ran_no_test_fails_update(self):
        code, _, err = self.run_with("print('test result: ok. 0 passed; 0 failed; 43 filtered out')")
        self.assertEqual(code, 1)
        self.assertIn(f'cargo ran no test named {coverage.COVERAGE_TEST}', err)

    def test_a_missing_cargo_fails_update(self):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = coverage.write_coverage_map(self.root, [str(self.root / 'no-such-cargo')])
        self.assertEqual(code, 1)
        self.assertIn('cannot run cargo', err.getvalue())


if __name__ == '__main__':
    unittest.main()
