#!/usr/bin/env python3
"""Free arithmetic probes of documented cache contracts; never sends a request."""
from decimal import Decimal as D
import json

def break_even(write, read):
    w, r = D(write), D(read)
    reads = 0
    while w + reads*r >= 1 + reads:
        reads += 1
    return {"write_multiple": str(w), "read_multiple": str(r),
            "additional_reads_for_strict_saving": reads,
            "assumptions": "One full write, identical eligible prefix, no expiry or other costs."}

def gemini_delta(delta):
    old = 10000
    reprocessed = 2050 if delta < 2050 else 0
    cached, uncached = old-reprocessed, delta+reprocessed
    return {"new_tokens": delta, "cached_tokens": cached, "uncached_tokens": uncached,
            "input_usd": str((D(cached)*D('.10') + D(uncached)*D('2'))/D(1000000))}

def common_prefix(a,b):
    for i,(x,y) in enumerate(zip(a,b)):
        if x!=y:return i
    return min(len(a),len(b))

baseline, compressed = gemini_delta(2100),gemini_delta(1000)
stable = ("unchanging authorized project instructions\n"*100).encode()
dynamic_a,dynamic_b=b"workspace=/work/job-a\n",b"workspace=/work/job-b\n"
out={
 "evidence_class":"Executed synthetic arithmetic and exact-byte fixtures; no provider inference, tokenizer, quota, latency, or task-quality measurement.",
 "sources":{
   "openai":"https://developers.openai.com/api/docs/guides/prompt-caching",
   "claude":"https://platform.claude.com/docs/en/build-with-claude/prompt-caching",
   "gemini":"https://ai.google.dev/gemini-api/docs/caching",
   "gemini_prices":"https://ai.google.dev/gemini-api/docs/pricing"},
 "break_even":[break_even('1.25','.025'),break_even('2','.025'),
               break_even('1.25','.05'),break_even('2.5','.05'),
               break_even('1.25','.1'),break_even('2.5','.1')],
 "gemini_argon_small_delta":{
   "assumptions":"10,000-token prior request eligible; new aggregate includes output/thoughts/tool results; only input charges shown, October introductory USD rates; documented 2,050-token reprocessing rule.",
   "baseline":baseline,"shorter_delta":compressed,
   "input_cost_change_percent":str((D(compressed['input_usd'])/D(baseline['input_usd'])-1)*100)},
 "fleet":{
   "assumptions":"Ten requests, one exact eligible prefix, one authorized cache scope, no TTL expiry; normalized prefix units. Concurrent cold writes are a comparison scenario, not guaranteed observed behavior.",
   "ten_independent_writes":str(D(10)*D('1.25')),
   "one_pioneer_nine_reads_at_005":str(D('1.25')+D(9)*D('.05')),
   "extra_prewarm_then_ten_reads_at_005":str(D('1.25')+D(10)*D('.05')),
   "ten_isolated_workspaces_minimum_writes":10},
 "byte_prefix_fixture":{
   "stable_bytes":len(stable),
   "dynamic_first_shared_bytes":common_prefix(dynamic_a+stable,dynamic_b+stable),
   "stable_first_shared_bytes":common_prefix(stable+dynamic_a,stable+dynamic_b),
   "limitation":"Bytes are not provider tokens; changing instruction authority/order can change semantics. This is not a recommended production rewrite."}
}
print(json.dumps(out,indent=2))
