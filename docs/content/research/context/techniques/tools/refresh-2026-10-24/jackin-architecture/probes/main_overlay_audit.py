#!/usr/bin/env python3
"""Compare the frozen source audit with an explicitly selected integration commit."""
import argparse
import datetime
import hashlib
import json
import subprocess
import sys
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--repo', type=Path, required=True)
parser.add_argument('--audit', type=Path, required=True)
parser.add_argument('--commit', required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
baseline = json.loads(args.audit.read_text())
rows = []
for item in baseline['source_files']:
    data = subprocess.check_output(['git', '-C', str(args.repo), 'show', args.commit + ':' + item['path']])
    digest = hashlib.sha256(data).hexdigest()
    rows.append({'path': item['path'], 'baseline_sha256': item['sha256'], 'overlay_sha256': digest,
                 'unchanged': digest == item['sha256']})
result = {
    'reviewed_at_utc': datetime.datetime.now(datetime.timezone.utc).isoformat(),
    'baseline': baseline['commit'],
    'integration_overlay': args.commit,
    'external_release_cutoff_unchanged': '2026-10-24T01:15:00Z',
    'reason': 'origin/main advanced during research; compatibility overlay before coordinator merge',
    'command': {'argv': [sys.executable, *sys.argv], 'cwd': str(Path.cwd()), 'exit_code': 0},
    'method': 'Read-only git show exact commit; compare SHA-256 with original source audit. No checkout/source mutation, build or host execution.',
    'source_comparison': rows,
    'counts': {'unchanged': sum(row['unchanged'] for row in rows), 'changed': sum(not row['unchanged'] for row in rows)},
}
args.output.write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result['counts']))
