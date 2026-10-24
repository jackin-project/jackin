import pathlib,urllib.request,json,hashlib,tarfile,io,datetime,platform,sys
base=pathlib.Path(__file__).parent
records=[]
for name,version in [('fff-search','0.11.0'),('sem-core','0.28.1'),('mq-markdown','0.9.3'),('mq-lang','0.9.3')]:
 meta=json.loads((base/'sources'/(f'{name}-{version.replace(".","_")}-registry.txt')).read_text())['version']
 url=f'https://static.crates.io/crates/{name}/{name}-{version}.crate'
 start=datetime.datetime.now(datetime.timezone.utc).isoformat()
 data=urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'Jackin-evidence-audit/2026-10-24'}),timeout=45).read()
 sha=hashlib.sha256(data).hexdigest()
 if sha!=meta['checksum']:raise ValueError(f'checksum mismatch {name}')
 rec={'crate':name,'version':version,'artifact_url':url,'accessed':start,'bytes':len(data),'sha256':sha,'registry_checksum_match':True,'executed':False,'scope':'downloaded published archive; selected manifests and API-source members inspected without building'}
 dst=base/'published-crates'/name;dst.mkdir(parents=True,exist_ok=True)
 with tarfile.open(fileobj=io.BytesIO(data),mode='r:gz') as tf:
  prefix=f'{name}-{version}/'
  wanted=['Cargo.toml','Cargo.toml.orig','.cargo_vcs_info.json','src/lib.rs']
  rec['members']=[]
  for part in wanted:
   try:member=tf.getmember(prefix+part)
   except KeyError:continue
   body=tf.extractfile(member).read();target=dst/part;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(body)
   rec['members'].append({'path':part,'bytes':len(body),'sha256':hashlib.sha256(body).hexdigest()})
   if part=='.cargo_vcs_info.json':rec['vcs']=json.loads(body)
 records.append(rec)
 print(json.dumps({k:rec[k] for k in ['crate','version','bytes','sha256','vcs']}))
out={'cwd':str(base.resolve().parent.parent),'command':'python research/market-literature/inspect_published_crates.py','interpreter':sys.version,'platform':platform.platform(),'exit_code':0,'cutoff':'2026-10-24T01:15:00Z','records':records}
(base/'published-crate-inspection.json').write_text(json.dumps(out,indent=2)+'\n')
