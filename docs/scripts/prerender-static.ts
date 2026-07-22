import { cp, mkdir, readdir, rm, writeFile } from 'node:fs/promises'
import { readdirSync } from 'node:fs'
import { availableParallelism } from 'node:os'
import { dirname, join, relative, sep } from 'node:path'

const root = join(import.meta.dirname, '..')
const contentRoot = join(root, 'content', 'docs')
const outDir = join(root, '.output', 'public')
const host = '127.0.0.1'
const port = 4173
const origin = `http://${host}:${port}`
const requestConcurrency = Math.max(8, availableParallelism() * 8)

function docsSlugs(dir = contentRoot): string[] {
  const entries = readdirSync(dir, { withFileTypes: true })
  return entries.flatMap((entry) => {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) return docsSlugs(path)
    if (!entry.isFile() || !entry.name.endsWith('.mdx')) return []

    const rel = relative(contentRoot, path)
      .split(sep)
      .filter((part) => !part.startsWith('(') || !part.endsWith(')'))
      .join('/')
    const withoutExt = rel.replace(/\.mdx$/, '')
    return withoutExt.endsWith('/index')
      ? [withoutExt.slice(0, -'/index'.length)]
      : [withoutExt]
  })
}

function pagePath(slug: string): string {
  return `/${slug}`.replace(/\/+/g, '/')
}

function outputPath(path: string): string {
  if (path === '/') return join(outDir, 'index.html')
  if (path === '/404') return join(outDir, '404.html')
  if (path.endsWith('.md') || path.endsWith('.txt') || path.endsWith('.xml') || path.endsWith('.webp')) {
    return join(outDir, path.slice(1))
  }
  if (path === '/api/search') return join(outDir, 'api', 'search')
  return join(outDir, path.slice(1), 'index.html')
}

async function waitForServer() {
  const deadline = Date.now() + 20_000
  let lastError: unknown
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${origin}/`)
      if (response.ok) return
      lastError = new Error(`preview returned ${response.status}`)
    } catch (error) {
      lastError = error
    }
    await new Promise((resolve) => setTimeout(resolve, 250))
  }
  throw lastError instanceof Error ? lastError : new Error('preview server did not start')
}

async function fetchStatic(path: string) {
  const response = await fetch(`${origin}${path}`)
  if (!response.ok) {
    throw new Error(`Failed to prerender ${path}: ${response.status} ${response.statusText}`)
  }

  const target = outputPath(path)
  await mkdir(dirname(target), { recursive: true })
  await writeFile(target, Buffer.from(await response.arrayBuffer()))
}

async function fetchAllStatic(paths: string[]) {
  let next = 0
  await Promise.all(
    Array.from({ length: Math.min(requestConcurrency, paths.length) }, async () => {
      while (next < paths.length) {
        const path = paths[next]
        next += 1
        await fetchStatic(path)
      }
    }),
  )
}

async function drain(
  stream: ReadableStream<Uint8Array>,
  target: NodeJS.WriteStream,
): Promise<string> {
  const reader = stream.getReader()
  const decoder = new TextDecoder()
  let output = ''
  while (true) {
    const { done, value } = await reader.read()
    if (done) break
    target.write(value)
    output += decoder.decode(value, { stream: true })
  }
  output += decoder.decode()
  return output
}

async function copySsrAssets() {
  const ssrAssetsDir = join(root, 'node_modules', '.nitro', 'vite', 'services', 'ssr', 'assets')
  const publicAssetsDir = join(outDir, 'assets')
  await mkdir(publicAssetsDir, { recursive: true })

  const entries = await readdir(ssrAssetsDir, { withFileTypes: true })
  await Promise.all(
    entries.map((entry) => {
      const source = join(ssrAssetsDir, entry.name)
      const target = join(publicAssetsDir, entry.name)
      return cp(source, target, { force: true, recursive: entry.isDirectory() })
    }),
  )
}

const pages = docsSlugs().flatMap((slug) => {
  const path = pagePath(slug)
  return [path, `${path}.md`, `/og/${slug}.webp`]
})

const paths = [
  '/',
  '/404',
  '/api/search',
  '/llms.txt',
  '/llms-full.txt',
  '/sitemap.xml',
  ...pages,
]

const child = Bun.spawn(
  ['bunx', 'vite', 'preview', '--host', host, '--port', String(port), '--strictPort'],
  {
    cwd: root,
    env: {
      ...process.env,
      TSS_CLIENT_OUTPUT_DIR: outDir,
    },
    stdout: 'pipe',
    stderr: 'pipe',
  },
)

const stdout = drain(child.stdout, process.stdout)
const stderr = drain(child.stderr, process.stderr)

try {
  await waitForServer()
  await fetchAllStatic(paths)
  await copySsrAssets()
  await rm(join(outDir, '404', 'index.html'), { force: true })
  console.log(`[prerender-static] wrote ${paths.length} static routes`)
} finally {
  child.kill()
  await child.exited.catch(() => undefined)
}

await stdout
const serverErrors = await stderr
if (serverErrors.includes('Error in renderToReadableStream')) {
  throw new Error('preview server reported an SSR render failure')
}
