import json, os, pathlib, subprocess, sys, time
ROOT = pathlib.Path(__file__).resolve().parent
mode = sys.argv[1]
node = subprocess.check_output(['sh', '-c', 'command -v node'], text=True).strip()
go = str(ROOT/'toolchains/go/bin/go')
env = {'PATH': str(ROOT/'toolchains/go/bin')+':'+os.environ['PATH'], 'LANG':'C.UTF-8',
       'GOTOOLCHAIN':'local', 'GOCACHE':str(ROOT/'go-cache'), 'GOMODCACHE':str(ROOT/'go-mod-cache'),
       'GOPROXY':'https://proxy.golang.org', 'GOSUMDB':'sum.golang.org',
       'DO_NOT_TRACK':'1', 'CAVEMAN_TELEMETRY':'0', 'CAVEMAN_CACHE_WARM':'off', 'CGO_ENABLED':'0', 'GOMAXPROCS':'2'}
proxy_keys = [key for key in ['HTTP_PROXY','HTTPS_PROXY','ALL_PROXY','NO_PROXY','http_proxy','https_proxy','all_proxy','no_proxy'] if key in os.environ]
for key in proxy_keys: env[key] = os.environ[key]
selections = {
 'ponytail-scorers': ('ponytail', [sys.executable,'benchmarks/agentic/run.py','--selftest']),
 'ponytail-completeness-gate': ('ponytail', [sys.executable,'benchmarks/agentic/complete.py','--selftest-offline']),
 'caveman-node': ('caveman', [node,'--test','--test-reporter=tap','tests/hooks/codex-sessionstart.test.mjs','tests/hooks/host-session-start.test.mjs','tests/hooks/subagent-start.test.mjs','tests/hooks/caveman-config.test.mjs','tests/hooks/headless-entrypoint.test.mjs']),
 'ponytail-node': ('ponytail', [node,'--test','--test-reporter=tap','tests/hooks.test.js','tests/behavior.test.js','tests/map.test.js','tests/uninstall.test.js','tests/activation-map.test.js','tests/commands.test.js','tests/band-import.test.js']),
 'caveman-go-nocgo': ('caveman-runtime', [go,'test','-p=1','-mod=readonly','-count=1','-short','./engine','./engine/ccr','./engine/tokens','./engine/pixel','./proxy/internal/config','./mcp']),
 'caveman-warm': ('caveman-runtime', [go,'test','-p=1','-mod=readonly','-count=1','-short','./proxy/cmd/caveman-proxy','-run','TestCacheWarm']),
 'caveman-gateway': ('caveman-runtime', [go,'test','-p=1','-mod=readonly','-count=1','-short','./proxy/internal/gateway','-run','TestCacheWarm|TestCachePrefix|TestRecoveryExempt']),
}
if mode == 'caveman-go-cgo':
 selections[mode] = selections['caveman-go-nocgo']
 env['CGO_ENABLED']='1'
checkout, argv = selections[mode]
started=time.time()
meta={'started_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime(started)), 'checkout':checkout,
      'commit':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT/checkout,text=True).strip(),
      'argv':argv,'env':{k:v for k,v in env.items() if k not in proxy_keys},'inherited_proxy_variable_names':proxy_keys,'node':subprocess.check_output([node,'--version'],text=True).strip()}
try:
    with (ROOT/'logs'/f'{mode}.log').open('w') as f:
        p=subprocess.run(argv,cwd=ROOT/checkout,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=900)
    meta['exit_code']=p.returncode
except subprocess.TimeoutExpired:
    meta['exit_code']='timeout'
meta['duration_seconds']=time.time()-started
(ROOT/'logs'/f'{mode}.json').write_text(json.dumps(meta,indent=2))
print(json.dumps(meta,indent=2))
print((ROOT/'logs'/f'{mode}.log').read_text()[-5500:])
