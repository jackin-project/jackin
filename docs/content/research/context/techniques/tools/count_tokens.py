# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

"""Prepare Anthropic token-count requests, offline unless --execute is specified.

python3 count_tokens.py --model EXACT_MODEL file sample.txt
python3 count_tokens.py --model EXACT_MODEL text 'public fixture'

--execute uses ANTHROPIC_API_KEY or the explicit --api-key-env variable.
No agent OAuth store is opened. Counts include framing and are estimates.
"""
import argparse
import json
import os
from pathlib import Path
import urllib.request


def body(model, content):
    return {"model": model, "messages": [{"role": "user", "content": content}]}


def count(request_body, key_env):
    key = os.environ.get(key_env)
    if not key:
        raise ValueError(f"set the explicitly selected API-key variable {key_env}")
    request = urllib.request.Request(
        "https://api.anthropic.com/v1/messages/count_tokens",
        data=json.dumps(request_body).encode("utf-8"),
        headers={"x-api-key": key, "anthropic-version": "2023-06-01",
                 "content-type": "application/json"}, method="POST")
    with urllib.request.urlopen(request, timeout=30) as response:
        return json.load(response)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True)
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--api-key-env", default="ANTHROPIC_API_KEY")
    parser.add_argument("mode", choices=["file", "text", "samples"])
    parser.add_argument("value")
    args = parser.parse_args()
    if args.mode == "samples":
        samples = json.loads(Path(args.value).read_text(encoding="utf-8"))
    else:
        text = (Path(args.value).read_bytes().decode("utf-8")
                if args.mode == "file" else args.value)
        samples = [{"label": args.value if args.mode == "file" else "text", "text": text}]
    output = []
    for sample in samples:
        request_body = body(args.model, sample["text"])
        row = {"label": sample["label"], "model": args.model,
               "characters": len(sample["text"]), "bytes": len(sample["text"].encode("utf-8"))}
        row["response" if args.execute else "request"] = (
            count(request_body, args.api_key_env) if args.execute else request_body)
        output.append(row)
    print(json.dumps(output, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
