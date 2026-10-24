#!/usr/bin/env python3
import datetime,hashlib,json,re,subprocess
from pathlib import Path
R=Path('/workspace/scratch/1335a158b2a9/jackin');A=R/'docs/content/research/context/techniques/tools/refresh-2026-10-24/jackin-architecture'
files=sorted((R/'docs/content/research/context/engine').glob('*.mdx'))+[R/'docs/content/research/context/tools'/n for n in ['02-headroom-design.mdx','10-lemoncrow-design.mdx','11-jackin-integration.mdx']]+[R/'docs/content/research/context/techniques'/n for n in ['08-retrieval-memory-and-state.mdx','17-adoption-roadmap.mdx','18-extension-overview.mdx']]
errors=[];rows=[]
for p in files:
 s=p.read_text();parts=s.split('---',2);body=parts[2] if len(parts)==3 else s;dm=re.search(r'^description:\s*"(.*)"$',parts[1] if len(parts)==3 else '',re.M);desc=dm.group(1) if dm else ''
 row={'path':p.relative_to(R).as_posix(),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'body_lines':len(body.strip().splitlines()),'description_characters':len(desc),'research_state_current':'**Research state:** Current' in body};rows.append(row)
 if not desc or len(desc)>160 or desc[-1] not in '.!?':errors.append((row['path'],'description'))
 if row['body_lines']>400:errors.append((row['path'],'body-lines'))
 if not row['research_state_current']:errors.append((row['path'],'research-state'))
 for ref in re.findall(r'<RepoFile\s+path="([^"]+)"',s):
  if not (R/ref).exists():errors.append((row['path'],'missing RepoFile',ref))
 for link in re.findall(r'\]\((/research/[^)#]+)(?:#[^)]*)?\)',s):
  route=link.rstrip('/');base=R/'docs/content'/route.lstrip('/')
  if not any(q.exists() for q in [base.with_suffix('.mdx'),base/'index.mdx',base]):errors.append((row['path'],'missing route',link))
for fn in ['coverage-dispositions.json','integration-coverage-dispositions.json']:
 j=json.loads((A/fn).read_text())
 for row in j['headings']:
  p=R/row['replacement_document'];heads=re.findall(r'^#{2,6} (.+)$',p.read_text(),re.M)
  if row['replacement_heading'] not in heads:errors.append((fn,'missing replacement heading',row['replacement_heading']))
jsonfiles=list(A.rglob('*.json'))
for p in jsonfiles:
 try:json.loads(p.read_text())
 except Exception as e:errors.append((p.relative_to(A).as_posix(),'JSON',str(e)))
args=['git','-C',str(R),'diff','--check','--']+[p.relative_to(R).as_posix() for p in files]+[A.relative_to(R).as_posix()]
check=subprocess.run(args,capture_output=True,text=True)
if check.returncode:errors.append(('git diff --check',check.stdout,check.stderr))
result={'executed_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'method':'Static owned-file structure, internal route existence, RepoFile existence, coverage-target existence, JSON parsing and git whitespace check. No rendered browser or repository-wide gate is implied.','command':{'argv':['python3','research/jackin-architecture/check_integration_docs.py'],'cwd':str(R.parent)},'documents':rows,'json_files_parsed':len(jsonfiles),'coverage_ledgers_validated':2,'git_diff_check_exit':check.returncode,'errors':errors,'passed':not errors}
(A/'integration-documentation-checks.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'documents':len(files),'json_files':len(jsonfiles),'max_body_lines':max(r['body_lines'] for r in rows),'max_description':max(r['description_characters'] for r in rows),'passed':not errors,'errors':errors},indent=2))
raise SystemExit(bool(errors))
