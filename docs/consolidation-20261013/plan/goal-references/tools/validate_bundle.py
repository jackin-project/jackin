#!/usr/bin/env python3
"""Validate planning bundle structure only; does not execute Jackin or prove semantic coverage.
Python 3.10+, standard library. Read-only; emits JSON on stdout and returns 0/1.
"""
from __future__ import annotations
import argparse
from collections import Counter
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import sys
from urllib.parse import unquote, urlsplit

SHA = re.compile(r'^[0-9a-f]{40}$')
STATES = {'PENDING','IN_PROGRESS','IN_REVIEW','BLOCKED','FAILED','VERIFIED','DONE'}
DISPOSITIONS = {'KEEP','ADAPT','COMBINE','SUPERSEDED','REJECT','DEFER'}
REFS = ['00-repository-baseline','01-branch-inventory','02-change-coverage-matrix',
        '03-overlaps-and-conflicts','04-target-architecture','05-execution-tasks',
        '06-validation-and-test-plan','07-risks-and-rollback','08-review-and-decision-log',
        '09-evidence-and-limitations','10-artifact-integrity','INDEX']


def prose(text: str) -> str:
    return re.sub(r'^```[^\n]*\n.*?^```\s*$', '', text, flags=re.M | re.S)


def anchors(text: str) -> set[str]:
    result = set(re.findall(r'<a\s+id=["\']([^"\']+)["\']', text))
    counts: Counter = Counter()
    for heading in re.findall(r'^#{1,6}\s+(.+?)\s*#*$', prose(text), re.M):
        slug = re.sub(r'[^\w\- ]', '', heading.lower()).replace(' ', '-')
        n = counts[slug]; counts[slug] += 1
        result.add(slug + (f'-{n}' if n else ''))
    return result


def validate(root: Path) -> dict:
    root = root.resolve(); errors: list[str] = []
    required = ['GOAL.md','GOAL_PROMPT.md','README.md'] + [f'goal-references/{s}.md' for s in REFS]
    required += ['goal-references/tools/README.md','goal-references/tools/collect_git_evidence.py',
                 'goal-references/tools/test_collector.py','goal-references/tools/validate_bundle.py']
    for name in required:
        if not (root/name).is_file(): errors.append(f'Missing required file: {name}')
    e = root/'goal-references/evidence'
    try:
        branches = json.loads((e/'remote-branches.json').read_text())['branches']
        obs = json.loads((e/'observations.json').read_text())
        decisions = json.loads((e/'decisions.json').read_text())
        tasks = json.loads((e/'tasks.json').read_text())
    except (OSError, ValueError, KeyError, TypeError) as exc:
        return {'status':'FAILED','errors':errors + [f'Invalid/missing normalized evidence: {exc}']}
    for field in ['id','ref','name','dossier']:
        values=[b.get(field) for b in branches]
        if len(values)!=len(set(values)) or any(not v for v in values):
            errors.append(f'Duplicate or empty branch {field}')
    by_b={b['id']:b for b in branches}; by_t={t['id']:t for t in tasks}
    if len(by_t)!=len(tasks):errors.append('Duplicate task IDs')
    if len({d['id'] for d in decisions})!=len(decisions):errors.append('Duplicate change IDs')
    for b in branches:
        if not SHA.fullmatch(b.get('tip','')): errors.append(f'Invalid full tip: {b["id"]}')
        if b['ref']!='refs/heads/'+b['name']:errors.append(f'Ref/name mismatch: {b["id"]}')
        dossier = root/'goal-references'/b['dossier']
        if not dossier.is_file():errors.append(f'Missing dossier: {b["id"]}');continue
        text=dossier.read_text()
        for value in [b['ref'],b['tip'],b['id']]:
            if value not in text:errors.append(f'Dossier missing identity {value}')
        if b['tip'] not in obs:errors.append(f'Tip has no observation: {b["id"]}')
    for sha, row in obs.items():
        if not SHA.fullmatch(sha):errors.append(f'Invalid observation SHA: {sha}')
        for field in ['title','analysis_status','findings','sources','independent_review']:
            if field not in row:errors.append(f'Observation {sha} missing {field}')
    fields=['id','title','objective','source_branches','files','dependencies','wave','role',
            'procedure','acceptance','commands','conflicts','rollback','state','review','evidence']
    for t in tasks:
        for field in fields:
            if field not in t:errors.append(f'{t["id"]} missing field {field}')
            elif field not in ['dependencies','wave'] and not t[field]:errors.append(f'{t["id"]} empty field {field}')
        if t.get('state') not in STATES:errors.append(f'{t["id"]} invalid state')
        for bid in t.get('source_branches',[]):
            if bid!='ALL' and bid not in by_b:errors.append(f'{t["id"]} unknown source {bid}')
        for dep in t.get('dependencies',[]):
            if dep not in by_t:errors.append(f'{t["id"]} missing dependency {dep}')
            elif by_t[dep]['wave']>=t['wave']:errors.append(f'{t["id"]} nonpreceding dependency {dep}')
    visiting:set[str]=set(); done:set[str]=set()
    def visit(tid):
        if tid in visiting:errors.append(f'Dependency cycle at {tid}');return
        if tid in done:return
        visiting.add(tid)
        for dep in by_t[tid].get('dependencies',[]):
            if dep in by_t:visit(dep)
        visiting.remove(tid);done.add(tid)
    for tid in by_t:visit(tid)
    for d in decisions:
        if d.get('disposition') not in DISPOSITIONS:errors.append(f'{d["id"]} invalid disposition')
        if d.get('task') not in by_t:errors.append(f'{d["id"]} unknown task')
        tips=set()
        for bid in d.get('branches',[]):
            if bid not in by_b:errors.append(f'{d["id"]} unknown branch {bid}')
            else:tips.add(by_b[bid]['tip'])
        if set(d.get('sources',[]))!=tips:errors.append(f'{d["id"]} source SHAs do not match branch IDs')
        for sha in d.get('sources',[]):
            if not SHA.fullmatch(sha):errors.append(f'{d["id"]} invalid source SHA')
        for field in ['behavior','alternative','rationale','tests','risk','approval']:
            if not d.get(field):errors.append(f'{d["id"]} missing {field}')
    paths=list(root.rglob('*.md')); internal=0;external=0;cache={}
    for md in paths:
        text=md.read_text()
        # Bundle links use ordinary destinations without unescaped nested parentheses.
        for match in re.finditer(r'(?<!!)\[[^\]\n]*\]\((<[^>]+>|[^)\s]+)(?:\s+"[^"]*")?\)',prose(text)):
            dest=match.group(1).strip('<>');u=urlsplit(dest)
            if u.scheme or u.netloc:external+=1;continue
            internal+=1
            target=(md.parent/unquote(u.path)).resolve() if u.path else md.resolve()
            if not target.is_relative_to(root):errors.append(f'{md.relative_to(root)}: link escapes bundle: {dest}');continue
            if not target.exists():errors.append(f'{md.relative_to(root)}: missing link: {dest}');continue
            if u.fragment and target.suffix.lower()=='.md':
                if target not in cache:cache[target]=anchors(target.read_text())
                if unquote(u.fragment) not in cache[target]:errors.append(f'{md.relative_to(root)}: missing anchor: {dest}')
    goal=(root/'GOAL.md').read_text() if (root/'GOAL.md').exists() else ''
    for n in range(1,10):
        if not re.search(rf'^## {n}\. ',goal,re.M):errors.append(f'GOAL missing numbered section {n}')
    prompt=(root/'GOAL_PROMPT.md').read_text() if (root/'GOAL_PROMPT.md').exists() else ''
    if not prompt.startswith('/goal\n'):errors.append('GOAL_PROMPT must start with /goal')
    taskdoc=(root/'goal-references/05-execution-tasks.md').read_text() if (root/'goal-references/05-execution-tasks.md').exists() else ''
    matrix=(root/'goal-references/02-change-coverage-matrix.md').read_text() if (root/'goal-references/02-change-coverage-matrix.md').exists() else ''
    for tid in by_t:
        if f'### {tid} —' not in goal or f'### {tid} —' not in taskdoc:errors.append(f'Missing full task card: {tid}')
    for d in decisions:
        if f'id="{d["id"].lower()}"' not in matrix:errors.append(f'Missing change anchor: {d["id"]}')
    return {'generated_at_utc':datetime.now(timezone.utc).isoformat(),
            'status':'PASSED' if not errors else 'FAILED',
            'scope':'Planning artifact structure only; not GitHub fact verification, Jackin tests or independent semantic review.',
            'counts':{'markdown_files':len(paths),'branches':len(branches),
                      'distinct_tips':len({b['tip'] for b in branches}),'observations':len(obs),
                      'branch_dossiers':len(list((root/'goal-references/branches').glob('*.md'))),
                      'tasks':len(tasks),'seed_decisions':len(decisions),
                      'internal_links_checked':internal,'external_links_not_fetched':external},
            'errors':errors}


def main()->int:
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--root',type=Path,default=Path.cwd());args=ap.parse_args()
    try:report=validate(args.root)
    except (OSError, ValueError, TypeError, KeyError) as exc:
        report={'status':'FAILED','errors':[f'Validation error: {exc}']}
    print(json.dumps(report,indent=2))
    return 0 if report['status']=='PASSED' else 1
if __name__=='__main__':raise SystemExit(main())
