from pathlib import Path
import hashlib,json,os,subprocess,time
root=Path('/workspace/scratch/1335a158b2a9')
out=root/'research/representation/independent-review';out.mkdir(exist_ok=True)
binary=root/'research/rtk-leanctx/artifacts/rtk-oss/rtk'
env=os.environ.copy()
env.update(XDG_CONFIG_HOME=str(out/'config'),XDG_DATA_HOME=str(out/'data'),
 RTK_DB_PATH=str(out/'tracking.db'),RTK_RECALL_DB=str(out/'recall.db'),
 RTK_TELEMETRY_DISABLED='1',NO_COLOR='1')
cases=[
 ('non_diff_structural',['diff','-'],b'operation failed: no permission\r\nlast'),
 ('non_diff_git_filter',['pipe','--filter','git-diff'],b'operation failed: no permission\r\nlast'),
 ('arbitrary_passthrough',['pipe','--passthrough'],b'valid\r\n\x00\xfflast'),
 ('grep_mixed_diagnostic',['pipe','--filter','grep'],(''.join(f'very/long/repository/path/src/example.rs:{i}:needle value {i}\n' for i in range(1,61))+'SCAN INCOMPLETE: protected directory unreadable\n').encode()),
]
rows=[]
for name,args,payload in cases:
 start=time.monotonic()
 r=subprocess.run([str(binary),*args],cwd=out,env=env,input=payload,capture_output=True,timeout=10)
 (out/(name+'.input')).write_bytes(payload)
 (out/(name+'.stdout')).write_bytes(r.stdout)
 (out/(name+'.stderr')).write_bytes(r.stderr)
 rows.append({'name':name,'argv':[str(binary),*args],'exit':r.returncode,
  'input_bytes':len(payload),'output_bytes':len(r.stdout),'byte_identical':r.stdout==payload,
  'input_sha256':hashlib.sha256(payload).hexdigest(),'stdout_sha256':hashlib.sha256(r.stdout).hexdigest(),
  'stdout_excerpt':r.stdout[:250].decode(errors='backslashreplace'),
  'retains_scan_incomplete':'SCAN INCOMPLETE' in r.stdout.decode(errors='replace'),
  'elapsed_seconds':round(time.monotonic()-start,6)})
x={'at_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),'reviewer':'/root/caveman_ponytail',
 'reviewed_authors':['/root/rtk_leanctx'],'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),
 'cwd':str(out),'env_overrides':{k:env[k] for k in ['XDG_CONFIG_HOME','XDG_DATA_HOME','RTK_DB_PATH','RTK_RECALL_DB','RTK_TELEMETRY_DISABLED','NO_COLOR']},
 'scope':'Independent released-OSS-stdin boundary probes; no host task or model. Non-diff and passthrough corroborate authored dossier; mixed grep diagnostic tests a separate unstructured failure line.',
 'records':rows}
(out/'rtk-independent-results.json').write_text(json.dumps(x,indent=2)+'\n')
print(json.dumps(x,indent=2))
