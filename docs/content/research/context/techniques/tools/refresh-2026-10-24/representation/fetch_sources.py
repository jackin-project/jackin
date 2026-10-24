"""Public source retrieval only. Copy to scratch before use; mutable pages can change."""
import concurrent.futures,gzip,hashlib,html.parser,json,pathlib,time,urllib.request,sys
ROOT=pathlib.Path(__file__).resolve().parent
(ROOT/"sources").mkdir(exist_ok=True)
URLS={'cavewoman-paper': 'https://arxiv.org/html/2606.24083v1', 'cavewoman-expanded-readme': 'https://raw.githubusercontent.com/danielle34/cavewoman/28875b39a6c957a0ae96950538cdacfcd0eff0b3/README.md', 'notation-paper': 'https://arxiv.org/html/2605.29676v2', 'notation-repo-readme': 'https://raw.githubusercontent.com/lkutschka/notation-matters/b31385bb66ffdfd0b9da7eeaef03870a74aeafb1/README.md', 'chain-of-draft': 'https://arxiv.org/html/2502.18600v2', 'concise-cot': 'https://arxiv.org/abs/2401.05618', 'budget-tale': 'https://arxiv.org/html/2412.18547v5', 'sketch-thought': 'https://arxiv.org/abs/2503.05179', 'metaglyph': 'https://arxiv.org/html/2601.07354', 'neo-bench': 'https://aclanthology.org/2024.acl-long.749/', 'mtob': 'https://arxiv.org/abs/2309.16575', 'distillation-s2-s1': 'https://arxiv.org/html/2407.06023v2', 'brevity-complexity': 'https://arxiv.org/html/2503.01141v1', 'compression-tradeoff': 'https://arxiv.org/html/2505.00019v1', 'compression-attack': 'https://arxiv.org/html/2510.22963v2', 'onto-format': 'https://arxiv.org/html/2604.17512v1', 'language-tokenization': 'https://arxiv.org/abs/2305.15425', 'claude-vision-coordinates': 'https://platform.claude.com/docs/en/build-with-claude/vision-coordinates.md', 'gemini-media-resolution': 'https://ai.google.dev/gemini-api/docs/media-resolution', 'claude-prompting-best-practices': 'https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices.md', 'claude-vision': 'https://platform.claude.com/docs/en/build-with-claude/vision.md', 'claude-pdf': 'https://platform.claude.com/docs/en/build-with-claude/pdf-support.md', 'openai-vision': 'https://developers.openai.com/api/docs/guides/images-vision.md', 'gemini-vision': 'https://ai.google.dev/gemini-api/docs/image-understanding', 'claude-output-styles': 'https://code.claude.com/docs/en/output-styles.md', 'claude-stop': 'https://platform.claude.com/docs/en/api/handling-stop-reasons.md', 'openai-structured': 'https://developers.openai.com/api/docs/guides/structured-outputs.md', 'claude-code-headless': 'https://code.claude.com/docs/en/headless.md', 'aider-diffs': 'https://aider.chat/docs/unified-diffs.html', 'claude-counting': 'https://platform.claude.com/docs/en/build-with-claude/token-counting.md', 'claude-structured': 'https://platform.claude.com/docs/en/build-with-claude/structured-outputs.md', 'claude-effort': 'https://platform.claude.com/docs/en/build-with-claude/effort.md', 'claude-code-cache': 'https://code.claude.com/docs/en/prompt-caching.md', 'claude-pricing': 'https://platform.claude.com/docs/en/about-claude/pricing.md', 'openai-pricing': 'https://developers.openai.com/api/docs/pricing.md', 'gemini-pricing': 'https://ai.google.dev/gemini-api/docs/pricing', 'claude-task-budgets': 'https://platform.claude.com/docs/en/build-with-claude/task-budgets.md', 'claude-preserved-thinking': 'https://platform.claude.com/docs/en/build-with-claude/preserved-thinking.md', 'tiktoken-package': 'https://pypi.org/pypi/tiktoken/0.12.0/json', 'tiktoken-readme': 'https://raw.githubusercontent.com/openai/tiktoken/0.12.0/README.md'}
class Text(html.parser.HTMLParser):
 def __init__(self):super().__init__();self.parts=[];self.skip=0
 def handle_starttag(self,tag,attrs):
  if tag in ['script','style','svg','nav']:self.skip+=1
  if tag in ['p','h1','h2','h3','h4','li','tr','pre','br']:self.parts.append('\n')
 def handle_endtag(self,tag):
  if tag in ['script','style','svg','nav'] and self.skip:self.skip-=1
 def handle_data(self,d):
  if not self.skip:self.parts.append(d)
def f(kv):
 k,u=kv;r={'url':u,'accessed_at_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime())}
 try:
  with urllib.request.urlopen(urllib.request.Request(u,headers={'User-Agent':'Jackin-context-research','Accept':'text/markdown,text/html;q=0.9'}),timeout=40) as q:
   raw=q.read();r.update(status=q.status,final_url=q.url,content_type=q.headers.get('Content-Type'),last_modified=q.headers.get('Last-Modified'),bytes=len(raw),sha256=hashlib.sha256(raw).hexdigest())
  (ROOT/'sources'/f'{k}.raw').write_bytes(raw)
  decoded=gzip.decompress(raw) if raw[:2]==b"\x1f\x8b" else raw
  s=decoded.decode();
  if '<html' in s[:2000].lower():p=Text();p.feed(s);s='\n'.join(x.strip() for x in ''.join(p.parts).splitlines() if x.strip())
  (ROOT/'sources'/f'{k}.txt').write_text(s)
 except Exception as e:r['error']=str(e)
 return k,r
selected=URLS if len(sys.argv)==1 else {k:v for k,v in URLS.items() if k in sys.argv[1:]}
meta=ROOT/'refetched-source-register.json';out=json.loads(meta.read_text()) if meta.exists() else {}
for k,r in concurrent.futures.ThreadPoolExecutor(max_workers=4).map(f,selected.items()):out[k]=r;print(k,r.get('status',r.get('error')),r.get('bytes'))
meta.write_text(json.dumps(out,indent=2))
