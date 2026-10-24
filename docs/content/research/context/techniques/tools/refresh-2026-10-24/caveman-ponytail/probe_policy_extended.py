"""Offline emitted-policy, continuation-state and map probes; no host installation."""
import hashlib, json, os, pathlib, subprocess, tempfile, time
ROOT=pathlib.Path(__file__).resolve().parent
node=subprocess.check_output(['sh','-c','command -v node'],text=True).strip()
result={'executed_at_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'node':subprocess.check_output([node,'--version'],text=True).strip()}
with tempfile.TemporaryDirectory(prefix='jackin-policy-state-') as td:
 tmp=pathlib.Path(td)
 env={'PATH':os.environ['PATH'],'LANG':'C.UTF-8','CLAUDE_CONFIG_DIR':str(tmp/'claude'),'XDG_CONFIG_HOME':str(tmp/'config'),'PONYTAIL_MAP':'0','PONYTAIL_DEFAULT_MODE':'full','CAVEMAN_DEFAULT_MODE':'caveman'}
 def hook(product,name,payload,e=None):
  base=ROOT/product/('hooks' if product=='ponytail' else 'src/hooks')
  p=subprocess.run([node,str(base/name)],input=json.dumps(payload),capture_output=True,text=True,check=True,cwd=tmp,env=e or env)
  return p.stdout
 penv=dict(env,CLAUDE_PROJECT_DIR=str(tmp/'project'))
 for source in ['compact','resume','clear']:
  hook('ponytail','ponytail-activate.js',{'source':'startup'},penv)
  hook('ponytail','ponytail-mode-tracker.js',{'prompt':'stop ponytail'},penv)
  output=hook('ponytail','ponytail-activate.js',{'source':source},penv)
  result['ponytail_'+source+'_after_off']={'injects_active_policy':'PONYTAIL MODE ACTIVE' in output,'bytes':len(output.encode())}
 for source in ['compact','resume','fork','clear']:
  base={'session_id':'session-a','cwd':str(tmp)}
  hook('caveman','caveman-activate.js',dict(base,source='startup'))
  hook('caveman','caveman-mode-tracker.js',dict(base,prompt='stop caveman'))
  output=hook('caveman','caveman-activate.js',dict(base,source=source))
  result['caveman_'+source+'_after_off']={'injects_active_policy':'Respond terse like smart caveman' in output,'bytes':len(output.encode())}
 fixture=tmp/'map-fixture';(fixture/'src').mkdir(parents=True);(fixture/'tests').mkdir()
 for rel,text in {
   'src/plain.ts':'export function visibleSymbol() { return 1; }\n',
   'src/naïve.ts':'export function unicodePathSymbol() { return 2; }\n',
   'src/has space.ts':'export function spacedPathSymbol() { return 3; }\n',
   'tests/guard.ts':'export function decisiveTestSymbol() { return 4; }\n',
   'src/long.ts':'// filler\n'*8000+'export function lateSymbol() { return 5; }\n',
 }.items():(fixture/rel).write_text(text)
 output=subprocess.check_output([node,str(ROOT/'ponytail/hooks/ponytail-map.js'),str(fixture)],env=env,text=True,cwd=tmp)
 result['map_fixture']={'bytes':len(output.encode()),'output':output,'visible_found':'visibleSymbol' in output,'unicode_path_found':'unicodePathSymbol' in output,'spaced_path_found':'spacedPathSymbol' in output,'test_found':'decisiveTestSymbol' in output,'late_symbol_found':'lateSymbol' in output}
for product in ['caveman','caveman-runtime','ponytail']:
 result[product+'_commit']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT/product,text=True).strip()
(ROOT/'policy-extended-results.json').write_text(json.dumps(result,indent=2))
print(json.dumps(result,indent=2))
