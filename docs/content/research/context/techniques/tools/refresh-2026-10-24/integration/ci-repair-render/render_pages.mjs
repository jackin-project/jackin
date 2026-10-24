#!/usr/bin/env node
import { readFile, writeFile, readdir, mkdir, stat } from 'node:fs/promises';
import path from 'node:path';
import { pathToFileURL } from 'node:url';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';

const arg = (name, fallback) => { const i = process.argv.indexOf(name); return i >= 0 ? process.argv[i + 1] : fallback; };
if (process.argv.includes('--help')) {
  console.log('Usage: node render_pages.mjs --repo REPO --manifest BUILD_INPUT_HASHES_JSON --browser CHROMIUM_EXECUTABLE --playwright-module PLAYWRIGHT_INDEX_JS [--origin http://127.0.0.1:4188] [--out DIRECTORY] [--route-prefix /research/context/]\nRequired path options also accept QA_REPO, QA_MANIFEST, QA_BROWSER and QA_PLAYWRIGHT_MODULE environment variables. The script uses an existing local static build and browser; it never installs or downloads either.');
  process.exit(0);
}
const required = (name, env) => { const value = arg(name, process.env[env]); if (!value || value.startsWith('--')) throw new Error(`Provide ${name} or ${env}`); return path.resolve(value); };
const repo = required('--repo', 'QA_REPO');
const origin = arg('--origin', process.env.QA_ORIGIN ?? 'http://127.0.0.1:4188');
const originUrl = new URL(origin);
if (originUrl.protocol !== 'http:' || !['127.0.0.1', 'localhost', '[::1]'].includes(originUrl.hostname) || originUrl.username || originUrl.password || originUrl.pathname !== '/') throw new Error('Origin must be an HTTP loopback server without credentials or a path.');
const out = path.resolve(arg('--out', process.env.QA_OUT ?? 'rendered-qa-output'));
const manifestPath = required('--manifest', 'QA_MANIFEST');
const executable = required('--browser', 'QA_BROWSER');
const playwrightModule = required('--playwright-module', 'QA_PLAYWRIGHT_MODULE');
const { default: playwright } = await import(pathToFileURL(playwrightModule).href);
const playwrightVersion = JSON.parse(await readFile(path.join(path.dirname(playwrightModule), 'package.json'))).version;
const routePrefix = arg('--route-prefix', '/research/context/');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const normalize = s => s.replace(/\s+/g, ' ').trim();
await mkdir(path.join(out, 'screenshots'), { recursive: true });
const manifestBytes = await readFile(manifestPath);
const manifest = JSON.parse(manifestBytes);
const mismatches = [];
for (const [file, expected] of Object.entries(manifest)) {
  const current = sha(await readFile(path.join(repo, file)));
  if (current !== expected) mismatches.push({ file, expected, current });
}
if (mismatches.length) {
  await writeFile(path.join(out, 'stale-inputs.json'), JSON.stringify({ mismatches }, null, 2));
  throw new Error(`Build inputs changed: ${mismatches.map(x => x.file).join(', ')}`);
}
async function walk(dir) {
  const files = [];
  for (const item of await readdir(dir, { withFileTypes: true })) {
    const full = path.join(dir, item.name);
    if (item.isDirectory()) files.push(...await walk(full));
    else if (item.isFile() && item.name.endsWith('.mdx')) files.push(full);
  }
  return files;
}
const contentRoot = path.join(repo, 'docs/content');
const sources = [];
for (const file of (await walk(path.join(contentRoot, 'research/context'))).sort()) {
  const bytes = await readFile(file); const source = bytes.toString();
  const rawTitle = source.match(/^title:\s*(.+)$/m)?.[1]?.trim() ?? '';
  const title = rawTitle.startsWith('"') ? JSON.parse(rawTitle) : rawTitle.replace(/^'|'$/g, '');
  let route = '/' + path.relative(contentRoot, file).replaceAll(path.sep, '/').replace(/\.mdx$/, '');
  route = route.replace(/\/index$/, '') + '/';
  if (!route.startsWith(routePrefix)) continue;
  const htmlPath = path.join(repo, 'docs/.output/public', route, 'index.html');
  const html = await readFile(htmlPath);
  sources.push({ file: path.relative(repo, file), route, expectedTitle: title, sourceSha256: sha(bytes), htmlSha256: sha(html), htmlModified: (await stat(htmlPath)).mtime.toISOString(), expectsCurrent: source.includes('**Research state:** Current') });
}
const representatives = new Set(['/research/context/', '/research/context/tools/02-headroom-design/', '/research/context/engine/02-architecture/', '/research/context/techniques/17-adoption-roadmap/', '/research/context/techniques/20-multimodal-token-economics/', '/research/context/techniques/23-cross-agent-portability/']);
const browser = await playwright.chromium.launch({ executablePath: executable, headless: true, args: ['--no-sandbox', '--disable-dev-shm-usage'] });
const started = new Date().toISOString();
const blockedExternal = new Map(); const rows = []; const linkMap = new Map(); const screenshotRows = [];
const navigationChecks = [];
const report = { scriptSha256: sha(await readFile(process.argv[1])), gitHead: execFileSync('git', ['-C', repo, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim(), navigationChecks, startedUtc: started, command: { argv: process.argv, cwd: process.cwd() }, inputManifest: { path: manifestPath, sha256: sha(manifestBytes), compared: Object.keys(manifest).length, mismatches }, node: process.version, playwrightVersion, playwrightModule, browserVersion: browser.version(), executable, headless: true, serverOrigin: origin, sourceCount: sources.length, sources, viewports: [{ name: 'desktop', width: 1440, height: 1000 }, { name: 'mobile', width: 390, height: 844 }], pages: rows, screenshots: screenshotRows };
const safeName = route => route.replace(/^\/|\/$/g, '').replaceAll('/', '__');
try {
  for (const viewport of report.viewports) {
    const context = await browser.newContext({ viewport: { width: viewport.width, height: viewport.height }, deviceScaleFactor: 1, isMobile: viewport.name === 'mobile', hasTouch: viewport.name === 'mobile', colorScheme: 'light', reducedMotion: 'reduce' });
    await context.route('**/*', async route => {
      const url = new URL(route.request().url());
      if (url.origin === origin || ['data:', 'blob:'].includes(url.protocol)) return route.continue();
      const key = url.origin + url.pathname; blockedExternal.set(key, (blockedExternal.get(key) ?? 0) + 1); return route.abort('blockedbyclient');
    });
    let next = 0;
    await Promise.all(Array.from({ length: 3 }, async () => {
      const page = await context.newPage();
      while (next < sources.length) {
        const source = sources[next++]; const errors = []; const failures = [];
        const onError = e => errors.push({ type: 'pageerror', text: String(e) });
        const onConsole = msg => { if (msg.type() === 'error') errors.push({ type: 'console', text: msg.text(), location: msg.location() }); };
        const onFailed = req => failures.push({ url: req.url(), failure: req.failure()?.errorText });
        page.on('pageerror', onError); page.on('console', onConsole); page.on('requestfailed', onFailed);
        const row = { route: source.route, viewport: viewport.name, errors, requestFailures: failures };
        const start = Date.now();
        try {
          const response = await page.goto(origin + source.route, { waitUntil: 'networkidle', timeout: 30000 });
          row.status = response?.status();
          await page.locator('#nd-page h1').waitFor({ state: 'visible', timeout: 10000 });
          await page.evaluate(() => document.fonts.ready);
          const data = await page.evaluate(() => {
            const main = document.querySelector('#nd-page');
            const visible = el => { if (!el) return false; const rect = el.getBoundingClientRect(); return !!(rect.width && rect.height && rect.right > 0 && rect.left < innerWidth && rect.bottom > 0 && rect.top < innerHeight && getComputedStyle(el).visibility !== 'hidden'); };
            const h1 = [...document.querySelectorAll('h1')].filter(visible).map(el => el.innerText);
            const nav = [...document.querySelectorAll('nav,[role="navigation"],#nd-sidebar')].map(el => ({ tag: el.tagName, id: el.id, visible: visible(el), links: el.querySelectorAll('a[href]').length }));
            const width = document.documentElement.clientWidth;
            const overflow = [...(main?.querySelectorAll('*') ?? [])].filter(el => visible(el) && (el.getBoundingClientRect().right > width + 2 || el.getBoundingClientRect().left < -2)).slice(0, 10).map(el => ({ tag: el.tagName, class: el.className?.baseVal ?? el.className, text: el.textContent?.slice(0, 90), width: el.getBoundingClientRect().width }));
            return { h1, documentTitle: document.title, bodyCharacters: main?.innerText.length ?? 0, currentMarker: main?.innerText.includes('Research state: Current') ?? false, nav, openSidebarVisible: [...document.querySelectorAll('button[aria-label="Open Sidebar"]')].some(visible), documentWidth: document.documentElement.scrollWidth, viewportWidth: width, horizontalOverflow: document.documentElement.scrollWidth > width + 2, overflowCandidates: overflow, ids: [...document.querySelectorAll('[id]')].map(el => el.id), links: [...document.querySelectorAll('#nd-page a[href],#nd-sidebar a[href]')].map(el => ({ href: el.href, text: el.innerText?.slice(0, 100) })), literalMermaid: main?.innerText.includes('flowchart TD') ?? false };
          });
          row.h1 = data.h1; row.titleMatches = data.h1.length === 1 && normalize(data.h1[0]) === normalize(source.expectedTitle);
          row.bodyCharacters = data.bodyCharacters; row.currentMarker = data.currentMarker; row.documentTitle = data.documentTitle;
          row.navigation = data.nav; row.mobileNavTriggerVisible = data.openSidebarVisible;
          row.width = { document: data.documentWidth, viewport: data.viewportWidth, overflow: data.horizontalOverflow, candidates: data.horizontalOverflow ? data.overflowCandidates : [] };
          row.ids = data.ids; row.literalMermaid = data.literalMermaid;
          for (const link of data.links) {
            const url = new URL(link.href);
            if (url.origin !== origin && url.origin !== 'https://jackin.tailrocks.com') continue;
            const key = url.pathname + url.search + url.hash;
            const old = linkMap.get(key) ?? { target: key, sources: new Set() }; old.sources.add(source.route); linkMap.set(key, old);
          }
          if (representatives.has(source.route)) {
            const top = `${safeName(source.route)}__${viewport.name}__top.png`;
            await page.screenshot({ path: path.join(out, 'screenshots', top), animations: 'disabled' });
            screenshotRows.push({ route: source.route, viewport: viewport.name, region: 'top', path: 'screenshots/' + top });
            const target = page.locator('#nd-page table').first();
            if (await target.count()) {
              await target.evaluate(el => window.scrollTo(0, Math.max(0, el.getBoundingClientRect().top + window.scrollY - 90)));
              const detail = `${safeName(source.route)}__${viewport.name}__table.png`;
              await page.screenshot({ path: path.join(out, 'screenshots', detail), animations: 'disabled' });
              screenshotRows.push({ route: source.route, viewport: viewport.name, region: 'first table', path: 'screenshots/' + detail });
            }
          }
          row.passed = row.status === 200 && row.titleMatches && row.bodyCharacters > 100 && (!source.expectsCurrent || row.currentMarker) && (viewport.name === 'mobile' ? row.mobileNavTriggerVisible : data.nav.some(x => x.links > 0 && x.visible)) && !row.width.overflow && !row.literalMermaid && errors.length === 0 && failures.filter(x => x.url.startsWith(origin)).length === 0;
        } catch (error) { row.exception = String(error); row.passed = false; }
        row.elapsedMs = Date.now() - start; rows.push(row);
        page.off('pageerror', onError); page.off('console', onConsole); page.off('requestfailed', onFailed);
        console.log(JSON.stringify({ route: row.route, viewport: row.viewport, passed: row.passed, h1: row.h1, errors: errors.length, overflow: row.width?.overflow, exception: row.exception }));
      }
      await page.close();
    }));
    const navPage = await context.newPage();
    const navCheck = { viewport: viewport.name, startRoute: '/research/context/', errors: [], failedResponses: [] };
    navPage.on('pageerror', error => navCheck.errors.push(String(error)));
    navPage.on('console', msg => { if (msg.type() === 'error') navCheck.errors.push(msg.text()); });
    navPage.on('response', response => { if (response.status() >= 400) navCheck.failedResponses.push({ url: response.url(), status: response.status() }); });
    try {
      await navPage.goto(origin + navCheck.startRoute, { waitUntil: 'networkidle', timeout: 30000 });
      if (viewport.name === 'mobile') {
        await navPage.getByRole('button', { name: 'Open Sidebar', exact: true }).click();
        navCheck.openSidebarClicked = true;
        await navPage.waitForFunction(() => [...document.querySelectorAll('#nd-sidebar-mobile a[href^="/research"]')].some(el => { const rect = el.getBoundingClientRect(); return rect.width > 0 && rect.height > 0 && rect.left < innerWidth && rect.right > 0; }), null, { timeout: 10000 });
      }
      const candidate = await navPage.evaluate(() => {
        const el = [...document.querySelectorAll('aside a[href],[role="dialog"] a[href]')].find(el => {
          const rect = el.getBoundingClientRect();
          return rect.width > 0 && rect.height > 0 && rect.right > 0 && rect.left < innerWidth && rect.bottom > 0 && rect.top < innerHeight && getComputedStyle(el).visibility !== 'hidden' && el.getAttribute('href')?.startsWith('/research') && new URL(el.href).pathname.replace(/\/$/, '') !== location.pathname.replace(/\/$/, '');
        });
        if (!el) return null;
        el.setAttribute('data-research-nav-check', 'target');
        return { href: el.getAttribute('href'), text: el.innerText };
      });
      if (!candidate) throw new Error('No visible internal research navigation link after opening sidebar');
      navCheck.clicked = candidate;
      const targetSlug = new URL(candidate.href, origin).pathname.replace(/^\/|\/$/g, '');
      let targetSource;
      try { targetSource = await readFile(path.join(contentRoot, targetSlug + '.mdx'), 'utf8'); }
      catch { targetSource = await readFile(path.join(contentRoot, targetSlug, 'index.mdx'), 'utf8'); }
      const targetTitleField = targetSource.match(/^title:\s*(.+)$/m)?.[1]?.trim() ?? '';
      navCheck.expectedTitle = targetTitleField.startsWith('"') ? JSON.parse(targetTitleField) : targetTitleField.replace(/^'|'$/g, '');
      await navPage.locator('aside [data-research-nav-check="target"],[role="dialog"] [data-research-nav-check="target"]').click();
      await navPage.waitForURL(url => url.pathname.replace(/\/$/, '') === new URL(candidate.href, origin).pathname.replace(/\/$/, ''), { timeout: 15000 });
      await navPage.waitForFunction(expected => document.querySelector('#nd-page h1')?.innerText.replace(/\s+/g, ' ').trim() === expected, normalize(navCheck.expectedTitle), { timeout: 15000 });
      await navPage.waitForLoadState('networkidle');
      navCheck.finalUrl = navPage.url(); navCheck.h1 = await navPage.locator('#nd-page h1').innerText(); navCheck.passed = normalize(navCheck.h1) === normalize(navCheck.expectedTitle) && navCheck.errors.length === 0 && navCheck.failedResponses.length === 0;
    } catch (error) { navCheck.passed = false; navCheck.error = String(error); }
    navigationChecks.push(navCheck); await navPage.close();
    await context.close();
  }
  const linkChecks = []; const links = [...linkMap.values()]; let nextLink = 0;
  const request = await playwright.request.newContext();
  await Promise.all(Array.from({ length: 6 }, async () => {
    while (nextLink < links.length) {
      const link = links[nextLink++]; const url = new URL(link.target, origin); const target = url.pathname.endsWith('/') ? url.pathname : url.pathname + '/';
      const check = { target: link.target, sources: [...link.sources] };
      try {
        const response = await request.get(origin + url.pathname + url.search, { timeout: 15000 }); check.status = response.status(); check.ok = response.ok();
        if (url.hash) {
          const id = decodeURIComponent(url.hash.slice(1)); const rendered = rows.find(x => x.route === target && x.viewport === 'desktop');
          check.fragment = id; check.fragmentCheckedAgainstRenderedDOM = !!rendered;
          if (rendered) { check.fragmentExists = rendered.ids?.includes(id) ?? false; check.ok &&= check.fragmentExists; }
          else check.fragmentLimit = 'Target outside the 56-page rendered sweep; HTTP status only.';
        }
      } catch (error) { check.ok = false; check.error = String(error); }
      linkChecks.push(check);
    }
  }));
  await request.dispose(); report.internalLinks = linkChecks;
  report.externalRequestsBlocked = [...blockedExternal].map(([url, count]) => ({ url, count }));
  report.finishedUtc = new Date().toISOString();
  report.endingInputMismatches = [];
  for (const [file, expected] of Object.entries(manifest)) {
    const current = sha(await readFile(path.join(repo, file)));
    if (current !== expected) report.endingInputMismatches.push({ file, expected, current });
  }
  report.summary = { routes: sources.length, pageViewportChecks: rows.length, passedPageChecks: rows.filter(x => x.passed).length, failedPageChecks: rows.filter(x => !x.passed).length, uniqueInternalLinks: linkChecks.length, failedInternalLinks: linkChecks.filter(x => !x.ok).length, screenshots: screenshotRows.length, consoleOrPageErrors: rows.reduce((n, x) => n + x.errors.length, 0), globalOverflowChecksFailed: rows.filter(x => x.width?.overflow).length };
  report.summary.failedNavigationChecks = navigationChecks.filter(x => !x.passed).length;
  report.passed = report.summary.failedPageChecks === 0 && report.summary.failedInternalLinks === 0 && report.summary.failedNavigationChecks === 0 && report.endingInputMismatches.length === 0;
  report.limits = ['Chromium desktop/mobile viewport checks, not physical devices or a complete accessibility audit.', 'Per-page navigation verifies visible desktop navigation or a visible mobile trigger; actual scoped sidebar-link interaction is tested on the context overview at each viewport, not every menu branch.', 'External HTTP requests blocked; external link availability is outside this local rendering sweep.', 'All rendered context-page fragments checked against DOM; other internal targets receive HTTP-status checks only.', 'Screenshots require separate human/model visual inspection; this JSON does not claim that inspection.', 'No application source mutation, provider call, user session or public server.'];
  await writeFile(path.join(out, 'rendered-pages-results.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ summary: report.summary, passed: report.passed }));
  process.exitCode = report.passed ? 0 : 1;
} finally { await browser.close(); }
