#!/usr/bin/env python3
"""Same acceptance command locally and in CI; negatives never become project code."""
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import gate


class Acceptance(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.binary = gate.build_tool()

    def syntax(self, source, ok=True, test=False):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'contest.rs'
            path.write_text(source)
            result = subprocess.run([self.binary, str(path)] + (['--test-context'] if test else []),
                                    text=True, capture_output=True)
            self.assertEqual(result.returncode == 0, ok, result.stderr)
            return json.loads(result.stdout) if ok else None

    def test_crap_boundaries(self):
        self.assertEqual(gate.score(10, 1), 10)
        self.assertGreater(gate.score(10, .999), 10)
        self.assertGreater(gate.score(11, 1), 10)
        self.assertEqual(gate.score(2, 0), 6)
        self.assertEqual(gate.score(3, 0), 12)
        for cc, cov in [(0, 1), (True, 1), (2, -1), (2, 1.01), (2, float('nan')), (2, float('inf')), (2, None)]:
            with self.subTest(cc=cc, cov=cov), self.assertRaises(ValueError):
                gate.score(cc, cov)

    def test_unwrap_context(self):
        production = 'pub fn f(x: Option<u8>) -> u8 { x.unwrap() }'
        self.syntax(production, False)
        self.syntax(production + '\n#[cfg(test)] mod tests {}', False)
        self.syntax('mod contest { ' + production + ' }', False)
        self.syntax('#[cfg(test)] mod helpers {' + production + '}', True)
        self.syntax('#[test] fn t() { "1".parse::<u8>().unwrap(); }')
        self.syntax(production, True, test=True)  # Cargo test target context
        self.syntax('fn f(x: Option<u8>) -> u8 { Option::unwrap(x) }', False)
        self.syntax('fn f(x: Option<u8>) -> u8 { x.expect("required") }')
        self.syntax('fn f() { println!("{}", "1".parse::<u8>().expect("number")); }')
        self.syntax('fn f() { println!("{}", "1".parse::<u8>().unwrap()); }', False)

    def test_suppression(self):
        for attr in ['#[allow(clippy::all)]', '#[expect(clippy::unwrap_used)]',
                     '#[cfg_attr(test, allow(clippy::all))]', '#![allow(warnings)]']:
            with self.subTest(attr=attr):
                self.syntax(attr + '\nfn f() {}', False)
                self.syntax('#[cfg(test)] mod t { ' + attr + '\nfn f() {} }', False)
        self.syntax('macro_rules! m { () => { #[allow(clippy::all)] fn f() {} } }', False)

    def test_complexity(self):
        row = self.syntax('fn f(x: bool) { if x && x { } while x { } }')[0]
        self.assertEqual(row['cc'], 4)
        rows = self.syntax('fn outer() { fn inner(x: bool) { if x {} } }')
        self.assertEqual([r['cc'] for r in rows], [1, 2])
        self.assertTrue(self.syntax('fn f() { let _x = || 1; }')[0]['unsupported'])
        self.assertTrue(self.syntax('fn f() { custom!(); }')[0]['unsupported'])

    def test_missing_and_invalid_coverage(self):
        key = (str(Path('/tmp/f.rs')), 1)
        rows = {key: {'name': 'f', 'cc': 10}}
        for report in [{}, {'type': 'llvm.coverage.json.export', 'data': []}]:
            with self.assertRaises(ValueError):
                gate.evaluate(rows, report)
        report = {'type': 'llvm.coverage.json.export', 'data': [{'functions': [{
            'filenames': ['/tmp/f.rs'], 'regions': [[1, 1, 1, 10, 1, 0, 0, 0]]}]}]}
        gate.evaluate(rows, report)  # exactly 10 passes through the actual evaluator
        report['data'][0]['functions'][0]['regions'][0][4] = 0
        with self.assertRaises(ValueError):
            gate.evaluate(rows, report)
        report['data'][0]['functions'][0]['regions'][0][4] = -1
        with self.assertRaises(ValueError):
            gate.evaluate(rows, report)

    def test_config_suppression(self):
        for content in ['[lints.clippy]\nall="allow"', '[lints.rust]\nwarnings={level="allow"}',
                        '[build]\nrustflags=["-A", "warnings"]', '[build]\nrustflags=["--cap-lints=allow"]', '[build]\nrustflags=["--force-warn=warnings"]']:
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                (root / 'Cargo.toml').write_text(content)
                with self.assertRaises(ValueError):
                    gate.audit_config(root)
        with patch.dict('os.environ', {'RUSTFLAGS': '-A warnings'}), self.assertRaises(ValueError):
            gate.audit_config(gate.ROOT)

    def test_workflow_suppression(self):
        for text in ['cargo clippy -- -A warnings', 'cargo clippy -- --cap-lints allow',
                     'continue-on-error: true', 'cargo clippy || true', 'set +e', 'exit 0']:
            with self.subTest(text=text), self.assertRaises(ValueError):
                gate.audit_workflow(text)

    def test_missing_business_project(self):
        with tempfile.TemporaryDirectory() as tmp, self.assertRaises(FileNotFoundError):
            gate.check(Path(tmp) / 'Cargo.toml')

    def test_fixture_end_to_end(self):
        gate.check(gate.ROOT / 'fixtures/quality/Cargo.toml')

    def test_tool_policy_and_clippy(self):
        for path in (gate.ROOT / 'tools/quality/src').rglob('*.rs'):
            gate.run(self.binary, path)
        gate.run('cargo', 'clippy', '--locked', '--manifest-path', gate.TOOL,
                 '--all-targets', '--', '-D', 'warnings')


if __name__ == '__main__':
    unittest.main(verbosity=2)
