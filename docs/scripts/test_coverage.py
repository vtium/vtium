"""Tests of docs/scripts/coverage's reading of tier 1 scenarios (ADR-1504 D7).

Run with `python3 -I -B docs/scripts/test_coverage.py`.
"""

import importlib.machinery
import importlib.util
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


if __name__ == '__main__':
    unittest.main()
