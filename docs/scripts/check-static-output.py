import html
from html.parser import HTMLParser
import json
import pathlib
import re
import socket
import shutil
import subprocess
import sys
import time
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET

root = pathlib.Path(__file__).resolve().parents[1]
content = root / 'content'
output = root / '.output/public'
origin = 'https://jackin.tailrocks.com'
bun = shutil.which('bun')
if bun is None:
    raise RuntimeError('bun is required')


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def source_pages():
    pages = {}
    for file in sorted(content.rglob('*.mdx')):
        parts = [p for p in file.relative_to(content).parts if not (p.startswith('(') and p.endswith(')'))]
        parts[-1] = pathlib.Path(parts[-1]).stem
        if parts[-1] == 'index':
            parts.pop()
        url = '/' + '/'.join(parts)
        require(url not in pages, f'duplicate source URL {url}')
        text = file.read_text()
        match = re.search(r'^title:\s*(.+)$', text, re.MULTILINE)
        require(match is not None, f'missing title: {file}')
        pages[url] = match.group(1).strip().strip('"\'')
    for file in content.rglob('meta.json'):
        metadata = json.loads(file.read_text())
        require(isinstance(metadata.get('pages', []), list), f'invalid navigation: {file}')
    return pages


def request(base, path):
    status, headers, body, _ = request_trace(base, path)
    return status, headers, body


def request_trace(base, path):
    try:
        with urllib.request.urlopen(base + path, timeout=10) as response:
            return response.status, response.headers, response.read(), response.geturl()
    except urllib.error.HTTPError as error:
        return error.code, error.headers, error.read(), error.geturl()


class Document(HTMLParser):
    def __init__(self):
        super().__init__()
        self.canonical = []
        self.assets = set()
        self.titles = []
        self.headings = []
        self.capture = None
        self.parts = []
        self.hidden = 0

    def handle_starttag(self, tag, attrs):
        values = dict(attrs)
        if tag in ('script', 'style'):
            self.hidden += 1
        if tag in ('title', 'h1', 'h2') and not self.hidden:
            self.capture = tag
            self.parts = []
        if tag == 'link' and values.get('rel') == 'canonical':
            self.canonical.append(values.get('href'))
        if tag == 'script' and values.get('src', '').startswith('/'):
            self.assets.add(values['src'])
        if tag == 'link' and values.get('rel') == 'stylesheet' and values.get('href', '').startswith('/'):
            self.assets.add(values['href'])

    def handle_data(self, data):
        if self.capture and not self.hidden:
            self.parts.append(data)

    def handle_endtag(self, tag):
        if tag in ('script', 'style'):
            self.hidden = max(0, self.hidden - 1)
        if tag == self.capture:
            text = ' '.join(''.join(self.parts).split())
            if tag == 'title':
                self.titles.append(text)
            else:
                self.headings.append((tag, text))
            self.capture = None
            self.parts = []


def require_not_found(body, label):
    document = Document()
    document.feed(body.decode())
    require(document.titles == ['Not found - jackin❯'], f'{label}: wrong not-found title')
    require(('h1', '404') in document.headings and ('h2', 'Page Not Found') in document.headings,
            f'{label}: missing not-found headings')


def main():
    pages = source_pages()
    require('/' not in pages, 'root source page conflicts with landing route')
    expected_html = {url.strip('/') + '/index.html' for url in pages} | {'index.html', '404.html'}
    actual_html = {str(path.relative_to(output)) for path in output.rglob('*.html')}
    require(actual_html == expected_html, f'HTML mismatch missing={expected_html-actual_html} extra={actual_html-expected_html}')
    expected_md = {url.strip('/') + '.md' for url in pages}
    actual_md = {str(path.relative_to(output)) for path in output.rglob('*.md')}
    require(actual_md == expected_md, f'Markdown mismatch missing={expected_md-actual_md} extra={actual_md-expected_md}')
    expected_og = {url.strip('/') + '.webp' for url in pages}
    actual_og = {str(path.relative_to(output / 'og')) for path in (output / 'og').rglob('*.webp')}
    require(actual_og == expected_og, f'OG mismatch missing={expected_og-actual_og} extra={actual_og-expected_og}')
    for url in ('/developing/construct-image', '/guides/security-model', '/commands', '/reference/brand-identity'):
        require(url in pages, f'anchor absent: {url}')
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    process = subprocess.Popen([bun, 'run', 'start', '--listen', f'tcp://127.0.0.1:{port}'], cwd=root, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    base = f'http://127.0.0.1:{port}'
    try:
        for _ in range(100):
            if process.poll() is not None:
                raise RuntimeError(f'static server exited: {process.stderr.read().decode()}')
            try:
                request(base, '/')
                break
            except OSError:
                time.sleep(0.05)
        else:
            raise RuntimeError('static server did not start')
        assets = set()
        for url, title in pages.items():
            status, headers, body = request(base, url + '/')
            text = body.decode()
            require(status == 200 and 'text/html' in headers.get('Content-Type', ''), f'bad HTML response {url}: {status}')
            require(f'<title>{html.escape(title)} - jackin❯</title>' in text, f'wrong title {url}')
            document = Document()
            document.feed(text)
            require(document.canonical == [origin + url + '/'], f'wrong canonical {url}')
            assets.update(document.assets)
            status, headers, body = request(base, url + '.md')
            require(status == 200 and 'text/markdown' in headers.get('Content-Type', '') and body.startswith(f'# {title} ('.encode()), f'wrong Markdown {url}')
            status, headers, body = request(base, '/og' + url + '.webp')
            require(status == 200 and 'image/webp' in headers.get('Content-Type', '') and body[:4] == b'RIFF' and body[8:12] == b'WEBP', f'wrong OG {url}')
        require(assets, 'no client assets in HTML')
        for asset in assets:
            status, _, body = request(base, asset)
            require(status == 200 and body, f'missing client asset {asset}')
        for url, phrase in {'/developing/construct-image': 'The construct image is the base', '/guides/security-model': 'Security model', '/commands': 'Exit codes', '/reference/brand-identity': 'Brand Identity'}.items():
            _, _, body = request(base, url + '/')
            require(phrase.encode() in body, f'anchor content missing {url}')
        _, _, homepage = request(base, '/')
        require(b'<html' in homepage, 'home missing HTML')
        _, _, sitemap_body = request(base, '/sitemap.xml')
        locations = {node.text for node in ET.fromstring(sitemap_body).iter('{http://www.sitemaps.org/schemas/sitemap/0.9}loc')}
        require(locations == {origin + '/'} | {origin + url + '/' for url in pages}, 'sitemap membership mismatch')
        _, _, llms_index = request(base, '/llms.txt')
        _, _, llms_full = request(base, '/llms-full.txt')
        search_status, search_headers, search, search_url = request_trace(base, '/api/search.json')
        require(search_status == 200, f'search response status was {search_status} at {search_url}')
        require('application/json' in search_headers.get('Content-Type', ''), 'search response is not JSON')
        search_index = json.loads(search)
        ids = set(search_index['internalDocumentIDStore']['internalIdToId'])
        obsolete_status, _, _, obsolete_url = request_trace(base, '/api/search')
        require(obsolete_status == 404, f'obsolete search URL resolved with status {obsolete_status} at {obsolete_url}')
        nearby_status, _, _, nearby_url = request_trace(base, '/api/search-missing')
        require(nearby_status == 404, f'nearby search URL resolved with status {nearby_status} at {nearby_url}')
        for url, title in pages.items():
            require(f']({url})'.encode() in llms_index, f'LLM index missing {url}')
            require(f'# {title} ({origin}{url}/)'.encode() in llms_full, f'LLM full missing {url}')
            require(url in ids, f'search index missing {url}')
        for missing_path in ('/this-path-must-not-exist.md', '/og/this-path-must-not-exist.webp'):
            missing_status, _, _ = request(base, missing_path)
            require(missing_status == 404, f'missing artifact did not return 404: {missing_path}')
        expected_status, expected_headers, expected_not_found = request(base, '/404')
        require(expected_status == 200 and 'text/html' in expected_headers.get('Content-Type', ''), 'not-found route is not an HTML document')
        require_not_found(expected_not_found, '/404')
        status, headers, missing = request(base, '/this-path-must-not-exist/')
        require(status == 404, f'unknown HTML status was {status}, expected 404')
        require('text/html' in headers.get('Content-Type', ''), 'unknown HTML response has the wrong content type')
        require_not_found(missing, 'unknown HTML')
        require(missing == expected_not_found, 'unknown route did not serve the not-found document')
        print(json.dumps({'pages': len(pages), 'html': len(actual_html), 'markdown': len(actual_md), 'og': len(actual_og), 'search_url': search_url, 'search_status': search_status, 'obsolete_search_url': obsolete_url, 'obsolete_search_status': obsolete_status, 'nearby_search_url': nearby_url, 'nearby_search_status': nearby_status, 'unknown_status': status, 'unknown_content_type': headers.get('Content-Type')}))
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


if __name__ == '__main__':
    main()
