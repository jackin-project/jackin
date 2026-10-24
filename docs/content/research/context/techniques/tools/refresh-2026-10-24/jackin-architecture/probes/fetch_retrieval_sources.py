#!/usr/bin/env python3
"""Fetch public primary retrieval/memory sources for the October 24 audit."""
import concurrent.futures
import hashlib
import json
import urllib.request
from datetime import datetime, timezone
from html.parser import HTMLParser
from pathlib import Path

DEST = Path(__file__).resolve().parent / "retrieval-sources"
DEST.mkdir(exist_ok=True)
URLS = {
    "claude-memory-tool": "https://platform.claude.com/docs/en/agents-and-tools/tool-use/memory-tool.md",
    "claude-context-announcement": "https://claude.com/blog/context-management",
    "anthropic-long-harness": "https://www.anthropic.com/research/effective-harnesses-for-long-running-agents",
    "anthropic-harness-2026": "https://www.anthropic.com/research/harness-design-long-running-apps",
    "manus-context": "https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus",
    "cursor-semantic": "https://cursor.com/blog/semsearch",
    "cline-indexing": "https://cline.bot/blog/why-cline-doesnt-index-your-codebase-and-why-thats-a-good-thing",
    "aider-repomap": "https://aider.chat/docs/repomap.html",
    "mem0-v1": "https://arxiv.org/abs/2504.19413v1",
    "convomem-v1": "https://arxiv.org/abs/2511.10523v1",
    "memgpt-v2": "https://arxiv.org/abs/2310.08560v2",
    "letta-filesystem": "https://www.letta.com/blog/benchmarking-ai-agent-memory/",
    "zep-critique": "https://www.getzep.com/blog/lies-damn-lies-statistics-is-mem0-really-sota-in-agent-memory/",
    "gptcache": "https://aclanthology.org/2023.nlposs-1.24/",
    "gpt-semantic-cache-v3": "https://arxiv.org/abs/2411.05276v3",
}


class Plain(HTMLParser):
    def __init__(self):
        super().__init__()
        self.parts = []
        self.skip = 0

    def handle_starttag(self, tag, attrs):
        if tag in ("script", "style"):
            self.skip += 1
        if tag in ("p", "div", "h1", "h2", "h3", "li", "tr", "br"):
            self.parts.append("\n")

    def handle_endtag(self, tag):
        if tag in ("script", "style"):
            self.skip = max(0, self.skip - 1)
        if tag in ("p", "div", "li", "tr"):
            self.parts.append("\n")

    def handle_data(self, data):
        if not self.skip:
            self.parts.append(data)


def fetch(item):
    name, url = item
    row = {"name": name, "url": url, "accessed_utc": datetime.now(timezone.utc).isoformat()}
    try:
        request = urllib.request.Request(url, headers={"User-Agent": "Jackin-public-research/1.0"})
        with urllib.request.urlopen(request, timeout=25) as response:
            data = response.read(8 * 1024 * 1024 + 1)
            assert len(data) <= 8 * 1024 * 1024
            row.update(status=response.status, final_url=response.url,
                       content_type=response.headers.get("Content-Type", ""))
        (DEST / (name + ".raw")).write_bytes(data)
        text = data.decode("utf-8", errors="replace")
        if "html" in row["content_type"]:
            plain = Plain()
            plain.feed(text)
            text = "\n".join(line.strip() for line in "".join(plain.parts).splitlines() if line.strip())
        (DEST / (name + ".txt")).write_text(text)
        row.update(bytes=len(data), sha256=hashlib.sha256(data).hexdigest(),
                   text_sha256=hashlib.sha256(text.encode()).hexdigest(),
                   local_raw=str(DEST / (name + ".raw")), local_text=str(DEST / (name + ".txt")))
    except Exception as exc:
        row["error"] = f"{type(exc).__name__}: {exc}"
    return row


with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:
    results = list(executor.map(fetch, URLS.items()))
(DEST.parent / "retrieval-source-register.json").write_text(json.dumps(results, indent=2) + "\n")
for result in results:
    print(json.dumps({key: result[key] for key in ("name", "status", "bytes", "error") if key in result}))
