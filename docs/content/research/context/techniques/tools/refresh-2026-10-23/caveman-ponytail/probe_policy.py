import json, os, pathlib, subprocess, tempfile, time

ROOT = pathlib.Path(__file__).resolve().parent
node = subprocess.check_output(['sh', '-c', 'command -v node'], text=True).strip()
results = {}
with tempfile.TemporaryDirectory(prefix='jackin-policy-') as td:
    temp = pathlib.Path(td)
    env = {'PATH': os.environ['PATH'], 'LANG': 'C.UTF-8',
           'CLAUDE_CONFIG_DIR': str(temp/'claude'), 'XDG_CONFIG_HOME': str(temp/'config'),
           'PLUGIN_DATA': str(temp/'data'), 'PONYTAIL_MAP': '0'}
    script = ROOT/'ponytail/hooks/ponytail-runtime.js'
    def state(project, session, value=None, codex=False):
        e = dict(env, CLAUDE_PROJECT_DIR=project, SESSION_ID=session)
        if not codex: e.pop('PLUGIN_DATA')
        js = 'const r=require(process.argv[1]);'
        js += 'r.setMode(process.argv[2]);' if value else ''
        js += 'process.stdout.write(r.readMode()||"off");'
        return subprocess.check_output([node, '-e', js, str(script), value or ''],env=e,text=True)
    a = str(temp/'repo-a'); b = str(temp/'repo-b')
    assert state(a, 's1', 'full') == 'full'
    assert state(b, 's2', 'lite') == 'lite'
    results['claude_distinct_projects_keep_modes'] = state(a, 's1') == 'full'
    assert state(a, 's2', 'ultra') == 'ultra'
    results['claude_same_project_other_session_reads_ultra'] = state(a, 's1') == 'ultra'
    assert state('', 's1', 'full', True) == 'full'
    assert state('', 's2', 'lite', True) == 'lite'
    results['codex_shared_plugin_data_other_session_reads_lite'] = state('', 's1', codex=True) == 'lite'
    durations = []
    for i in range(11):
        start=time.perf_counter()
        p=subprocess.run([node, str(ROOT/'ponytail/hooks/ponytail-activate.js')],input='',capture_output=True,text=True,env=dict(env,PONYTAIL_DEFAULT_MODE='full'),cwd=temp,check=True)
        durations.append((time.perf_counter()-start)*1000)
    output=json.loads(p.stdout)['hookSpecificOutput']['additionalContext']
    (ROOT/'ponytail-injected-full.txt').write_text(output)
    results['ponytail_map_disabled_activation_ms']={'first':durations[0],'subsequent':durations[1:]}
    results['ponytail_injection_bytes']=len(output.encode())
    cave_env=dict(env,CAVEMAN_DEFAULT_MODE='caveman')
    cave_env.pop('PLUGIN_DATA')
    p=subprocess.run([node,str(ROOT/'caveman/src/hooks/caveman-activate.js')],input=json.dumps({'session_id':'isolated','source':'startup','cwd':str(temp)}),capture_output=True,text=True,env=cave_env,cwd=temp,check=True)
    (ROOT/'caveman-injected.txt').write_text(p.stdout)
    results['caveman_injection_bytes']=len(p.stdout.encode())
(ROOT/'policy-probe-results.json').write_text(json.dumps(results,indent=2))
print(json.dumps(results,indent=2))
