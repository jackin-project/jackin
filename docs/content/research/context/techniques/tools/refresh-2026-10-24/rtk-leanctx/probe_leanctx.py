#!/usr/bin/env python3
"""Offline MCP fixtures. No agent/model, provider, installer or global config.

Usage: python3 probe_leanctx.py /absolute/lean-ctx /absolute/output-dir
Only directory-scoped research data are written. Results describe fixture
behavior, not task quality, billing or host activation. Uses Python stdlib.
"""
import json, os, pathlib, queue, subprocess, sys, threading, time

BINARY = pathlib.Path(sys.argv[1]).resolve()
OUT = pathlib.Path(sys.argv[2]).resolve()
OUT.mkdir(parents=True, exist_ok=True)
ROOT = OUT / 'fixture-repo'
ROOT.mkdir(exist_ok=True)
subprocess.run(['git', 'init', '-q', str(ROOT)], check=True)
SOURCE = ''.join(f'pub fn function_{i}() -> usize {{\n' + ''.join(f'    let v{j} = {i+j};\n' for j in range(12)) + '    v0\n}\n' for i in range(1, 21))
(ROOT / 'fixture.rs').write_text(SOURCE)
(ROOT / 'exact.txt').write_bytes('alpha\r\nβeta\r\nno-final-newline'.encode())
for args in [('config','user.email','research@example.invalid'),('config','user.name','Research fixture'),('add','.'),('commit','-qm','fixture'),('branch','baseline'),('checkout','-qb','changed')]:
    subprocess.run(['git',*args],cwd=ROOT,check=True)
(ROOT/'fixture.rs').write_text(SOURCE.replace('function_1()', 'changed_1()'))
subprocess.run(['git','commit','-qam','branch fixture'],cwd=ROOT,check=True)
subprocess.run(['git','checkout','-q','baseline'],cwd=ROOT,check=True)

class MCP:
    def __init__(self, name, *, scope=None, claude=False, policy='aggressive', shared=None):
        self.name=name; self.events=[]; self.seq=0; self.q=queue.Queue()
        self.state=shared or OUT / name
        self.state.mkdir(parents=True,exist_ok=True)
        config='''proxy_enabled = false
rules_injection = "off"
minimal_overhead = true
[setup]
auto_inject_rules = false
auto_inject_skills = false
auto_update_mcp = false
manage_hooks = false
[telemetry]
enabled = false
preference = "explicitly_disabled"
[cloud]
contribute_enabled = false
sync_stats_enabled = false
sync_gain_enabled = false
sync_models_enabled = false
'''
        (self.state/'config.toml').write_text(config)
        env={k:v for k,v in os.environ.items() if not k.startswith(('LEAN_CTX','LCTX','CLAUDE','CURSOR','CODEX_THREAD'))}
        env.update({f'LEAN_CTX_{k}_DIR':str(self.state) for k in ['CONFIG','DATA','STATE','CACHE']})
        env.update(LEAN_CTX_HEADLESS='1',LEAN_CTX_NO_UPDATE_CHECK='1',LEAN_CTX_TELEMETRY='off',DO_NOT_TRACK='1',LEAN_CTX_PROJECT_ROOT=str(ROOT),LEAN_CTX_CACHE_POLICY=policy,LEAN_CTX_WORKER_THREADS='2',LEANCTX_INDEX_THREADS='1')
        if scope: env['LEAN_CTX_SCOPE']=scope
        if claude: env['CLAUDECODE']='1'
        self.stderr=(OUT/(name+'-stderr.log')).open('wb')
        self.p=subprocess.Popen([str(BINARY),'mcp'],cwd=ROOT,env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=self.stderr)
        def reader():
            for line in self.p.stdout:
                try:self.q.put(json.loads(line))
                except Exception:self.q.put({'unparsed':line.decode(errors='replace')})
        threading.Thread(target=reader,daemon=True).start()
        self.started=time.monotonic()
        self.init=self.request('initialize',{'protocolVersion':'2025-03-26','capabilities':{},'clientInfo':{'name':'jackin-research','version':'0.1'}})
        self.notify('notifications/initialized',{})
    def notify(self,method,params):
        self.p.stdin.write((json.dumps({'jsonrpc':'2.0','method':method,'params':params})+'\n').encode());self.p.stdin.flush()
    def request(self,method,params,timeout=20):
        self.seq+=1; req={'jsonrpc':'2.0','id':self.seq,'method':method,'params':params};start=time.monotonic()
        self.p.stdin.write((json.dumps(req)+'\n').encode());self.p.stdin.flush()
        while True:
            left=timeout-(time.monotonic()-start)
            if left<=0: raise TimeoutError(method)
            obj=self.q.get(timeout=left)
            if obj.get('id')==self.seq and ('result' in obj or 'error' in obj):
                self.events.append({'request':req,'response':obj,'elapsed_ms':round((time.monotonic()-start)*1000,3)})
                return obj
    def read(self,mode='full',path='fixture.rs',**kwargs):
        return self.request('tools/call',{'name':'ctx_read','arguments':{'path':path,'mode':mode,**kwargs}})
    def close(self):
        self.p.stdin.close()
        try:self.p.wait(timeout=10)
        except subprocess.TimeoutExpired:self.p.terminate();self.p.wait(timeout=5)
        self.stderr.close()
        (OUT/(self.name+'-wire.json')).write_text(json.dumps(self.events,indent=2,ensure_ascii=False))

def texts(r):
    if 'error' in r:
        raise RuntimeError(json.dumps(r['error']))
    return '\n'.join(x.get('text','') for x in r.get('result',{}).get('content',[]) if x.get('type')=='text')

results={'binary':str(BINARY),'generated_at_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'cases':[]}
cold_only='--cold-only' in sys.argv
cases=[('unknown',{},'full'),('claude',{'claude':True},'full'),('scoped',{'scope':'agent-A'},'full'),('safe',{'scope':'agent-A','policy':'safe'},'full'),('map-scoped',{'scope':'agent-A'},'map'),('map-claude',{'claude':True},'map'),('auto-unknown',{},'auto'),('auto-claude',{'claude':True},'auto'),('auto-scoped',{'scope':'agent-A'},'auto')]
for name,params,mode in ([] if cold_only else cases):
    m=None
    try:
        m=MCP(name,**params)
        schemas=m.request('tools/list',{})
        first=m.read('full' if mode=='auto' else mode);second=m.read(mode);third=m.read(mode)
        (m.state/'last_compaction.json').write_text(json.dumps({'ts':int(time.time()*1000)}))
        compact=m.read(mode)
        exact=m.read(path='exact.txt',raw=True)
        subprocess.run(['git','checkout','-q','changed'],cwd=ROOT,check=True)
        changed=m.read(mode)
        subprocess.run(['git','checkout','-q','baseline'],cwd=ROOT,check=True)
        instructions=m.init.get('result',{}).get('instructions','')
        tools=schemas.get('result',{}).get('tools',[])
        record={'name':name,'mode':mode,'initialize_ms':m.events[0]['elapsed_ms'],'initialize_instruction_bytes':len(instructions.encode()),'tool_count':len(tools),'tool_schema_json_bytes':len(json.dumps(tools,separators=(',',':'),ensure_ascii=False).encode()),'tools':[x['name'] for x in tools],'first':texts(first),'second':texts(second),'third':texts(third),'after_compaction':texts(compact),'after_source_change':texts(changed),'exact_raw_matches':texts(exact).encode()==(ROOT/'exact.txt').read_bytes(),'exact_raw':texts(exact),'call_ms':[e['elapsed_ms'] for e in m.events if e['request']['method']=='tools/call']}
        if name=='auto-scoped':
            record['before_restart_seed']=texts(m.read('full'))
        results['cases'].append(record)
    except Exception as e:results['cases'].append({'name':name,'error':repr(e)})
    finally:
        if m:m.close()

# Resume and distinct consumer with shared data, each in a separate process.
for name,scope in ([] if cold_only else [('resume-A','agent-A'),('other-B','agent-B')]):
    m=None
    try:
        m=MCP(name,scope=scope,shared=OUT/'auto-scoped')
        r=m.read('auto')
        results['cases'].append({'name':name,'first_after_restart':texts(r)})
    except Exception as e:results['cases'].append({'name':name,'error':repr(e)})
    finally:
        if m:m.close()

# A second restart arm has never received a compaction marker or source change.
# Same explicit scope is deliberately reused; it does not itself prove that a
# resumed model still possesses the earlier observation.
for name,scope,seed in [('coldseed','agent-A',True),('cold-same-A','agent-A',False),('cold-other-B','agent-B',False)]:
    m=None
    try:
        m=MCP(name,scope=scope,shared=OUT/'coldseed')
        record={'name':name}
        if seed:record['before_restart_seed']=texts(m.read('full'))
        record['first_after_restart']=texts(m.read('auto'))
        results['cases'].append(record)
    except Exception as e:results['cases'].append({'name':name,'error':repr(e)})
    finally:
        if m:m.close()

(OUT/'results.json').write_text(json.dumps(results,indent=2,ensure_ascii=False))
for c in results['cases']:
    print(json.dumps({k:v for k,v in c.items() if k not in ['first','second','third','after_compaction','after_source_change','exact_raw','first_after_restart','before_restart_seed','tools']},ensure_ascii=False))
