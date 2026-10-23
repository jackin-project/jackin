# SPDX-FileCopyrightText: 2026 Alexey Zhokhov
# SPDX-License-Identifier: Apache-2.0

"""Prepare synthetic PNG token-count probes; offline unless --execute is given.

python3 image_tokens.py --model EXACT_MODEL 280x280 1000x1000

No inherited pixel formula, cap or model list. Does not measure OCR fidelity.
"""
import argparse
import base64
import json
import struct
import zlib
from count_tokens import body, count


def png(width, height):
    if not 1 <= width <= 4096 or not 1 <= height <= 4096:
        raise ValueError("fixture dimensions must be 1..4096 pixels")
    raw = b"".join(b"\x00" + bytes((row * 7 + x) % 256 for x in range(width * 3))
                   for row in range(height))

    def chunk(kind, data):
        payload = kind + data
        return struct.pack(">I", len(data)) + payload + struct.pack(">I", zlib.crc32(payload) & 0xffffffff)

    return (b"\x89PNG\r\n\x1a\n" +
            chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)) +
            chunk(b"IDAT", zlib.compress(raw, 1)) + chunk(b"IEND", b""))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", required=True)
    parser.add_argument("--execute", action="store_true")
    parser.add_argument("--api-key-env", default="ANTHROPIC_API_KEY")
    parser.add_argument("dimensions", nargs="+")
    args = parser.parse_args()
    output = []
    for dimension in args.dimensions:
        width, height = map(int, dimension.lower().split("x"))
        data = png(width, height)
        content = [{"type": "image", "source": {"type": "base64", "media_type": "image/png",
                    "data": base64.b64encode(data).decode("ascii")}}]
        request_body = body(args.model, content)
        row = {"dimensions": dimension, "model": args.model, "png_bytes": len(data)}
        row["response" if args.execute else "request"] = (
            count(request_body, args.api_key_env) if args.execute else request_body)
        output.append(row)
    print(json.dumps(output, indent=2))


if __name__ == "__main__":
    main()
