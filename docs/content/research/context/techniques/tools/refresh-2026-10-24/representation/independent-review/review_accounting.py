from pathlib import Path
from decimal import Decimal
import hashlib,importlib.util,json,sys,time
root=Path('/workspace/scratch/1335a158b2a9')
tools=root/'jackin/docs/content/research/context/techniques/tools'
sys.path.insert(0,str(tools))
import provider_usage
out=root/'research/representation/independent-review'
x=json.loads((tools/'refresh-2026-10-24/evaluation/accounting-fixtures.json').read_text())
rows=[]
for c in x['cases']:
 result=provider_usage.normalize(c['contract'],c['usage'],c.get('cache_writes'))
 actual=provider_usage.price(result,c['rates'])
 # Independently dot the reviewed disjoint expected partitions, using Decimal.
 if c['expected_usd'] is None:
  expected=None
 else:
  expected=sum(Decimal(n)*Decimal(str(c['rates'][k]))/Decimal(1_000_000) for k,n in c['expected_tokens'].items() if n)
 equal=result['tokens']==c['expected_tokens']
 costmatch=actual['usd'] is None if expected is None else abs(Decimal(str(actual['usd']))-expected)<Decimal('0.000000000001')
 rows.append({'id':c['id'],'partition_matches':equal,'expected_decimal_usd':str(expected) if expected is not None else None,
  'observed_usd':actual['usd'],'price_matches':costmatch,
  'reasoning_output_tokens':result['reasoning_output_tokens'],
  'scope':'Re-executed reviewed explicit fixture only; no model/API call'})
 assert equal and costmatch,c['id']
res={'reviewer':'/root/caveman_ponytail','at_utc':time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),
 'reviewed_files':{str(p.relative_to(root/'jackin')):hashlib.sha256(p.read_bytes()).hexdigest() for p in [tools/'provider_usage.py',tools/'session_cost.py',tools/'refresh-2026-10-24/evaluation/accounting-fixtures.json']},
 'source_contracts':[
 'OpenAI prompt-caching#monitor-cache-performance: cached/write disjoint, lifetime parts sum write, ordinary=total-read-write',
 'Claude compaction-threshold and compaction-on-demand: use final iterations once; top-level excludes compaction',
 'Claude adaptive-thinking: thinking_tokens subset of output_tokens, final message_delta',
 'Gemini thinking: output and thought disjoint; 62+171+297=530 documented example',
 'Gemini caching: total_cached_tokens subset of input'],
 'cases':rows,'result':'No accounting discrepancy found in these four scoped fixtures or reviewed normalization paths; incomplete/ambiguous receipts fail or remain unknown.',
 'limits':['No broad upstream suite rerun','No actual provider invoice/receipt fetched','Exact rate selection, tiers and non-token fees remain explicit caller responsibility']}
(out/'accounting-review-results.json').write_text(json.dumps(res,indent=2)+'\n')
print(json.dumps(res,indent=2))
