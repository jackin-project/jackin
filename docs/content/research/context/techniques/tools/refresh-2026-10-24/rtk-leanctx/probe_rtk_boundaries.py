#!/usr/bin/env python3
"""Compare distinct RTK stdin diff surfaces, plus missing/oversize recovery.

Usage: python3 probe_rtk_boundaries.py /absolute/rtk /absolute/output-dir
Fixtures are local Git producer output and explicit malformed variants.
No global configuration, model call, or producer retry is performed.
"""
import hashlib, json, os, pathlib, subprocess, sys, time

binary = pathlib.Path(sys.argv[1]).resolve()
out = pathlib.Path(sys.argv[2]).resolve()
out.mkdir(parents=True, exist_ok=True)
root = out / 'fixture'
root.mkdir(exist_ok=True)
env = os.environ.copy()
env.update(XDG_CONFIG_HOME=str(out/'config'), XDG_DATA_HOME=str(out/'data'),
           RTK_DB_PATH=str(out/'tracking.db'), RTK_RECALL_DB=str(out/'recall.db'),
           RTK_TELEMETRY_DISABLED='1', GIT_CONFIG_GLOBAL='/dev/null', GIT_CONFIG_NOSYSTEM='1', NO_COLOR='1')
def run(args, data=None):
    start = time.monotonic()
    r = subprocess.run(args, input=data, cwd=root, env=env, capture_output=True, timeout=15)
    return {'exit':r.returncode, 'stdout':r.stdout.decode(errors='backslashreplace'),
            'stderr':r.stderr.decode(errors='backslashreplace'),
            'stdout_sha256':hashlib.sha256(r.stdout).hexdigest(),
            'elapsed_ms':round((time.monotonic()-start)*1000,3)}
def git(*args):
    r = run(['git',*args]); assert r['exit']==0,r;return r
git('init','-q'); git('config','user.email','research@example.invalid'); git('config','user.name','Research fixture')
f = root/'header-like.txt'
f.write_text('--- old sentinel\n+++ old second\n@@ old third\nunchanged\n')
git('add','.'); git('commit','-qm','fixture')
f.write_text('--- new sentinel\n+++ new second\n@@ new third\nunchanged')
patch = git('diff')['stdout'].encode()
variants = [('producer-patch', patch), ('truncated-hunk', patch.rsplit(b'\n',3)[0]),
            ('invalid-utf8', b'\xff\x00\r\n'),
            ('trailing-no-newline', b'not a patch\r\nlast'),
            ('over-cap', b'x'*(10*1024*1024+1))]
records=[]
for name,payload in variants:
    for family,args in [('diff-stdin',['diff','-']),('git-diff-filter',['pipe','--filter','git-diff'])]:
        result=run([str(binary),*args],payload)
        record={'name':name,'surface':family,'input_bytes':len(payload),'input_sha256':hashlib.sha256(payload).hexdigest(),
                'byte_identical':hashlib.sha256(payload).hexdigest()==result['stdout_sha256'],
                'has_no_newline_marker':'No newline at end of file' in result['stdout'],**result}
        # Avoid recording the >10 MiB rejection fallback payload in git artifacts.
        if len(record['stdout'])>20000:
            record['stdout_bytes']=len(record['stdout'].encode());record['stdout']=record['stdout'][:200]+' [artifact excerpt; SHA-256 retained]'
        records.append(record)
records.append({'name':'missing-recall','surface':'recall',**run([str(binary),'recall','000000000000'])})
data={'binary':str(binary),'generated_at_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'producer_patch':patch.decode(),'records':records}
(out/'results.json').write_text(json.dumps(data,indent=2,ensure_ascii=False))
for r in records:print(json.dumps({k:v for k,v in r.items() if k not in ['stdout','stderr']},ensure_ascii=False))
