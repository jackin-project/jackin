// SPDX-License-Identifier: Apache-2.0
import { spawn } from 'node:child_process'
import { mkdir, writeFile } from 'node:fs/promises'
import { resolve } from 'node:path'
import net from 'node:net'

const [docsArg, outputArg, browserArg] = process.argv.slice(2)
if (!docsArg || !outputArg || !browserArg) throw new Error('usage: probe.mjs DOCS OUTPUT BROWSER')
const docs = resolve(docsArg)
const output = resolve(outputArg)
await mkdir(output, { recursive: false })
const profile = `${output}/profile`
await mkdir(profile)
const started = Date.now()
const checks = []
const requests = new Map()
const exceptions = []
const requestIds = new Map()
const blocked = []
let phase = 'setup'
let pageState
let serverLog = ''
let diagnostics = ''
let browser
let staticServer
let ws
let sessionId
let nextId = 1
const pending = new Map()
const wait = ms => new Promise(done => setTimeout(done, ms))
function require(condition, message) {
  if (!condition) throw new Error(message)
  checks.push(message)
}
async function until(predicate, message, seconds = 12) {
  const limit = Date.now() + seconds * 1000
  while (Date.now() < limit) {
    const value = await predicate()
    if (value) return value
    await wait(50)
  }
  throw new Error(message)
}
async function freePort() {
  const server = net.createServer()
  await new Promise((ok, fail) => server.once('error', fail).listen(0, '127.0.0.1', ok))
  const { port } = server.address()
  await new Promise((ok, fail) => server.close(error => error ? fail(error) : ok()))
  return port
}
function rpc(method, params = {}, target = sessionId) {
  const id = nextId++
  return new Promise((ok, fail) => {
    const timer = setTimeout(() => { pending.delete(id); fail(new Error(`CDP timeout: ${method}`)) }, 15000)
    pending.set(id, { ok, fail, timer })
    ws.send(JSON.stringify({ id, method, params, ...(target ? { sessionId: target } : {}) }))
  })
}
async function evaluate(expression) {
  const result = await rpc('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
  if (result.exceptionDetails) throw new Error(`Page evaluation failed: ${result.exceptionDetails.exception?.description ?? result.exceptionDetails.text}`)
  return result.result.value
}
async function stop(child) {
  if (!child || child.exitCode !== null || child.signalCode !== null) return
  const exited = new Promise(ok => child.once('exit', ok))
  child.kill('SIGTERM')
  await Promise.race([exited, wait(5000)])
  if (child.exitCode === null && child.signalCode === null) throw new Error('owned child did not terminate after SIGTERM')
}
let error
try {
  const port = await freePort()
  const env = { PATH: process.env.PATH, HOME: output, TMPDIR: output, LANG: 'en_US.UTF-8', CI: '1' }
  staticServer = spawn(process.execPath, [`${docs}/node_modules/serve/build/main.js`, '.output/public', '--config', 'serve.json', '--listen', `tcp://127.0.0.1:${port}`], { cwd: docs, env, stdio: ['ignore', 'pipe', 'pipe'] })
  staticServer.stdout.on('data', data => { serverLog += data.toString() })
  staticServer.stderr.on('data', data => { serverLog += data.toString() })
  const base = `http://127.0.0.1:${port}`
  await until(async () => { try { return (await fetch(base)).status === 200 } catch { return false } }, 'static server became ready')
  browser = spawn(resolve(browserArg), ['--headless=new', '--disable-gpu', '--disable-background-networking', '--no-proxy-server', '--no-first-run', '--no-default-browser-check', '--remote-debugging-address=127.0.0.1', '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], { env, stdio: ['ignore', 'ignore', 'pipe'] })
  browser.stderr.on('data', data => { diagnostics += data.toString() })
  const endpoint = await until(() => diagnostics.match(/DevTools listening on (ws:\/\/[^\s]+)/)?.[1], 'isolated browser debug endpoint became ready')
  ws = new WebSocket(endpoint)
  ws.addEventListener('message', event => {
    const message = JSON.parse(event.data)
    if (message.id) {
      const entry = pending.get(message.id)
      if (!entry) return
      clearTimeout(entry.timer)
      pending.delete(message.id)
      if (message.error) entry.fail(new Error(JSON.stringify(message.error)))
      else entry.ok(message.result)
    } else if (message.method === 'Network.requestWillBeSent') {
      const url = message.params.request.url
      if (url.startsWith(base)) {
        const path = url.slice(base.length)
        requestIds.set(message.params.requestId, path)
        requests.set(path, { ...(requests.get(path) ?? {}), requested: true, requestCount: (requests.get(path)?.requestCount ?? 0) + 1, ...(message.params.redirectResponse ? { redirectStatus: message.params.redirectResponse.status } : {}) })
      }
    } else if (message.method === 'Network.loadingFailed') {
      const path = requestIds.get(message.params.requestId)
      if (path) requests.set(path, { ...(requests.get(path) ?? {}), loadingFailed: message.params.errorText })
    } else if (message.method === 'Fetch.requestPaused') {
      const { request, requestId } = message.params
      const allowed = request.url.startsWith(`${base}/`)
      if (!allowed) blocked.push(request.url)
      rpc(allowed ? 'Fetch.continueRequest' : 'Fetch.failRequest', { requestId, ...(allowed ? {} : { errorReason: 'BlockedByClient' }) }).catch(failure => exceptions.push(failure.message))
    } else if (message.method === 'Network.responseReceived') {
      const response = message.params.response
      if (response.url.startsWith(base)) requests.set(response.url.slice(base.length), { ...(requests.get(response.url.slice(base.length)) ?? {}), status: response.status, mimeType: response.mimeType })
    } else if (message.method === 'Runtime.exceptionThrown') {
      exceptions.push(message.params.exceptionDetails.exception?.description ?? message.params.exceptionDetails.text)
    } else if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
      exceptions.push(message.params.args.map(item => item.value ?? item.description).join(' '))
    }
  })
  await new Promise((ok, fail) => { ws.addEventListener('open', ok, { once: true }); ws.addEventListener('error', fail, { once: true }) })
  const version = await rpc('Browser.getVersion', {}, undefined)
  const target = await rpc('Target.createTarget', { url: 'about:blank' }, undefined)
  const attached = await rpc('Target.attachToTarget', { targetId: target.targetId, flatten: true }, undefined)
  sessionId = attached.sessionId
  await rpc('Page.enable')
  await rpc('Runtime.enable')
  require(await evaluate('1 + 1') === 2, 'isolated browser executes JavaScript before loading the app')
  await rpc('Network.enable')
  await rpc('Fetch.enable', { patterns: [{ urlPattern: '*', requestStage: 'Request' }] })
  await rpc('Emulation.setDeviceMetricsOverride', { width: 1280, height: 900, deviceScaleFactor: 1, mobile: false })
  phase = 'navigate'
  const navigation = await rpc('Page.navigate', { url: `${base}/guides/security-model/` })
  require(!navigation.errorText, 'browser navigation had no transport error')
  await until(() => evaluate('document.readyState === "complete" && document.body.innerText.includes("Security model")'), 'security documentation rendered')
  require(await evaluate('document.title === "Security model - jackin❯"'), 'documentation has its own title')
  phase = 'open-search'
  const searchButton = await evaluate('Array.from(document.querySelectorAll("button")).filter(b=>((b.getAttribute("aria-label")??"")+" "+b.innerText).trim().startsWith("Search")).map(b=>b.getBoundingClientRect()).filter(r=>r.width>0&&r.height>0).map(r=>({x:r.x+r.width/2,y:r.y+r.height/2}))[0]')
  require(Boolean(searchButton), 'visible Search button exists')
  await rpc('Input.dispatchMouseEvent', { type: 'mousePressed', ...searchButton, button: 'left', clickCount: 1 })
  await rpc('Input.dispatchMouseEvent', { type: 'mouseReleased', ...searchButton, button: 'left', clickCount: 1 })
  await until(() => evaluate('Boolean(document.querySelector("[role=dialog] input"))'), 'hydrated search dialog opened')
  await evaluate('document.querySelector("[role=dialog] input").focus()')
  await rpc('Input.insertText', { text: 'construct' })
  phase = 'search-result'
  const hit = await until(() => evaluate('Array.from(document.querySelectorAll("[role=dialog] [role=option]")).filter(e=>e.innerText.includes("The construct image")).map(e=>({id:e.id,text:e.innerText}))[0]'), 'actual search did not return the construct-image page')
  require(Boolean(hit), 'real search found the construct-image page')
  require(requests.get('/api/search.json')?.status === 200, 'browser fetched the actual static JSON index successfully')
  require(!requests.has('/api/search') && !requests.has('/api/search/'), 'browser did not fetch an extensionless search endpoint')
  await writeFile(`${output}/search-hit.png`, Buffer.from((await rpc('Page.captureScreenshot', { format: 'png' })).data, 'base64'))
  await evaluate('document.querySelector("[role=dialog] input").select()')
  await rpc('Input.insertText', { text: 'jackin-no-search-match-20261109' })
  await until(() => evaluate('document.querySelector("[role=dialog]")?.innerText.includes("No results found")'), 'nonexistent search query did not report no results')
  require(await evaluate('document.querySelectorAll("[role=dialog] [role=option]").length === 0'), 'nonexistent query has no result options')
  await writeFile(`${output}/search-miss.png`, Buffer.from((await rpc('Page.captureScreenshot', { format: 'png' })).data, 'base64'))
  await evaluate('document.querySelector("[role=dialog] input").select()')
  await rpc('Input.insertText', { text: 'construct' })
  await until(() => evaluate('Array.from(document.querySelectorAll("[role=dialog] [role=option]")).some(e=>e.innerText.includes("The construct image"))'), 'search result restored')
  await evaluate('Array.from(document.querySelectorAll("[role=dialog] [role=option]")).find(e=>e.innerText.includes("The construct image")).click()')
  await until(() => evaluate('location.pathname.startsWith("/developing/construct-image") && document.body.innerText.includes("The construct image is the base")'), 'search result navigation failed')
  require(true, 'search result navigated to actual hydrated document content')
  const title = 'The construct image - jackin❯'
  await until(() => evaluate(`document.title === ${JSON.stringify(title)}`), 'search navigation did not update the document title')
  const documentPath = await evaluate('location.pathname')
  const previousRequests = requests.get(documentPath)?.requestCount ?? 0
  await rpc('Page.reload')
  await until(async () => (requests.get(documentPath)?.requestCount ?? 0) > previousRequests && await evaluate(`document.readyState === "complete" && document.title === ${JSON.stringify(title)} && document.body.innerText.includes("The construct image is the base")`), 'direct document reload failed')
  require(await evaluate('document.title') === title, 'direct reload preserved the document title and content')
  await evaluate(`document.querySelector('a[href="/"]').click()`)
  await until(() => evaluate('location.pathname === "/" && document.body.innerText.includes("OpenShell")'), 'home navigation failed')
  require(true, 'home navigation rendered actual landing content')
  await evaluate('history.back(); true')
  await until(() => evaluate(`location.pathname.startsWith("/developing/construct-image") && document.title === ${JSON.stringify(title)} && document.body.innerText.includes("The construct image is the base")`), 'browser Back failed')
  require(await evaluate('document.title') === title, 'browser Back restored document title and content')
  await evaluate('history.forward(); true')
  await until(() => evaluate('location.pathname === "/" && document.body.innerText.includes("OpenShell")'), 'browser Forward failed')
  require(true, 'browser Forward restored the landing page')
  require(exceptions.length === 0, 'browser reported no runtime or hydration errors')
  await writeFile(`${output}/version.json`, JSON.stringify({ product: version.product, protocolVersion: version.protocolVersion }, null, 2))
} catch (failure) {
  error = failure instanceof Error ? failure.message : String(failure)
  if (sessionId && ws?.readyState === WebSocket.OPEN) {
    try {
      pageState = await evaluate('({ url:location.pathname, title:document.title, ready:document.readyState, body:document.body?.innerText.slice(0,12000), dialog:document.querySelector("[role=dialog]")?.innerText, query:document.querySelector("[role=dialog] input")?.value, optionCount:document.querySelectorAll("[role=dialog] [role=option]").length, loadingIndicator:!!document.querySelector("[role=dialog] .animate-pulse") })')
      await writeFile(`${output}/failure.png`, Buffer.from((await rpc('Page.captureScreenshot', { format: 'png' })).data, 'base64'))
    } catch {}
  }
} finally {
  if (ws?.readyState === WebSocket.OPEN) {
    try { await rpc('Browser.close', {}, null) } catch {}
    ws.close()
  }
  try { await stop(browser) } catch (failure) { error = `${error ?? ''} ${failure.message}` }
  try { await stop(staticServer) } catch (failure) { error = `${error ?? ''} ${failure.message}` }
  await writeFile(`${output}/server.log`, serverLog)
  await writeFile(`${output}/browser-diagnostics.log`, diagnostics.replace(/ws:\/\/[^\s]+/g, '[private debugging endpoint]'))
  const result = { actual_exit: error ? 1 : 0, elapsed_seconds: (Date.now() - started) / 1000, phase, checks, requests: Object.fromEntries(requests), blocked_page_requests: blocked, exceptions, pageState, ...(error ? { error } : {}), cleanup: { browser_exit: browser?.exitCode, browser_signal: browser?.signalCode, server_exit: staticServer?.exitCode, server_signal: staticServer?.signalCode } }
  await writeFile(`${output}/result.json`, JSON.stringify(result, null, 2) + '\n')
  console.log(JSON.stringify(result))
  process.exitCode = result.actual_exit
}
