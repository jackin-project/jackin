#!/usr/bin/env python3
"""Measure image (visual) token cost on the real Anthropic tokenizer, across models.

No PIL/ImageMagick needed: builds valid PNGs of arbitrary WxH with the stdlib (struct+zlib),
base64-encodes them into an image content block, and reads input_tokens from count_tokens.

Verifies the documented model: visual_tokens = ceil(w/28) * ceil(h/28), then capped per model
family. Run with no args for the default sweep, or pass `WxH WxH ...`.

  python3 image_tokens.py 280x280 1000x1000 2000x2000

Auth: the local Claude Code OAuth credential (read-only; token never printed). count_tokens is free.
"""
import json, struct, zlib, base64, urllib.request, urllib.error, math, sys
_cred = json.load(open('/home/agent/.claude/.credentials.json'))
_tok = (_cred.get('claudeAiOauth') or {}).get('accessToken')
H = {"Authorization": f"Bearer {_tok}", "anthropic-version": "2023-06-01",
     "anthropic-beta": "oauth-2025-04-20", "content-type": "application/json"}
MODELS = ["claude-opus-4-8", "claude-sonnet-4-6", "claude-haiku-4-5"]

def png(w, h):
    """Minimal valid 8-bit RGB PNG, mildly noisy rows (realistic, not a degenerate solid)."""
    raw = b''.join(b'\x00' + bytes(((i * 7 + j) % 256 for j in range(w * 3))) for i in range(h))
    def chunk(typ, data):
        c = typ + data
        return struct.pack('>I', len(data)) + c + struct.pack('>I', zlib.crc32(c) & 0xffffffff)
    return (b'\x89PNG\r\n\x1a\n'
            + chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 2, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(raw, 1))
            + chunk(b'IEND', b''))

def count_img(b64, model):
    body = json.dumps({"model": model, "messages": [{"role": "user", "content":
            [{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64}}]}]}).encode()
    req = urllib.request.Request("https://api.anthropic.com/v1/messages/count_tokens",
                                 data=body, headers=H, method="POST")
    try:
        with urllib.request.urlopen(req, timeout=40) as r:
            return json.load(r)["input_tokens"]
    except urllib.error.HTTPError as e:
        return f"HTTP{e.code}"

def main():
    dims = sys.argv[1:] or ["28x28", "280x280", "1000x1000", "2000x2000"]
    print("dims\tpredict\t" + "\t".join(m.split('-')[1] for m in MODELS))
    for d in dims:
        w, h = (int(x) for x in d.lower().split('x'))
        pred = math.ceil(w / 28) * math.ceil(h / 28)
        b64 = base64.b64encode(png(w, h)).decode()
        cells = "\t".join(str(count_img(b64, m)) for m in MODELS)
        print(f"{d}\t{pred}\t{cells}")

if __name__ == "__main__":
    main()
