#!/usr/bin/env python3
"""Pinned executable fixtures, no model calls or installer. Python stdlib.

Usage: python3 probe_rtk.py /absolute/rtk /absolute/output /absolute/rust-bin
Re-executes only intentionally harmless fixture commands to compare two arms.
Transformation errors never trigger a producer retry. Raw bytes are retained.
"""
import hashlib,json,os,pathlib,re,signal,subprocess,sys,time

BINARY=pathlib.Path(sys.argv[1]).resolve();OUT=pathlib.Path(sys.argv[2]).resolve();RUST=pathlib.Path(sys.argv[3]).resolve()
OUT.mkdir(parents=True,exist_ok=True);ROOT=OUT/'fixture';ROOT.mkdir(exist_ok=True)
ENV=os.environ.copy();ENV.update(XDG_CONFIG_HOME=str(OUT/'config'),XDG_DATA_HOME=str(OUT/'data'),RTK_DB_PATH=str(OUT/'tracking.db'),RTK_RECALL_DB=str(OUT/'recall.db'),RTK_TEE_DIR=str(OUT/'tee'),RTK_TELEMETRY_DISABLED='1',CLAUDE_CONFIG_DIR=str(OUT/'claude-config'),NO_COLOR='1',GIT_CONFIG_GLOBAL='/dev/null',GIT_CONFIG_NOSYSTEM='1')
ENV['PATH']=str(BINARY.parent)+os.pathsep+str(RUST)+os.pathsep+ENV.get('PATH','')
ENV['CARGO_HOME']=str(OUT/'cargo')
records=[]
def run(args,input=None,cwd=ROOT,timeout=20):
    t=time.monotonic();p=subprocess.run(args,input=input,cwd=cwd,env=ENV,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=timeout)
    return {'argv':args,'exit':p.returncode,'stdout':p.stdout.decode(errors='backslashreplace'),'stderr':p.stderr.decode(errors='backslashreplace'),'stdout_sha256':hashlib.sha256(p.stdout).hexdigest(),'elapsed_ms':round((time.monotonic()-t)*1000,3)}
def git(*a):
    r=run(['git',*a]);assert r['exit']==0,r;return r
git('init','-q');git('config','user.email','research@example.invalid');git('config','user.name','Research fixture')
(ROOT/'file space.txt').write_text('alpha\nbeta\n')
(ROOT/'old-name.txt').write_text('rename me\n')
(ROOT/'binary.dat').write_bytes(bytes(range(256)))
(ROOT/'crlf.txt').write_bytes(b'alpha\r\nbeta\r\n')
git('add','.');git('commit','-qm','fixture')
(ROOT/'file space.txt').write_text('alpha\nβeta\ngamma')
git('mv','old-name.txt','renamed-雪.txt')
(ROOT/'binary.dat').write_bytes(bytes(range(255,-1,-1)))
(ROOT/'crlf.txt').write_bytes(b'alpha\nbeta\n')
(ROOT/'space dir').mkdir(exist_ok=True)
commands=[
 'git status --porcelain=v1',
 'git diff --exit-code',
 'git diff --quiet',
 'git diff --cached --exit-code',
 'git diff --exit-code && printf should-not-run',
 'git diff --exit-code || printf expected-difference',
 "git diff -- 'file space.txt'",
 'git diff > captured.patch',
 'git diff | wc -l',
 'git status --porcelain=v1 | wc -l',
 'env LC_ALL=C git diff --exit-code',
 "cd 'space dir' && git diff --exit-code",
 "bash -c 'git diff --exit-code'",
 "printf '%s\\n' 'a b' 'x' | rg 'a b'",
 "git status $(printf -- '--short')",
]
for command in commands:
    raw=run(['bash','-c',command]);file_before=(ROOT/'captured.patch').read_bytes() if 'captured.patch' in command else None
    rewrite=run([str(BINARY),'rewrite',command]);rewritten=rewrite['stdout'] if rewrite['exit'] in (0,3) and rewrite['stdout'] else command
    actual=run(['bash','-c',rewritten]);file_after=(ROOT/'captured.patch').read_bytes() if file_before is not None else None
    records.append({'kind':'shell-equivalence','command':command,'raw':raw,'rewrite':rewrite,'actual':actual,'exit_preserved':raw['exit']==actual['exit'],'stdout_identical':raw['stdout_sha256']==actual['stdout_sha256'],'redirect_bytes_identical':None if file_before is None else file_before==file_after})

for label,left,right in [('crlf',b'x\r\n',b'x\n'),('no-final-newline',b'x\n',b'x'),('nonutf',b'\xffx',b'\xffy')]:
    a=ROOT/(label+'-a');b=ROOT/(label+'-b');a.write_bytes(left);b.write_bytes(right)
    records.append({'kind':'diff-bytes','name':label,'raw':run(['diff',str(a),str(b)]),'rtk':run([str(BINARY),'diff',str(a),str(b)])})
for name,payload,args in [('binary-passthrough',b'\xff\x00\r\n',['--passthrough']),('invalid-utf8-filter',b'\xff\x00\r\n',['--filter','git-diff']),('malformed-json',b'{broken json\n',[]),('unicode-crlf-no-final','α\r\nβ'.encode(),[])]:
    r=run([str(BINARY),'pipe',*args],input=payload)
    records.append({'kind':'pipe','name':name,'input_sha256':hashlib.sha256(payload).hexdigest(),'byte_identical':hashlib.sha256(payload).hexdigest()==r['stdout_sha256'],'result':r})

# Actual compiler/test producer: no external dependencies, no network.
crate=ROOT/'rust-fixture';(crate/'src').mkdir(parents=True,exist_ok=True)
(crate/'Cargo.toml').write_text('[package]\nname="rtk-research-fixture"\nversion="0.1.0"\nedition="2021"\n')
(crate/'src/lib.rs').write_text('pub fn value() -> usize { let unused = 7; 2 }\n#[cfg(test)] mod tests { #[test] fn deliberate_failure() { assert_eq!(super::value(), 3); } }\n')
for name,argv in [('warning-success',['check','--offline']),('test-failure',['test','--offline'])]:
    raw=run([str(RUST/'cargo'),*argv],cwd=crate,timeout=60)
    actual=run([str(BINARY),'cargo',*argv],cwd=crate,timeout=60)
    (OUT/(name+'-raw.txt')).write_text(raw['stdout']+raw['stderr'])
    text=actual['stdout']+actual['stderr'];hashes=re.findall(r'rtk recall ([0-9a-f]+)',text)
    recalls=[run([str(BINARY),'recall',h],cwd=crate) for h in hashes]
    records.append({'kind':'rust-producer','name':name,'raw':raw,'actual':actual,'exit_preserved':raw['exit']==actual['exit'],'recalls':recalls})

# A controlled long-lived producer verifies partial-output delivery and signal handling.
shim=OUT/'signal-shim';shim.mkdir(exist_ok=True);p=shim/'cargo'
p.write_text('#!/bin/sh\ni=0; while [ "$i" -lt 50 ]; do printf "   Compiling fixture-%s\\n" "$i"; i=$((i+1)); done\nprintf "error[E0308]: sentinel before signal\\n"\nprintf ready > "$RTK_RESEARCH_READY"\nexec sleep 30\n');p.chmod(0o755)
env=ENV.copy();env['PATH']=str(shim)+os.pathsep+env['PATH'];ready=OUT/'signal-ready';env['RTK_RESEARCH_READY']=str(ready)
proc=subprocess.Popen([str(BINARY),'cargo','clippy'],cwd=ROOT,env=env,stdout=subprocess.PIPE,stderr=subprocess.PIPE,start_new_session=True)
deadline=time.monotonic()+5
while not ready.exists() and time.monotonic()<deadline:time.sleep(.01)
if ready.exists():
    proc.send_signal(signal.SIGTERM)
    try:o,e=proc.communicate(timeout=5)
    except subprocess.TimeoutExpired:os.killpg(proc.pid,signal.SIGKILL);o,e=proc.communicate()
    records.append({'kind':'signal','exit':proc.returncode,'sentinel_preserved':b'E0308' in o+e,'stdout':o.decode(errors='replace'),'stderr':e.decode(errors='replace')})
else:
    os.killpg(proc.pid,signal.SIGKILL);o,e=proc.communicate();records.append({'kind':'signal','error':'producer did not become ready','stderr':e.decode(errors='replace')})

result={'binary':str(BINARY),'version':run([str(BINARY),'--version']),'generated_at_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'records':records}
(OUT/'results.json').write_text(json.dumps(result,indent=2,ensure_ascii=False))
for r in records:print(json.dumps({k:v for k,v in r.items() if k not in ['raw','rewrite','actual','rtk','result','recalls','stdout','stderr']},ensure_ascii=False))
