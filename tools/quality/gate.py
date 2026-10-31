#!/usr/bin/env python3
"""Fail-closed Rust quality gate. No third-party Python packages."""
import argparse
import json
import math
import os
import shlex
import re
from pathlib import Path
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / 'tools/quality/Cargo.toml'


def run(*args, cwd=ROOT):
    return subprocess.check_output([str(a) for a in args], cwd=cwd, text=True)


def score(cc, coverage):
    if type(cc) is not int or cc < 1:
        raise ValueError('invalid CC')
    if type(coverage) not in (int, float) or not math.isfinite(coverage) or not 0 <= coverage <= 1:
        raise ValueError('invalid coverage')
    return cc * cc * (1 - coverage) ** 3 + cc


def audit_workflow(text):
    # No shell wrappers or error-swallowing steps are part of the supported CI contract.
    forbidden = r'(?m)(?:--(?:allow|cap-lints)\b|(?:^|\s)-A|continue-on-error\s*:|\|\|\s*(?:true|:|exit\s+0)|set\s+\+e|exit\s+0)'
    if re.search(forbidden, text):
        raise ValueError('CI lint suppression or swallowed failure')


def audit_config(project):
    # These variables can inject flags/wrappers that weaken compiler checks.
    for key in set(('RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTC', 'CLIPPY_CONF_DIR', 'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER')) | {k for k in os.environ if k.endswith('_RUSTFLAGS')}:
        if os.environ.get(key):
            raise ValueError(f'unsupported compiler override: {key}')
    for path in (ROOT / ".github/workflows").glob("*.y*ml"):
        audit_workflow(path.read_text())
    paths = set()
    for base in (project, ROOT):
        paths.update(base.rglob('Cargo.toml'))
        paths.update(base.rglob('.cargo/config'))
        paths.update(base.rglob('.cargo/config.toml'))
        for parent in (base, *base.parents):
            paths.update((parent / '.cargo' / name) for name in ('config', 'config.toml'))
    cargo_home = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo'))
    paths.update(cargo_home / name for name in ('config', 'config.toml'))
    def check(value, trail=''):
        if isinstance(value, dict):
            for key, item in value.items():
                if key == 'rustflags':
                    flags = shlex.split(item) if isinstance(item, str) else item
                    if not isinstance(flags, list) or any(not isinstance(f, str) or f.startswith(('-A', '-W', '--allow', '--warn', '--force-warn', '--cap-lints')) or f.startswith('@') for f in flags):
                        raise ValueError('lint-altering rustflags')
                if key in ('rustc', 'rustc-wrapper', 'rustc-workspace-wrapper') and not (key == 'rustc-wrapper' and item == 'sccache'):
                    raise ValueError(f'compiler override in {trail}.{key}')
                if key in ('allow-unwrap-in-tests', 'allow-unwrap-in-consts'):
                    raise ValueError('Clippy exceptions are forbidden')
                if key in ('lints', 'clippy', 'rust') and isinstance(item, dict):
                    for lint in item.values():
                        if lint == 'allow' or isinstance(lint, dict) and lint.get('level') == 'allow':
                            raise ValueError('Cargo lint suppression')
                check(item, trail + '.' + key)
        elif isinstance(value, list):
            for item in value:
                check(item, trail)
    for path in paths:
        if path.is_file() and 'target' not in path.parts:
            check(tomllib.loads(path.read_text()), str(path))
    # No Clippy exception/configuration surface in this initial policy.
    for base in (ROOT, project):
        for name in ('clippy.toml', '.clippy.toml'):
            if any(base.rglob(name)):
                raise ValueError('Clippy configuration files are not permitted')


def build_tool():
    messages = run('cargo', 'build', '--locked', '--manifest-path', TOOL, '--message-format=json')
    binaries = [m['executable'] for line in messages.splitlines()
                if (m := json.loads(line)).get('executable')]
    if len(binaries) != 1:
        raise ValueError('missing quality executable')
    return binaries[0]


def inventory(manifest):
    metadata = json.loads(run('cargo', 'metadata', '--no-deps', '--format-version=1', '--manifest-path', manifest))
    files = {}
    for package in metadata['packages']:
        if package['id'] not in metadata['workspace_members']:
            continue
        for target in package['targets']:
            source = Path(target['src_path']).resolve()
            test = target['kind'] == ['test']
            candidates = {source, *source.parent.rglob('*.rs')}
            for path in candidates:
                # A file shared by a production target and test target is production.
                files[path] = files.get(path, True) and test
    if not files:
        raise ValueError('no Rust source targets')
    return files


def measured_rows(binary, files):
    rows = {}
    for path, test in files.items():
        args = [binary, path] + (['--test-context'] if test else [])
        functions = json.loads(run(*args))
        for fn in functions:
            key = (str(path), fn['start'])
            if key in rows or fn['unsupported']:
                raise ValueError(f'ambiguous/unsupported function: {key}')
            rows[key] = fn
    if not rows:
        raise ValueError('no measured functions')
    return rows


def evaluate(rows, report):
    if report.get('type') != 'llvm.coverage.json.export':
        raise ValueError('invalid LLVM coverage report')
    regions = {key: {} for key in rows}
    for data in report['data']:
        for fn in data['functions']:
            raw = fn['regions']
            if not raw:
                raise ValueError('empty function regions')
            first = raw[0]
            path = str(Path(fn['filenames'][first[5]]).resolve())
            key = (path, first[0])
            if key not in rows:
                raise ValueError(f'coverage function has no syntax metric: {key}')
            for region in raw:
                if len(region) != 8 or any(type(v) is not int or v < 0 for v in region):
                    raise ValueError('invalid LLVM region')
                if region[7] != 0:  # Count only code regions, not gap/expansion/skipped regions.
                    continue
                region_path = str(Path(fn['filenames'][region[5]]).resolve())
                identity = (region_path, *region[:4])
                regions[key][identity] = max(regions[key].get(identity, 0), region[4])
    failures = []
    for key, fn in rows.items():
        counts = regions[key]
        if not counts:
            raise ValueError(f'missing coverage: {key}')
        coverage = sum(count > 0 for count in counts.values()) / len(counts)
        value = score(fn['cc'], coverage)
        print(f'{key[0]}:{key[1]} {fn["name"]} CC={fn["cc"]} coverage={coverage:.6f} CRAP={value:.6f}')
        if value > 10:
            failures.append(key)
    if failures:
        raise ValueError(f'CRAP > 10: {failures}')


def check(manifest):
    manifest = Path(manifest).resolve(strict=True)
    audit_config(manifest.parent)
    binary = build_tool()
    files = inventory(manifest)
    rows = measured_rows(binary, files)
    run('cargo', 'clippy', '--locked', '--manifest-path', manifest,
        '--workspace', '--all-targets', '--all-features', '--', '-D', 'warnings')
    with tempfile.TemporaryDirectory(prefix='crewmoor-coverage-') as temp:
        report = Path(temp) / 'coverage.json'
        run('cargo', 'llvm-cov', '--locked', '--manifest-path', manifest,
            '--workspace', '--all-targets', '--all-features', '--json', '--output-path', report)
        evaluate(rows, json.loads(report.read_text()))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('manifest', help='business Cargo.toml; missing project/metrics fails')
    args = parser.parse_args()
    try:
        check(args.manifest)
    except (ValueError, KeyError, IndexError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f'QUALITY GATE FAILED: {error}\n')
