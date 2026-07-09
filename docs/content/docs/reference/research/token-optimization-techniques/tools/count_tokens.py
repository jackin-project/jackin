#!/usr/bin/env python3
"""Real Anthropic tokenizer via /v1/messages/count_tokens, authed with the local
Claude Code OAuth session credential (read-only; the token is never printed).

Usage:
  count_tokens.py samples <file.json>   # file = [{"label","text"}, ...] -> TSV table
  count_tokens.py file <label> <path>   # count tokens of a file's contents
  count_tokens.py text  <label> <text>  # count tokens of a literal string
Token count returned is for the content wrapped in a single user message; a fixed
per-message overhead (~7 tokens, calibrated by the 'floor' label) applies to all rows.
"""
import json, sys, urllib.request, urllib.error, time
MODEL = "claude-opus-4-8"
_cred = json.load(open('/home/agent/.claude/.credentials.json'))
_tok = (_cred.get('claudeAiOauth') or {}).get('accessToken')
H = {"Authorization": f"Bearer {_tok}", "anthropic-version": "2023-06-01",
     "anthropic-beta": "oauth-2025-04-20", "content-type": "application/json"}
def count(text, model=MODEL):
    body = json.dumps({"model": model, "messages": [{"role": "user", "content": text}]}).encode()
    req = urllib.request.Request("https://api.anthropic.com/v1/messages/count_tokens",
                                 data=body, headers=H, method="POST")
    for attempt in range(4):
        try:
            r = urllib.request.urlopen(req, timeout=30)
            return json.load(r)["input_tokens"]
        except urllib.error.HTTPError as e:
            if e.code in (429, 529) and attempt < 3:
                time.sleep(2 * (attempt + 1)); continue
            raise
    raise RuntimeError("unreachable")
def main():
    mode = sys.argv[1]
    if mode == "samples":
        rows = json.load(open(sys.argv[2]))
        print("label\ttokens\tchars\tbytes\ttok/100char")
        for r in rows:
            t = r["text"]; n = count(t); c = len(t); b = len(t.encode())
            print(f"{r['label']}\t{n}\t{c}\t{b}\t{100*n/max(c,1):.1f}")
            time.sleep(0.1)
    elif mode == "file":
        t = open(sys.argv[3]).read(); n = count(t)
        print(f"{sys.argv[2]}\t{n}\t{len(t)}\t{len(t.encode())}")
    elif mode == "text":
        t = sys.argv[3]; print(f"{sys.argv[2]}\t{count(t)}\t{len(t)}")
if __name__ == "__main__":
    main()
