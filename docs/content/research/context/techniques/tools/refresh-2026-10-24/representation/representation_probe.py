#!/usr/bin/env python3
"""Offline fixture counts and deterministic round trips; no model or API evaluation."""
import argparse, base64, csv, hashlib, io, json, math, os, platform, socket, sys
from pathlib import Path

# Cached public encodings only. Any accidental socket connection aborts the probe.
def no_network(*args, **kwargs):
    raise RuntimeError("network disabled by representation probe")
socket.socket.connect = no_network
socket.create_connection = no_network
import tiktoken

P = argparse.ArgumentParser()
P.add_argument("--out", required=True)
a = P.parse_args()
out = Path(a.out)
out.mkdir(parents=True, exist_ok=True)
fixtures = out / "fixtures"
fixtures.mkdir(exist_ok=True)
enc = {n:tiktoken.get_encoding(n) for n in ("cl100k_base", "o200k_base")}
def canonical(x):
    return json.dumps(x, ensure_ascii=False, sort_keys=True, separators=(",",":"), allow_nan=False)
def measure(label, s, suffix="txt"):
    b = s.encode("utf-8")
    name = label+"."+suffix
    (fixtures/name).write_bytes(b)
    return {"fixture":name,"bytes":len(b),"characters":len(s),"sha256":hashlib.sha256(b).hexdigest(),
            "tokens":{n:len(e.encode(s,disallowed_special=())) for n,e in enc.items()}}
def table_encode(rows):
    keys = list(rows[0])
    if len(keys)!=len(set(keys)) or any(set(r)!=set(keys) for r in rows):
        raise ValueError("uniform unique fields required; missing differs from null")
    return {"fields":keys,"rows":[[r[k] for k in keys] for r in rows]}
def table_decode(x):
    keys=x["fields"]
    if len(keys)!=len(set(keys)) or any(len(r)!=len(keys) for r in x["rows"]):
        raise ValueError("invalid table width or duplicate fields")
    return [dict(zip(keys,row)) for row in x["rows"]]
def separated(rows, delimiter, typed):
    f=io.StringIO(newline="")
    w=csv.writer(f,delimiter=delimiter,lineterminator="\n")
    fields=list(rows[0]);w.writerow(fields)
    for r in rows:
        w.writerow([canonical(r[k]) if typed else r[k] for k in fields])
    return f.getvalue()
def separated_decode(s, delimiter, typed):
    rr=list(csv.reader(io.StringIO(s,newline=""),delimiter=delimiter))
    if len(rr[0])!=len(set(rr[0])): raise ValueError("duplicate header")
    if any(len(r)!=len(rr[0]) for r in rr[1:]): raise ValueError("width mismatch")
    return [dict(zip(rr[0],[json.loads(v) if typed else v for v in r])) for r in rr[1:]]
def strict_json(s):
    def unique(pairs):
        result={}
        for k,v in pairs:
            if k in result: raise ValueError("duplicate JSON key")
            result[k]=v
        return result
    return json.loads(s,object_pairs_hook=unique)
def rejected(fn):
    try: fn()
    except ValueError: return True
    return False

rows=[{"record_identifier":f"acct-{i:05d}",
       "display_name":["Ana","李娜","Zoë","comma,quote\"","line\nbreak","pipe|tab\t"][i%6],
       "is_active":i%2==0,
       "balance":round(i*12.75,2),
       "nullable_note":None if i%3==0 else ("" if i%3==1 else "keep NOT disabled"),
       "postal_code":f"{i:05d}",
       "timestamp_utc":"2026-10-24T00:30:00Z"} for i in range(18)]
formats={}
for name,kwargs in [("json_pretty2",{"indent":2}),("json_pretty4",{"indent":4}),("json_tabs",{"indent":"\t"}),
                    ("json_compact",{"separators":(",",":")}),("json_ascii_escaped",{"ensure_ascii":True,"separators":(",",":")})]:
    kw={"ensure_ascii":False,"allow_nan":False};kw.update(kwargs)
    s=json.dumps(rows,**kw);entry=measure(name,s,"json")
    entry["typed_roundtrip"]=canonical(strict_json(s))==canonical(rows);formats[name]=entry
s=canonical(table_encode(rows))
formats["json_fields_rows"]=measure("json_fields_rows",s,"json")
formats["json_fields_rows"]["typed_roundtrip"]=canonical(table_decode(strict_json(s)))==canonical(rows)
for delimiter,label in [(",","csv"),("\t","tsv"),("|","pipe")]:
    for typed in [False,True]:
        name=label+("_json_cells" if typed else "_plain")
        s=separated(rows,delimiter,typed);entry=measure(name,s)
        decoded=separated_decode(s,delimiter,typed)
        entry["typed_roundtrip"]=canonical(decoded)==canonical(rows)
        entry["null_empty_collapsed"]=not typed and decoded[0]["nullable_note"]==decoded[1]["nullable_note"]
        formats[name]=entry
adversarial={
 "ragged_rows":[{"id":"a","note":None},{"id":"b"}],
 "duplicate_key_json":'{"permission":"deny","permission":"allow"}',
 "nested_uniform":[{"id":"a","values":[1,None,{"x":"comma,quote\""}]},{"id":"b","values":[]}],
 "duplicate_header":{"fields":["id","id"],"rows":[["first","last"]]},
 "wrong_row_width":{"fields":["id","name"],"rows":[["a"]]}
}
measure("adversarial",json.dumps(adversarial,ensure_ascii=False,indent=2),"json")
checks={
 "json_all_typed_roundtrips":all(x["typed_roundtrip"] for k,x in formats.items() if k.startswith("json_")),
 "typed_separated_roundtrips":all(x["typed_roundtrip"] for k,x in formats.items() if k.endswith("_json_cells")),
 "plain_separated_not_typed":all(not x["typed_roundtrip"] for k,x in formats.items() if k.endswith("_plain")),
 "plain_csv_null_empty_collapses":formats["csv_plain"]["null_empty_collapsed"],
 "ragged_fields_rows_rejected":rejected(lambda:table_encode(adversarial["ragged_rows"])),
 "duplicate_json_key_rejected":rejected(lambda:strict_json(adversarial["duplicate_key_json"])),
 "duplicate_table_field_rejected":rejected(lambda:table_decode(adversarial["duplicate_header"])),
 "wrong_table_width_rejected":rejected(lambda:table_decode(adversarial["wrong_row_width"])),
 "nested_values_roundtrip":canonical(table_decode(table_encode(adversarial["nested_uniform"])))==canonical(adversarial["nested_uniform"])
}
styles={
 "verbose":"Please read logs.csv, select rows whose level is ERROR or more severe, and report the three most frequent causes. Do not drop negations, account identifiers, or permission failures. Include the exact line numbers that support each cause.",
 "plain":"Read logs.csv. Filter level >= ERROR. Report the top 3 causes with exact line numbers. Preserve negations, account identifiers, and permission failures.",
 "telegraphic":"logs.csv: level >= ERROR; top 3 causes + exact lines. Keep negations, account IDs, permission failures.",
 "symbols":"logs.csv → level ≥ ERROR → top3 causes ∧ exact lines. ¬drop negations, account IDs, permission failures.",
 "english_gate":"If all tests pass, request approval before committing.",
 "chinese_gate":"若所有測試通過，提交前先請求批准。"
}
style_counts={k:measure("style_"+k,v) for k,v in styles.items()}
surface_values={
 "id_hyphenated":"c9ed7d13-6bd3-4f1a-9158-c8bdeeb11373",
 "id_plain":"c9ed7d136bd34f1a9158c8bdeeb11373",
 "id_base64":base64.b64encode(bytes.fromhex("c9ed7d136bd34f1a9158c8bdeeb11373")).decode(),
 "camel":"accountPermissionFailureCount",
 "snake":"account_permission_failure_count",
 "timestamp_iso":"2026-10-24T00:30:00Z",
 "timestamp_epoch":"1792801800",
 "ascii_operators":"a >= 10 and b != 0 -> allow",
 "unicode_operators":"a ≥ 10 ∧ b ≠ 0 ⇒ allow",
 "path_full":"crates/apps/jackin-capsule/src/tui/render_conformance_fixtures.rs",
 "path_alias":"RCF",
 "path_legend":"RCF = crates/apps/jackin-capsule/src/tui/render_conformance_fixtures.rs"
}
surface_counts={k:measure("surface_"+k,v) for k,v in surface_values.items()}
alias={}
for n in enc:
    saving=surface_counts["path_full"]["tokens"][n]-surface_counts["path_alias"]["tokens"][n]
    legend=surface_counts["path_legend"]["tokens"][n]
    alias[n]={"saved_tokens_per_mention":saving,"legend_tokens":legend,
              "strict_token_break_even_mentions":legend//saving+1 if saving>0 else None}
before="".join(f"const parameter_{i:03d} = {i};\n" for i in range(200))
old="const parameter_100 = 100;\n"
new="const parameter_100 = 101;\n"
def apply_once(text, old, new):
    if not old or text.count(old)!=1: raise ValueError("anchor must occur exactly once")
    return text.replace(old,new,1)
after=apply_once(before,old,new)
edit={"file":"fixture.ts","old":old,"new":new}
whole={"file":"fixture.ts","content":after}
measure("edit_before",before,"ts");measure("edit_after",after,"ts")
edit_counts={"exact_edit":measure("edit_payload",canonical(edit),"json"),
             "whole_file":measure("whole_file_payload",canonical(whole),"json")}
checks["edit_result_exact"]=after=="".join(f"const parameter_{i:03d} = {101 if i==100 else i};\n" for i in range(200))
checks["duplicate_edit_anchor_rejected"]=rejected(lambda:apply_once(before+old,old,new))

# Formula calculations only; no images generated, resized, sent, or recognized.
def claude_size(w,h,edge,budget):
    # Enumerating integer long-edge candidates independently implements the documented constraints.
    landscape=w>=h;long=max(w,h);short=min(w,h)
    for newlong in range(min(long,edge),0,-1):
        newshort=max(1,round(newlong*short/long))
        if math.ceil(newlong/28)*28<=edge and math.ceil(newshort/28)*28<=edge and math.ceil(newlong/28)*math.ceil(newshort/28)<=budget:
            return (newlong,newshort) if landscape else (newshort,newlong)
    raise ValueError("no valid dimensions")
images=[]
for dims in [(1000,1000),(1920,1080),(3840,2160),(1075,1520)]:
    entry={"original":list(dims),"claude":{}}
    for tier,edge,budget in [("standard",1568,1568),("high",2576,4784)]:
        w,h=claude_size(*dims,edge,budget)
        entry["claude"][tier]={"resized":[w,h],"visual_tokens":math.ceil(w/28)*math.ceil(h/28)}
    images.append(entry)
checks["claude_documented_a4"]=images[3]["claude"]["standard"]["resized"]==[924,1307]
checks["claude_documented_screenshot"]=images[1]["claude"]["standard"]["resized"]==[1456,819]
checks["claude_documented_high4k"]=images[2]["claude"]["high"]["resized"]==[2576,1449]
openai=[
 {"model":"gpt-6-astra","input":[1024,1024],"detail":"high","processed":[1024,1024],"patches":1024,"billed_image_tokens":math.ceil(1024*1.2)},
 {"model":"gpt-6-astra","input":[2048,2048],"detail":"high","processed":[1600,1600],"patches":2500,"billed_image_tokens":math.ceil(2500*1.2)},
 {"model":"gpt-6-astra","input":[2048,2048],"detail":"original","processed":[2048,2048],"patches":4096,"billed_image_tokens":math.ceil(4096*1.2)}
]
# Argon4's input rate from the separately dated official provider register; arithmetic, not a bill.
gemini=[{"image_resolution":n,"approx_tokens":t,"input_usd_at_2_per_million":t*2/1_000_000} for n,t in [("low",280),("medium",560),("high",1120),("ultra_high",2240)]]
result={
 "scope":"Synthetic fixture bytes, public proxy tokenizer counts, local round trips, and documented formula arithmetic only. No API calls, model generations, OCR, latency, task acceptance, subscription quota, or production host execution.",
 "network":"socket connections disabled before tokenizer initialization; predownloaded tokenizer cache only",
 "python":platform.python_version(),"tiktoken":tiktoken.__version__,"encodings":list(enc),
 "formats":formats,"checks":checks,"style_counts":style_counts,
 "surface_counts":surface_counts,"alias_arithmetic":alias,"edit_counts":edit_counts,
 "image_formula_examples":{"claude":images,"openai":openai,"gemini":gemini},
 "format_semantics":"typed values, object keys/presence and array order for fixtures; not source-byte identity, arbitrary JSON numeric lexemes, or model comprehension",
 "style_semantics":"manually authored candidate phrasings; no validated equivalence or language translation evaluation"
}
(out/"results.json").write_text(json.dumps(result,ensure_ascii=False,indent=2)+"\n")
print(json.dumps({"checks_passed":sum(checks.values()),"checks_total":len(checks),"all_pass":all(checks.values()),"formats":{k:v["tokens"] for k,v in formats.items()}},indent=2))
if not all(checks.values()): sys.exit(1)
