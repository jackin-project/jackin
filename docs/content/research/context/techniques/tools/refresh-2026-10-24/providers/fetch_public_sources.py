import concurrent.futures,gzip,hashlib,html.parser,json,pathlib,time,urllib.request,sys
ROOT=pathlib.Path(__file__).resolve().parent
(ROOT/'sources').mkdir(exist_ok=True)
URLS={
'sglang-hicache-design':'https://docs.sglang.io/docs/advanced_features/hicache_design.md',
'sglang-hicache-practices':'https://docs.sglang.io/docs/advanced_features/hicache_best_practices.md',
'opencode-tool-core':'https://raw.githubusercontent.com/anomalyco/opencode/c272a63cb4dd86a3e8771cf9a03be051fae28e95/packages/core/src/tool.ts',
'lmcache-multiprocess':'https://docs.lmcache.ai/mp/index.html',
'sglang-gateway':'https://docs.sglang.io/advanced_features/sgl_model_gateway.html',
'route-llm':'https://arxiv.org/abs/2406.18665',
'llm-router-bench':'https://aclanthology.org/2026.findings-acl.1881/',
'compactionbench':'https://www.microsoft.com/en-us/research/publication/benchmarking-long-horizon-agent-context-compaction-with-compactionbench/',
'openai-cache':'https://developers.openai.com/api/docs/guides/prompt-caching.md',
'openai-pricing':'https://developers.openai.com/api/docs/pricing.md',
'openai-reasoning':'https://developers.openai.com/api/docs/guides/reasoning.md',
'openai-compaction':'https://developers.openai.com/api/docs/guides/compaction.md',
'openai-codex-config':'https://developers.openai.com/codex/config-reference.md',
'openai-codex-pricing':'https://developers.openai.com/codex/pricing.md',
'openai-plugin-hooks':'https://developers.openai.com/plugins/build/plugins.md',
'claude-cache':'https://platform.claude.com/docs/en/build-with-claude/prompt-caching.md',
'claude-pricing':'https://platform.claude.com/docs/en/about-claude/pricing.md',
'claude-compaction':'https://platform.claude.com/docs/en/build-with-claude/compaction.md',
'claude-context-editing':'https://platform.claude.com/docs/en/build-with-claude/context-editing.md',
'claude-effort':'https://platform.claude.com/docs/en/build-with-claude/effort.md',
'claude-thinking':'https://platform.claude.com/docs/en/build-with-claude/adaptive-thinking.md',
'claude-batch':'https://platform.claude.com/docs/en/build-with-claude/batch-processing.md',
'claude-rate-limits':'https://platform.claude.com/docs/en/api/rate-limits.md',
'claude-counting':'https://platform.claude.com/docs/en/build-with-claude/token-counting.md',
'claude-ptc':'https://platform.claude.com/docs/en/agents-and-tools/tool-use/programmatic-tool-calling.md',
'claude-tool-search':'https://platform.claude.com/docs/en/agents-and-tools/tool-use/tool-search-tool.md',
'claude-mid-system':'https://platform.claude.com/docs/en/build-with-claude/mid-conversation-system-messages.md',
'claude-code-cache':'https://code.claude.com/docs/en/prompt-caching.md',
'claude-code-costs':'https://code.claude.com/docs/en/costs.md',
'claude-code-hooks':'https://code.claude.com/docs/en/hooks.md',
'claude-code-subagents':'https://code.claude.com/docs/en/sub-agents.md',
'claude-code-memory':'https://code.claude.com/docs/en/memory.md',
'claude-code-skills':'https://code.claude.com/docs/en/skills.md',
'claude-code-mcp':'https://code.claude.com/docs/en/mcp.md',
'claude-code-context':'https://code.claude.com/docs/en/context-window.md',
'claude-sdk-system':'https://code.claude.com/docs/en/agent-sdk/modifying-system-prompts.md',
'gemini-cache':'https://ai.google.dev/gemini-api/docs/caching',
'gemini-pricing':'https://ai.google.dev/gemini-api/docs/pricing',
'gemini-thinking':'https://ai.google.dev/gemini-api/docs/thinking',
'opencode-tool-promise':'https://raw.githubusercontent.com/anomalyco/opencode/c272a63cb4dd86a3e8771cf9a03be051fae28e95/packages/plugin/src/promise/tool.ts',
'opencode-tool-effect':'https://raw.githubusercontent.com/anomalyco/opencode/c272a63cb4dd86a3e8771cf9a03be051fae28e95/packages/plugin/src/effect/tool.ts',
'litellm-router-current':'https://docs.litellm.ai/docs/auto_router/',
'litellm-semantic-current':'https://docs.litellm.ai/docs/proxy/caching_semantic',
'opencode-plugin-promise':'https://raw.githubusercontent.com/anomalyco/opencode/c272a63cb4dd86a3e8771cf9a03be051fae28e95/packages/plugin/src/promise/plugin.ts',
'opencode-hook-core':'https://raw.githubusercontent.com/anomalyco/opencode/c272a63cb4dd86a3e8771cf9a03be051fae28e95/packages/core/src/plugin/hooks.ts',
'claude-task-budgets':'https://platform.claude.com/docs/en/build-with-claude/task-budgets.md',
'claude-thinking-workflows':'https://platform.claude.com/docs/en/build-with-claude/thinking-tool-and-multi-turn-workflows.md',
'vllm-apc':'https://docs.vllm.ai/en/stable/features/automatic_prefix_caching/',
'vllm-cache-design':'https://docs.vllm.ai/en/stable/design/prefix_caching/',
'sglang-hicache':'https://docs.sglang.io/docs/advanced_features/hicache',
'lmcache-overview':'https://docs.lmcache.ai/getting_started/overview.html',
'litellm-cache':'https://docs.litellm.ai/docs/proxy/caching',
'litellm-semantic-cache':'https://docs.litellm.ai/docs/proxy/semantic_caching',
'litellm-router':'https://docs.litellm.ai/docs/proxy/auto-routing',
'claude-pro-max':'https://support.claude.com/en/articles/11145838-use-claude-code-with-your-pro-or-max-plan',
'omp-extensions':'https://raw.githubusercontent.com/can1357/oh-my-pi/1c0303b1f2ec515cbf4b44a9a49d68a029531aac/docs/extensions.md',
'grok-hooks':'https://raw.githubusercontent.com/xai-org/grok-build/2bdd1d6a6369de0e8c68132ea4539e9abd9e14a8/crates/codegen/xai-grok-pager/docs/user-guide/10-hooks.md',
'opencode-plugin-types':'https://raw.githubusercontent.com/anomalyco/opencode/c272a63cb4dd86a3e8771cf9a03be051fae28e95/packages/plugin/src/promise/index.ts',
'hermes-pinned-hooks':'https://raw.githubusercontent.com/NousResearch/hermes-agent/5894e750c3dba980d43b25aa29e3958149ba2c61/website/docs/user-guide/features/hooks.md',
'muse-hook-reference':'https://meta-models.github.io/muse-code-sdk/next/guides/plugins/reference/hook-events/',
'claude-monthly-api-credits':'https://support.claude.com/en/articles/17154008-monthly-api-credits-for-max-and-team-plans',
'claude-preserved-thinking':'https://platform.claude.com/docs/en/build-with-claude/preserved-thinking.md',
'claude-compaction-on-demand':'https://platform.claude.com/docs/en/build-with-claude/compaction-on-demand.md',
'claude-compaction-threshold':'https://platform.claude.com/docs/en/build-with-claude/compaction-threshold.md',
'claude-compaction-keep-recent-turns':'https://platform.claude.com/docs/en/build-with-claude/compaction-keep-recent-turns.md',
'claude-compaction-background':'https://platform.claude.com/docs/en/build-with-claude/compaction-background.md',
'claude-compaction-thinking-blocks':'https://platform.claude.com/docs/en/build-with-claude/compaction-thinking-blocks.md',
'gemini-interactions':'https://ai.google.dev/gemini-api/docs/interactions-overview',
'amp-plugins':'https://ampcode.com/docs/customize/plugins',
'amp-tools':'https://ampcode.com/docs/tools',
'amp-rules':'https://ampcode.com/docs/customize/agents-md',
'kimi-hooks':'https://www.kimi.com/code/docs/en/kimi-code-cli/customization/hooks.html',
'opencode-plugins':'https://opencode.ai/docs/plugins/',
'antigravity-hooks':'https://www.antigravity.google/docs/hooks',
'gemini-cli-hooks':'https://geminicli.com/docs/hooks/reference/',
'cursor-hooks':'https://cursor.com/docs/hooks',
'muse-extending':'https://dev.meta.ai/docs/muse-code/extending',
'muse-configuration':'https://dev.meta.ai/docs/muse-code/configuration',
'hermes-hooks':'https://hermes-agent.nousresearch.com/docs/user-guide/features/hooks',
'hermes-middleware':'https://hermes-agent.nousresearch.com/docs/developer-guide/middleware',
'claude-subscription-sdk':'https://support.claude.com/en/articles/15036540-use-the-claude-agent-sdk-with-your-claude-plan',
'claude-usage-credits':'https://support.claude.com/en/articles/12429409-manage-usage-credits-for-paid-claude-plans',
'claude-gateway':'https://code.claude.com/docs/en/llm-gateway.md',
'claude-sdk-subagents':'https://code.claude.com/docs/en/agent-sdk/subagents.md',
'claude-code-model':'https://code.claude.com/docs/en/model-config.md',
'claude-structured':'https://platform.claude.com/docs/en/build-with-claude/structured-outputs.md',
'claude-context-windows':'https://platform.claude.com/docs/en/build-with-claude/context-windows.md',
'claude-models':'https://platform.claude.com/docs/en/about-claude/models/overview.md',
}
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
meta=ROOT/'source-register.json';out=json.loads(meta.read_text()) if meta.exists() else {}
for k,r in concurrent.futures.ThreadPoolExecutor(max_workers=4).map(f,selected.items()):out[k]=r;print(k,r.get('status',r.get('error')),r.get('bytes'))
meta.write_text(json.dumps(out,indent=2))
