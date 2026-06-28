#!/usr/bin/env python3
"""Decompose a Claude Code session transcript into token classes and dollars.

THE TRAP THIS AVOIDS: Claude Code writes one JSONL line per content block, and repeats the
*same* message.usage on every line of the same API response. Summing usage over raw lines
overcounts by ~3x (one response can be 6 lines). This script deduplicates by message.id first
— the single most important correctness step in any transcript analyzer.

  python3 session_cost.py [path-to-session.jsonl] [model]

Default path = the newest transcript for this project; default model = claude-opus-4-8.
Prices are list $/MTok (override in PRICES). Output: token volume by class, dollar split by class.
"""
import json, sys, glob, os

PRICES = {  # $/MTok: input, cache-read(0.1x), cache-write-5m(1.25x), cache-write-1h(2x), output
    "claude-opus-4-8":   dict(inp=5.0,  cr=0.5,  cw5=6.25, cw1=10.0, out=25.0),
    "claude-sonnet-4-6": dict(inp=3.0,  cr=0.3,  cw5=3.75, cw1=6.0,  out=15.0),
    "claude-haiku-4-5":  dict(inp=1.0,  cr=0.1,  cw5=1.25, cw1=2.0,  out=5.0),
    "claude-fable-5":    dict(inp=10.0, cr=1.0,  cw5=12.5, cw1=20.0, out=50.0),
}

def newest_transcript():
    base = os.path.expanduser("~/.claude/projects")
    files = glob.glob(f"{base}/*/*.jsonl")
    return max(files, key=os.path.getmtime) if files else None

def main():
    path = sys.argv[1] if len(sys.argv) > 1 else newest_transcript()
    model = sys.argv[2] if len(sys.argv) > 2 else "claude-opus-4-8"
    P = PRICES[model]
    seen = {}  # message.id -> usage (dedup; identical usage repeats across a response's lines)
    for line in open(path):
        line = line.strip()
        if not line:
            continue
        o = json.loads(line)
        if o.get("type") != "assistant":
            continue
        m = o.get("message") or {}
        mid, u = m.get("id"), m.get("usage")
        if mid and u:
            seen[mid] = u
    tot = dict(inp=0, cr=0, cw5=0, cw1=0, out=0)
    for u in seen.values():
        tot["inp"] += u.get("input_tokens", 0)
        tot["cr"]  += u.get("cache_read_input_tokens", 0)
        cc = u.get("cache_creation") or {}
        tot["cw5"] += cc.get("ephemeral_5m_input_tokens", 0)
        tot["cw1"] += cc.get("ephemeral_1h_input_tokens", 0)
        tot["out"] += u.get("output_tokens", 0)
    d = {k: tot[k] * P[k] / 1e6 for k in tot}
    D = sum(d.values()) or 1.0
    inp_side = tot["inp"] + tot["cr"] + tot["cw5"] + tot["cw1"]
    print(f"transcript: {path}")
    print(f"model: {model}   unique API responses: {len(seen)}")
    print(f"\nTOKEN VOLUME")
    print(f"  uncached input : {tot['inp']:>12,}")
    print(f"  cache read     : {tot['cr']:>12,}  ({100*tot['cr']/max(1,inp_side):.1f}% of input-side volume)")
    print(f"  cache write 5m : {tot['cw5']:>12,}")
    print(f"  cache write 1h : {tot['cw1']:>12,}")
    print(f"  output         : {tot['out']:>12,}")
    print(f"\nDOLLAR SPLIT  (total ${D:.4f})")
    print(f"  output      : {100*d['out']/D:5.1f}%   (${d['out']:.4f})")
    print(f"  cache write : {100*(d['cw5']+d['cw1'])/D:5.1f}%   (${d['cw5']+d['cw1']:.4f})")
    print(f"  cache read  : {100*d['cr']/D:5.1f}%   (${d['cr']:.4f})")
    print(f"  uncached in : {100*d['inp']/D:5.1f}%   (${d['inp']:.4f})")
    print("\nNote: output_tokens includes thinking (billed, but redacted from the transcript text).")
    print("To split thinking vs visible, count_tokens the text+tool_use blocks and subtract from output.")

if __name__ == "__main__":
    main()
