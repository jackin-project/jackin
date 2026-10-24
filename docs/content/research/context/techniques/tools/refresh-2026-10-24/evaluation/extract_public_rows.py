# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0
"""Offline re-aggregation of explicitly downloaded public pinned benchmark data.
No model calls, credentials, install commands or upstream executable imports.
"""
import argparse, ast, collections, csv, hashlib, json, pathlib, re, statistics
ROOT=pathlib.Path(__file__).parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--source-dir',type=pathlib.Path,default=ROOT/'sources')
parser.add_argument('--output-dir',type=pathlib.Path,default=ROOT)
args=parser.parse_args()
SRC=args.source_dir
ROOT=args.output_dir
ROOT.mkdir(parents=True,exist_ok=True)
def csvrows(path): return list(csv.DictReader(path.open(encoding='utf-8-sig')))
def grouped(rows,key):
 out=collections.defaultdict(list)
 for r in rows: out[r[key]].append(r)
 return out
def total(rows,key): return sum(float(r.get(key) or 0) for r in rows)
out={}; slim={}
rows=csvrows(SRC/'dasein/results/2026-07-04/paired.csv'); slim['dasein']=rows
out['dasein']={a:{'attempts':len(rr),'tasks':len({r['instance'] for r in rr}),
 'solved':total(rr,'success'),'reported_usd_sum':total(rr,'cost_usd'),
 'usd_per_solved':total(rr,'cost_usd')/total(rr,'success')} for a,rr in grouped(rows,'arm').items()}
out['lemoncrow']={}; slim['lemoncrow']={}
for d in ['swe50_2026_06_30','swe-lite_2026-07-16','swe-lite-release-validation_2026-09-21','exploration-release-validation_2026-09-25']:
 rows=csvrows(SRC/'lemoncrow/codebench/results'/d/'results.csv')
 keys=['task','arm','rep','ok','valid','correct','score','cost_usd','duration_ms','num_turns','input_tokens','cache_read_tokens','cache_creation_tokens','output_tokens','models','judge_model']
 slim['lemoncrow'][d]=[{k:r.get(k) for k in keys} for r in rows]
 out['lemoncrow'][d]={a:{'attempts':len(rr),'tasks':len({r['task'] for r in rr}),
   'correct':sum(r.get('correct')=='True' for r in rr),'ok':sum(r.get('ok')=='True' for r in rr),
   'reported_usd_sum':total(rr,'cost_usd'),'output_tokens':total(rr,'output_tokens'),
   'judges':sorted({r.get('judge_model','') for r in rr})} for a,rr in grouped(rows,'arm').items()}
rtk=json.loads((SRC/'rtk-current/results.json').read_text()); slim['rtk_payload']=rtk['commands']
out['rtk_payload']={m:{'raw':sum(c['tokens'][m]['raw'] for c in rtk['commands']),
 'rtk':sum(c['tokens'][m]['rtk'] for c in rtk['commands'])} for m in rtk['commands'][0]['tokens']}
out['rtk_agents']={}; slim['rtk_agents']={}
for label,rel in [('claude','agents/claude_results.json'),('droid','agents/droid_results.json'),('amp','amp/amp_results.json'),('amp_wsl','amp/amp_results_wsl.json')]:
 rows=json.loads((SRC/'rtk-current'/rel).read_text())['runs']
 keys=['side','idx','correct','of','context_tokens','tool_result_chars','cost_usd','credits','model','usage','wall_s','rc','error']
 slim['rtk_agents'][label]=[{k:r.get(k) for k in keys} for r in rows]
 out['rtk_agents'][label]={a:{'runs':len(rr),'correct':[r['correct'] for r in rr],
  'mean_context_tokens':statistics.mean(r['context_tokens'] for r in rr),
  'mean_tool_chars':statistics.mean(r['tool_result_chars'] for r in rr),
  'reported_usd_sum':sum(r['cost_usd'] for r in rr) if all(r.get('cost_usd') is not None for r in rr) else None,
  'credits_sum':sum(r['credits'] for r in rr) if all(r.get('credits') is not None for r in rr) else None} for a,rr in grouped(rows,'side').items()}
tok=json.loads((SRC/'tokbench/analysis/data.json').read_text()); slim['tokbench']=[]; mismatches=[]
for r in tok['runs']:
 p=SRC/'tokbench/results'/r['dir']/'run-summary.tsv'
 if not p.exists(): continue
 phases=list(csv.DictReader(p.open(),delimiter='\t'))
 sums={k:sum(int(x.get(k) or 0) for x in phases) for k in ['input','output','turns','seconds']}
 if sums['input']!=r['total_input']: mismatches.append(r['dir'])
 slim['tokbench'].append(dict(run=r['dir'],arm=r['arm'],classification=r['cls'],why=r.get('why'),**sums))
out['tokbench']={'raw_summary_mismatches':mismatches,'matrix':{},'excluded_void_input':{}}
for a,rr in grouped([r for r in slim['tokbench'] if r['classification']=='matrix'],'arm').items():
 out['tokbench']['matrix'][a]={'runs':len(rr),'distinct_tasks':1,'input_median':statistics.median(r['input'] for r in rr),'input_sum':sum(r['input'] for r in rr)}
for a,rr in grouped([r for r in slim['tokbench'] if r['classification']=='void'],'arm').items():
 out['tokbench']['excluded_void_input'][a]={'runs':len(rr),'input_sum':sum(r['input'] for r in rr)}
rows=csvrows(SRC/'context-guru/deploy/eval-containers/results/swe-bench-sweep-2026-07-13.csv'); slim['context_guru_sweep']=rows
out['context_guru_sweep']={a:{'rows':len(rr),'tasks':len({r['task'] for r in rr}),
 'passed':sum(r['passed']=='True' for r in rr),'up_failed':sum(r['note']=='up-failed' for r in rr),
 'gateway_before':total(rr,'gw_before'),'gateway_saved':total(rr,'gw_saved')} for a,rr in grouped(rows,'config').items()}
# Extract just the two pure grading functions, without importing the API runner.
path=SRC/'headroom-benchmarks/src/headroom_benchmarks/benchmarks/tier1/bfcl.py'
tree=ast.parse(path.read_text()); funcs=[n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name in ('ground_truth_param_values','check_response')]
namespace={'json':json}; exec(compile(ast.Module(body=funcs,type_ignores=[]),str(path),'exec'),namespace)
out['structured_headroom_grader_probe']={'wrong_function_accepted':namespace['check_response']('{"name":"delete_everything","arguments":{"unrelated":42}}','[{"add":{"x":[42],"y":[7]}}]'),
 'meaning':'The exact upstream BFCL checker accepts any one ground-truth argument substring, without requiring correct function or full arguments.'}
slim['structured_headroom_grader_probe']=out['structured_headroom_grader_probe']
# Historical Terminal-Bench raw rollups: keep absent costs visible and cross-check
# the table's cache label against separately published trial-level token classes.
# Despite its filename, turns.csv contains one aggregate row per trial, not receipts.
base=SRC/'lemoncrow/harbor/results/baseline'
trial=csvrows(base/'tbench_opus48_claudecode_2.1.205_tasks.csv')
turn=csvrows(base/'tbench_opus48_claudecode_2.1.205_turns.csv')
for label,rows in [('tasks',trial),('turns',turn)]:
 names=[r['trial_name'] for r in rows]
 if len(names)!=len(set(names)):
  raise ValueError(f'LemonCrow baseline {label}: duplicate trial_name')
if {r['trial_name'] for r in trial}!={r['trial_name'] for r in turn}:
 raise ValueError('LemonCrow baseline: tasks/turns trial_name sets differ')
byname={r['trial_name']:r for r in turn}
slim['lemoncrow_terminal_baseline']=[dict(task=r['task'],trial=r['trial_name'],
 result=r['result'],reported_cost_usd=float(r['cost_usd']) if r['cost_usd'] else None,
 table_input=int(r['input_tokens']),table_cache=int(r['cache_tokens']),
 prompt=int(byname[r['trial_name']]['prompt_tokens']),
 reads=int(byname[r['trial_name']]['cache_read_tokens']),
 writes=int(byname[r['trial_name']]['cache_creation_tokens']),
 output=int(byname[r['trial_name']]['completion_tokens'])) for r in trial]
rr=slim['lemoncrow_terminal_baseline']
out['lemoncrow_terminal_baseline']={'trials':len(rr),'tasks':len({r['task']for r in rr}),
 'pass':sum(r['result']=='pass'for r in rr),'fail':sum(r['result']=='fail'for r in rr),
 'other_reward':sum(r['result']not in ('pass','fail')for r in rr),
 'missing_reported_cost':sum(r['reported_cost_usd'] is None for r in rr),
 'observed_reported_cost_sum_excludes_unknown':sum(r['reported_cost_usd'] or 0 for r in rr),
 'prompt_tokens':sum(r['prompt']for r in rr),'cache_reads':sum(r['reads']for r in rr),
 'cache_writes':sum(r['writes']for r in rr),
 'ordinary_input':sum(r['prompt']-r['reads']-r['writes']for r in rr),
 'table_input_minus_table_cache':sum(r['table_input']-r['table_cache']for r in rr),
 'table_cache_equals_reads_rows':sum(r['table_cache']==r['reads']for r in rr),
 'table_cache_equals_reads_plus_writes_rows':sum(r['table_cache']==r['reads']+r['writes']for r in rr)}
slim['lemoncrow_terminal_runs']={}
for label in ['2026-07-28__19-19-14','2026-07-29__15-35-08']:
 r=json.loads((SRC/'lemoncrow/harbor/results'/label/'result.json').read_text())
 evalkey,ev=next(iter(r['stats']['evals'].items()))
 slim['lemoncrow_terminal_runs'][label]={'job_id':r['id'],'evaluation':evalkey,
  'total_trials':r['n_total_trials'],'completed':r['stats']['n_completed_trials'],
  'errors':r['stats']['n_errored_trials'],'evaluation_trials':ev['n_trials'],
  'reward_counts':{k:len(v)for k,v in ev['reward_stats']['reward'].items()},
  'reported_mean':ev['metrics'][0]['mean']}
out['lemoncrow_terminal_runs']=slim['lemoncrow_terminal_runs']
# Published first-party small tables: arithmetic only, not the absent raw trials.
wrap=(SRC/'caveman/docs/WRAP-BENCHMARK.md').read_text()
slim['caveman_wrap_report']=[dict(case=m[0],direct=int(m[1].replace(',','')),
 caveman=int(m[2].replace(',',''))) for m in re.findall(
 r'^\| `([^`]+)` \| [^|]+ \| ([\d,]+) \| ([\d,]+) \| [^|]+ \| 3/3 \|',wrap,re.M)]
rr=slim['caveman_wrap_report']; direct=sum(r['direct']for r in rr);candidate=sum(r['caveman']for r in rr)
out['caveman_wrap_report']={'cases':len(rr),'direct_input':direct,'caveman_input':candidate,
 'input_reduction':1-candidate/direct,'scope':'Published report table, summed unweighted input classes, not raw-run reproduction or a price estimate.'}
proof=(SRC/'headroom-first-party/benchmarks/results/index_proof_table.txt').read_text()
section=proof.split('=== default (protect_recent=4) ===')[1].split('=== full corpus')[0]
slim['headroom_synthetic_report']=[dict(scenario=m[0].strip(),before=int(m[1].replace(',','')),
 after=int(m[2].replace(',','')))for m in re.findall(r'^([A-Z][A-Za-z0-9 ()]+?)\s{2,}([\d,]+)\s+([\d,]+)\s+\d+%',section,re.M) if m[0].strip()!='TOTAL']
rr=slim['headroom_synthetic_report'];before=sum(r['before']for r in rr);after=sum(r['after']for r in rr)
out['headroom_synthetic_report']={'scenarios':len(rr),'before':before,'after':after,
 'reduction':1-after/before,'scope':'First-party static synthetic tokenizer proof table, not an agent task or provider receipt.'}
(ROOT/'benchmark-rows.json').write_text(json.dumps(slim,indent=2)+'\n')
(ROOT/'aggregation-results.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps(out,indent=2))
