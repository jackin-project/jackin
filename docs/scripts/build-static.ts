import { constants } from 'node:fs'
import { copyFile, lstat, mkdir, mkdtemp, readFile, readdir, realpath, rename, rm, writeFile } from 'node:fs/promises'
import { dirname, isAbsolute, join, relative, sep } from 'node:path'
import { pathToFileURL } from 'node:url'
import { postInstall } from 'fumadocs-mdx/vite'
import { build } from 'vite'
import type { ClientAssets } from '../src/entry-server'
import type { createInventory } from '../src/build/inventory.server'
import type { render } from '../src/entry-server'

const root = await realpath(process.cwd())
const outputParent = join(root, '.output')
await mkdir(outputParent, { recursive: true })
const outputParentInfo = await lstat(outputParent)
if (outputParentInfo.isSymbolicLink() || !outputParentInfo.isDirectory()) {
  throw new Error(`Unsafe output directory: ${outputParent}`)
}
if (await realpath(outputParent) !== outputParent) throw new Error(`Output directory resolves outside the project: ${outputParent}`)
const output = await mkdtemp(join(outputParent, 'static-build-'))
const publicDir = join(output, 'public')
const privateDir = join(output, 'private')

async function assertDirectoryTree(directory: string, allowMissing: boolean): Promise<void> {
  let info
  try {
    info = await lstat(directory)
  } catch (error) {
    if (allowMissing && error instanceof Error && 'code' in error && error.code === 'ENOENT') return
    throw error
  }
  if (info.isSymbolicLink() || !info.isDirectory()) throw new Error(`Unsafe directory: ${directory}`)
  const resolved = await realpath(directory)
  const resolvedRel = relative(root, resolved)
  if (isAbsolute(resolvedRel) || resolvedRel === '..' || resolvedRel.startsWith(`..${sep}`)) {
    throw new Error(`Directory resolves outside the project: ${directory}`)
  }
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isSymbolicLink()) throw new Error(`Symlink in input tree: ${path}`)
    if (entry.isDirectory()) await assertDirectoryTree(path, false)
  }
}

await assertDirectoryTree(join(root, 'content'), false)
await assertDirectoryTree(join(root, 'public'), true)
await assertDirectoryTree(join(root, '.source'), true)
await postInstall()
await assertDirectoryTree(join(root, '.source'), false)
await build({
  build: {
    ssr: 'src/build/inventory.server.ts',
    outDir: join(privateDir, 'inventory'),
    rollupOptions: { output: { entryFileNames: 'inventory.js' } },
  },
})
const inventoryModule: { createInventory: typeof createInventory } = await import(
  pathToFileURL(join(privateDir, 'inventory/inventory.js')).href
)
const inventory = await inventoryModule.createInventory()
const snapshot = JSON.stringify(inventory.pages)
if (!snapshot) throw new Error('Page catalog is not serializable')
const sourceDir = join(root, '.source')
const sourceDirInfo = await lstat(sourceDir)
if (sourceDirInfo.isSymbolicLink() || !sourceDirInfo.isDirectory()) throw new Error(`Unsafe source directory: ${sourceDir}`)
const generatedPages = join(sourceDir, 'pages.generated.ts')
try {
  const generatedPagesInfo = await lstat(generatedPages)
  if (generatedPagesInfo.isSymbolicLink() || !generatedPagesInfo.isFile()) throw new Error(`Unsafe generated pages file: ${generatedPages}`)
} catch (error) {
  if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error
}
await writeFile(join(root, '.source/pages.generated.ts'),
  `import type { PageData } from '../src/build/page-data'\nexport const pages: Record<string, PageData> = ${snapshot}\n`)

await assertDirectoryTree(join(root, 'public'), true)
await build({
  build: {
    outDir: publicDir,
    manifest: true,
    rollupOptions: { input: 'src/entry-client.tsx' },
  },
})
await copyFile(join(root, 'serve.json'), join(publicDir, 'serve.json'), constants.COPYFILE_EXCL)
async function rejectSymlinks(directory: string): Promise<void> {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name)
    if (entry.isSymbolicLink()) throw new Error(`Symlink in static output: ${path}`)
    if (entry.isDirectory()) await rejectSymlinks(path)
  }
}
await rejectSymlinks(publicDir)
const manifest: unknown = JSON.parse(await readFile(join(publicDir, '.vite/manifest.json'), 'utf8'))
function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}
if (!isRecord(manifest)) throw new Error('Invalid client manifest')
const visited = new Set<string>()
const css = new Set<string>()
function visit(key: string) {
  if (visited.has(key)) return
  visited.add(key)
  if (!isRecord(manifest)) throw new Error('Invalid client manifest')
  const entry = manifest[key]
  if (!isRecord(entry)) throw new Error(`Missing client manifest entry: ${key}`)
  if (entry.css !== undefined) {
    if (!Array.isArray(entry.css) || !entry.css.every((item) => typeof item === 'string')) {
      throw new Error(`Invalid CSS manifest entry: ${key}`)
    }
    for (const item of entry.css) css.add(`/${item}`)
  }
  if (entry.imports !== undefined) {
    if (!Array.isArray(entry.imports) || !entry.imports.every((item) => typeof item === 'string')) {
      throw new Error(`Invalid imports manifest entry: ${key}`)
    }
    for (const item of entry.imports) visit(item)
  }
}
const entry = manifest['src/entry-client.tsx']
if (!isRecord(entry) || typeof entry.file !== 'string') throw new Error('Missing client entry')
visit('src/entry-client.tsx')
const assets: ClientAssets = { script: `/${entry.file}`, css: [...css] }

await build({
  build: {
    ssr: 'src/entry-server.tsx',
    outDir: join(privateDir, 'renderer'),
    rollupOptions: { output: { entryFileNames: 'renderer.js' } },
  },
})
const renderer: { render: typeof render } = await import(
  pathToFileURL(join(privateDir, 'renderer/renderer.js')).href
)
async function writeResponse(path: string, response: Response, expectedStatus: number) {
  if (response.status !== expectedStatus) throw new Error(`${path}: HTTP ${response.status}, expected ${expectedStatus}`)
  const target = join(publicDir, path)
  await ensureOutputParent(target)
  await writeFile(target, new Uint8Array(await response.arrayBuffer()), { flag: 'wx' })
}
const publicRoot = await realpath(publicDir)
async function ensureOutputParent(target: string) {
  const parent = dirname(target)
  const rel = relative(publicDir, parent)
  if (isAbsolute(rel) || rel === '..' || rel.startsWith(`..${sep}`)) throw new Error(`Output escapes staging root: ${target}`)
  let current = publicDir
  for (const part of rel.split(sep).filter(Boolean)) {
    current = join(current, part)
    try {
      const info = await lstat(current)
      if (info.isSymbolicLink() || !info.isDirectory()) throw new Error(`Unsafe output parent: ${current}`)
    } catch (error) {
      if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error
      await mkdir(current)
    }
    const resolved = await realpath(current)
    const resolvedRel = relative(publicRoot, resolved)
    if (isAbsolute(resolvedRel) || resolvedRel === '..' || resolvedRel.startsWith(`..${sep}`)) {
      throw new Error(`Resolved output parent escapes staging root: ${current}`)
    }
  }
}
const destinations = new Set<string>()
function reserve(path: string) {
  if (!path || path.startsWith('/') || path.split('/').some((part) => !part || part === '.' || part === '..') || path.includes('\\')) {
    throw new Error(`Unsafe destination: ${path}`)
  }
  if (destinations.has(path)) throw new Error(`Duplicate destination: ${path}`)
  destinations.add(path)
}
const htmlArtifacts = [
  { url: '/', path: 'index.html', status: 200 },
  ...Object.keys(inventory.pages).map((url) => ({ url, path: `${url.replace(/^\/|\/$/g, '')}/index.html`, status: 200 })),
  { url: '/404', path: '404.html', status: 200 },
]
for (const artifact of htmlArtifacts) {
  reserve(artifact.path)
  await writeResponse(artifact.path, await renderer.render(new Request(`http://localhost${artifact.url}`), assets), artifact.status)
}
for (const artifact of inventory.artifacts) {
  if (!artifact.path.startsWith('/')) throw new Error(`Invalid artifact URL: ${artifact.path}`)
  const path = artifact.path.slice(1)
  reserve(path)
  await writeResponse(path, artifact.response, 200)
}
const finalDir = join(outputParent, 'public')
const backupDir = join(output, 'previous-public')
let previous = false
try {
  await rename(finalDir, backupDir)
  previous = true
} catch (error) {
  if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error
}
try {
  await rename(publicDir, finalDir)
} catch (error) {
  if (previous) await rename(backupDir, finalDir)
  throw error
}
await rm(output, { recursive: true, force: true })
process.stdout.write(`Static output: ${finalDir}\n`)
