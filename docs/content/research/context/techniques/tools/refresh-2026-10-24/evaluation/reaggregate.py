# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
"""Recompute the audit's numeric extracts offline; never launches an agent.

The source extraction and upstream-grader probe are separate recorded operations.
This program checks arithmetic over the curated facts, not provenance or quality.
"""
import argparse
import collections
import json
import math
from pathlib import Path
import statistics


def grouped(rows, key):
    result = collections.defaultdict(list)
    for row in rows:
        result[row[key]].append(row)
    return result


def total(rows, key):
    return sum(float(row.get(key) or 0) for row in rows)


def aggregate(data):
    out = {}
    out['dasein'] = {arm: {
        'attempts': len(rows), 'tasks': len({r['instance'] for r in rows}),
        'solved': total(rows, 'success'), 'reported_usd_sum': total(rows, 'cost_usd'),
        'usd_per_solved': total(rows, 'cost_usd') / total(rows, 'success')}
        for arm, rows in grouped(data['dasein'], 'arm').items()}
    out['lemoncrow'] = {}
    for label, rows in data['lemoncrow'].items():
        out['lemoncrow'][label] = {arm: {
            'attempts': len(rr), 'tasks': len({r['task'] for r in rr}),
            'correct': sum(r.get('correct') == 'True' for r in rr),
            'ok': sum(r.get('ok') == 'True' for r in rr),
            'reported_usd_sum': total(rr, 'cost_usd'),
            'output_tokens': total(rr, 'output_tokens'),
            'judges': sorted({r.get('judge_model', '') for r in rr})}
            for arm, rr in grouped(rows, 'arm').items()}
    rows = data['rtk_payload']
    out['rtk_payload'] = {model: {
        side: sum(row['tokens'][model][side] for row in rows) for side in ('raw', 'rtk')}
        for model in rows[0]['tokens']}
    out['rtk_agents'] = {}
    for label, rows in data['rtk_agents'].items():
        out['rtk_agents'][label] = {arm: {
            'runs': len(rr), 'correct': [r['correct'] for r in rr],
            'mean_context_tokens': statistics.mean(r['context_tokens'] for r in rr),
            'mean_tool_chars': statistics.mean(r['tool_result_chars'] for r in rr),
            'reported_usd_sum': sum(r['cost_usd'] for r in rr)
                if all(r.get('cost_usd') is not None for r in rr) else None,
            'credits_sum': sum(r['credits'] for r in rr)
                if all(r.get('credits') is not None for r in rr) else None}
            for arm, rr in grouped(rows, 'side').items()}
    out['tokbench'] = {'raw_summary_mismatches': [], 'matrix': {}, 'excluded_void_input': {}}
    for arm, rows in grouped([r for r in data['tokbench'] if r['classification'] == 'matrix'], 'arm').items():
        out['tokbench']['matrix'][arm] = {
            'runs': len(rows), 'distinct_tasks': 1,
            'input_median': statistics.median(r['input'] for r in rows),
            'input_sum': sum(r['input'] for r in rows)}
    for arm, rows in grouped([r for r in data['tokbench'] if r['classification'] == 'void'], 'arm').items():
        out['tokbench']['excluded_void_input'][arm] = {
            'runs': len(rows), 'input_sum': sum(r['input'] for r in rows)}
    out['context_guru_sweep'] = {arm: {
        'rows': len(rows), 'tasks': len({r['task'] for r in rows}),
        'passed': sum(r['passed'] == 'True' for r in rows),
        'up_failed': sum(r['note'] == 'up-failed' for r in rows),
        'gateway_before': total(rows, 'gw_before'), 'gateway_saved': total(rows, 'gw_saved')}
        for arm, rows in grouped(data['context_guru_sweep'], 'config').items()}
    # Recorded upstream-function execution is not re-executed by this arithmetic check.
    out['structured_headroom_grader_probe'] = data['structured_headroom_grader_probe']
    rows = data['lemoncrow_terminal_baseline']
    out['lemoncrow_terminal_baseline'] = {
        'trials': len(rows), 'tasks': len({r['task'] for r in rows}),
        'pass': sum(r['result'] == 'pass' for r in rows),
        'fail': sum(r['result'] == 'fail' for r in rows),
        'other_reward': sum(r['result'] not in ('pass', 'fail') for r in rows),
        'missing_reported_cost': sum(r['reported_cost_usd'] is None for r in rows),
        'observed_reported_cost_sum_excludes_unknown': sum(r['reported_cost_usd'] or 0 for r in rows),
        'prompt_tokens': sum(r['prompt'] for r in rows),
        'cache_reads': sum(r['reads'] for r in rows),
        'cache_writes': sum(r['writes'] for r in rows),
        'ordinary_input': sum(r['prompt'] - r['reads'] - r['writes'] for r in rows),
        'table_input_minus_table_cache': sum(r['table_input'] - r['table_cache'] for r in rows),
        'table_cache_equals_reads_rows': sum(r['table_cache'] == r['reads'] for r in rows),
        'table_cache_equals_reads_plus_writes_rows': sum(r['table_cache'] == r['reads'] + r['writes'] for r in rows)}
    out['lemoncrow_terminal_runs'] = data['lemoncrow_terminal_runs']
    rows = data['caveman_wrap_report']
    direct, candidate = sum(r['direct'] for r in rows), sum(r['caveman'] for r in rows)
    out['caveman_wrap_report'] = {
        'cases': len(rows), 'direct_input': direct, 'caveman_input': candidate,
        'input_reduction': 1 - candidate / direct,
        'scope': 'Published report table, summed unweighted input classes, not raw-run reproduction or a price estimate.'}
    rows = data['headroom_synthetic_report']
    before, after = sum(r['before'] for r in rows), sum(r['after'] for r in rows)
    out['headroom_synthetic_report'] = {
        'scenarios': len(rows), 'before': before, 'after': after, 'reduction': 1 - after / before,
        'scope': 'First-party static synthetic tokenizer proof table, not an agent task or provider receipt.'}
    return out


def compare(actual, expected, path='root'):
    if isinstance(expected, dict):
        if actual.keys() != expected.keys():
            raise ValueError(f'{path}: key mismatch')
        for key in expected:
            compare(actual[key], expected[key], f'{path}.{key}')
    elif isinstance(expected, list):
        if len(actual) != len(expected):
            raise ValueError(f'{path}: length mismatch')
        for index, (a, e) in enumerate(zip(actual, expected)):
            compare(a, e, f'{path}[{index}]')
    elif isinstance(expected, float):
        if not math.isclose(actual, expected, rel_tol=1e-12, abs_tol=1e-12):
            raise ValueError(f'{path}: {actual} != {expected}')
    elif actual != expected:
        raise ValueError(f'{path}: {actual} != {expected}')


def main():
    root = Path(__file__).parent
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, default=root/'benchmark-rows.json')
    parser.add_argument('--expected', type=Path, default=root/'aggregation-results.json')
    args = parser.parse_args()
    result = aggregate(json.loads(args.input.read_text()))
    compare(result, json.loads(args.expected.read_text()))
    print(json.dumps({'status': 'all extracted arithmetic matches', 'sections': len(result),
                      'upstream_grader_probe': 'recorded result only; use extract_public_rows.py to execute pinned pure functions',
                      'provider_calls': 0, 'agent_runs': 0, 'paid_experiments_repeated': 0}, indent=2))


if __name__ == '__main__':
    main()
