#!/usr/bin/env python3
"""Update traceability mappings and coverage summary from scenarios."""
from __future__ import annotations
import datetime
import re
from pathlib import Path

SCENARIOS_PATH = Path('spec/00-overview/scenarios.yaml')
MATRIX_PATH = Path('spec/00-overview/traceability-matrix.yaml')
REPORT_PATH = Path('spec/00-overview/traceability-coverage.md')


def parse_scenarios(path: Path):
    lines = path.read_text().splitlines()
    scenarios = []
    current = None
    list_key = None

    for line in lines:
        m = re.match(r'^- id:\s*(\S+)\s*$', line)
        if m:
            if current:
                scenarios.append(current)
            current = {
                'id': m.group(1),
                'success_criteria': [],
                'requirements': [],
                'tests': [],
            }
            list_key = None
            continue

        m = re.match(r'^\s+(success_criteria|requirements|tests):\s*\[(.*)\]\s*$', line)
        if m:
            key = m.group(1)
            items = [i.strip() for i in m.group(2).split(',') if i.strip()]
            if current is not None:
                current[key] = items
            list_key = None
            continue

        m = re.match(r'^\s+(success_criteria|requirements|tests):\s*$', line)
        if m:
            list_key = m.group(1)
            continue

        m = re.match(r'^\s+[a-zA-Z_]+:\s*', line)
        if m:
            list_key = None
            continue

        if list_key:
            m = re.match(r'^\s+-\s*([A-Za-z0-9_-]+)\s*$', line)
            if m and current is not None:
                current[list_key].append(m.group(1))
            continue

    if current:
        scenarios.append(current)

    return scenarios


def build_req_map(scenarios):
    req_map = {}
    for sc in scenarios:
        scenario_id = sc['id']
        tests = sc.get('tests') or []
        for req_id in (sc.get('success_criteria') or []) + (sc.get('requirements') or []):
            entry = req_map.setdefault(req_id, {'scenarios': set(), 'tests': set()})
            entry['scenarios'].add(scenario_id)
            entry['tests'].update(tests)
    return req_map


def update_matrix(matrix_path: Path, req_map):
    matrix_lines = matrix_path.read_text().splitlines(keepends=True)
    output_lines = []
    current_req = None

    i = 0
    while i < len(matrix_lines):
        line = matrix_lines[i]

        m = re.match(r'^- requirement_id:\s*(\S+)\s*$', line)
        if m:
            current_req = m.group(1)
            output_lines.append(line)
            i += 1
            continue

        m = re.match(r'^(\s+)(scenario_ids|test_ids):.*$', line)
        if m:
            indent = m.group(1)
            key = m.group(2)
            mapping = req_map.get(current_req)
            if mapping:
                items = sorted(mapping['scenarios' if key == 'scenario_ids' else 'tests'])
            else:
                items = []

            if items:
                output_lines.append(f"{indent}{key}:\n")
                for item in items:
                    output_lines.append(f"{indent}  - {item}\n")
            else:
                output_lines.append(f"{indent}{key}: []\n")

            i += 1
            list_indent = indent + '  '
            while i < len(matrix_lines) and matrix_lines[i].startswith(list_indent + '- '):
                i += 1
            continue

        output_lines.append(line)
        i += 1

    matrix_path.write_text(''.join(output_lines))


def read_entries(matrix_path: Path):
    lines = matrix_path.read_text().splitlines()
    entries = []
    current = None

    def finish_entry():
        nonlocal current
        if current is not None:
            entries.append(current)
            current = None

    i = 0
    while i < len(lines):
        line = lines[i]
        if line.startswith('- requirement_id:'):
            finish_entry()
            current = {
                'id': line.split(':', 1)[1].strip(),
                'spec_source': '',
                'scenario_ids': [],
                'test_ids': [],
            }
            i += 1
            continue

        if current is None:
            i += 1
            continue

        if line.strip().startswith('spec_source:'):
            current['spec_source'] = line.split(':', 1)[1].strip()
            i += 1
            continue

        def read_list(start_index: int):
            raw = lines[start_index]
            if raw.strip().endswith('[]'):
                return [], start_index + 1
            items = []
            j = start_index + 1
            while j < len(lines):
                if lines[j].startswith('  ') and lines[j].lstrip().startswith('- '):
                    items.append(lines[j].split('-', 1)[1].strip())
                    j += 1
                    continue
                break
            return items, j

        if line.strip().startswith('scenario_ids:'):
            items, j = read_list(i)
            current['scenario_ids'] = items
            i = j
            continue

        if line.strip().startswith('test_ids:'):
            items, j = read_list(i)
            current['test_ids'] = items
            i = j
            continue

        i += 1

    finish_entry()
    return entries


def write_report(entries, report_path: Path):
    sc_entries = [e for e in entries if e['id'].startswith('SC-')]
    req_entries = [e for e in entries if e['id'].startswith('REQ-')]

    def coverage_stats(items):
        total = len(items)
        with_scen = sum(1 for e in items if e['scenario_ids'])
        with_test = sum(1 for e in items if e['test_ids'])
        return total, with_scen, with_test

    sc_total, sc_scen, sc_test = coverage_stats(sc_entries)
    req_total, req_scen, req_test = coverage_stats(req_entries)

    layer_stats = {}
    for e in req_entries:
        spec_source = e['spec_source']
        if not spec_source:
            layer = 'unknown'
        else:
            parts = spec_source.split('/')
            layer = '/'.join(parts[:2]) if len(parts) >= 2 else spec_source
        stats = layer_stats.setdefault(layer, {'total': 0, 'scen': 0, 'test': 0})
        stats['total'] += 1
        if e['scenario_ids']:
            stats['scen'] += 1
        if e['test_ids']:
            stats['test'] += 1

    lines_out = []
    lines_out.append('# Traceability Coverage Report')
    lines_out.append('')
    lines_out.append(f'Generated: {datetime.date.today().isoformat()}')
    lines_out.append('')
    lines_out.append('## Summary')
    lines_out.append('')
    lines_out.append(f'- SC entries: {sc_total} total; {sc_scen} with scenario_ids; {sc_test} with test_ids')
    lines_out.append(f'- REQ entries: {req_total} total; {req_scen} with scenario_ids; {req_test} with test_ids')
    lines_out.append('')
    lines_out.append('## REQ Coverage by Layer')
    lines_out.append('')
    for layer in sorted(layer_stats.keys()):
        stats = layer_stats[layer]
        lines_out.append(f"- {layer}: {stats['total']} total; {stats['scen']} with scenario_ids; {stats['test']} with test_ids")
    lines_out.append('')
    lines_out.append('Notes:')
    lines_out.append('- scenario_ids/test_ids indicate mapping only; they do not imply tests are implemented or passing.')
    lines_out.append('- See `spec/00-overview/traceability-matrix.yaml` for per-requirement details.')

    report_path.write_text('\n'.join(lines_out) + '\n')


def main():
    scenarios = parse_scenarios(SCENARIOS_PATH)
    req_map = build_req_map(scenarios)
    update_matrix(MATRIX_PATH, req_map)
    entries = read_entries(MATRIX_PATH)
    write_report(entries, REPORT_PATH)


if __name__ == '__main__':
    main()
