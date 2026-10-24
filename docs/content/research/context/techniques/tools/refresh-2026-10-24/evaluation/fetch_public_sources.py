# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
"""Inspect or explicitly fetch the pinned public source manifest; no execution."""
import argparse
import hashlib
import json
from pathlib import Path, PurePosixPath
import urllib.request

LABELS = {
    'daseinlabs/code-compression-bench': 'dasein',
    'rossoctl/context-guru': 'context-guru',
    'Entelligentsia/tokbench': 'tokbench',
    'shreyassks/headroom-benchmarks': 'headroom-benchmarks',
    'lemoncrow-lab/benchmarks': 'lemoncrow',
    'zuwasi/rtk-token-benchmark': 'rtk-current',
    'hystax/optscale-kompressor': 'kompressor',
    'JuliusBrussee/caveman': 'caveman',
    'headroomlabs-ai/headroom': 'headroom-first-party',
    'lemoncrow-lab/lemoncrow': 'lemoncrow-runtime',
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, default=Path(__file__).with_name('source-register.json'))
    parser.add_argument('--destination', type=Path, required=True)
    parser.add_argument('--fetch', action='store_true')
    args = parser.parse_args()
    rows = json.loads(args.manifest.read_text())
    checked = 0
    for row in rows:
        relative = PurePosixPath(row['path'])
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError('manifest contains an invalid relative path')
        url = f"https://raw.githubusercontent.com/{row['repo']}/{row['commit']}/{row['path']}"
        if url != row['url']:
            raise ValueError('source URL does not match pinned identity')
        destination = args.destination / LABELS[row['repo']] / relative
        if not args.fetch:
            print(json.dumps({'url': url, 'destination': str(destination), 'sha256': row['sha256']}))
            continue
        request = urllib.request.Request(url, headers={'User-Agent': 'Jackin-public-research-reproduction'})
        with urllib.request.urlopen(request, timeout=45) as response:
            data = response.read(max(row['bytes'] + 1, 2_000_001))
        if len(data) != row['bytes'] or hashlib.sha256(data).hexdigest() != row['sha256']:
            raise ValueError(f"source digest mismatch: {row['repo']}/{row['path']}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(data)
        checked += 1
    print(json.dumps({'mode': 'fetch' if args.fetch else 'plan', 'manifest_files': len(rows),
                      'downloaded_and_verified': checked, 'provider_calls': 0}))


if __name__ == '__main__':
    main()
