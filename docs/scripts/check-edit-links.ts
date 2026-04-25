import { existsSync } from 'node:fs'
import { readdir, readFile } from 'node:fs/promises'
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const docsRoot = resolve(__dirname, '..')
const repoRoot = resolve(docsRoot, '..')
const distRoot = resolve(docsRoot, 'dist')

const repoEditPathPrefix = '/jackin-project/jackin/edit/main/'
const editLinkPattern = /href="(https:\/\/github\.com\/jackin-project\/jackin\/edit\/main\/[^"]+)"/g

async function htmlFiles(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { withFileTypes: true })
  const files = await Promise.all(
    entries.map(async (entry) => {
      const path = join(dir, entry.name)
      if (entry.isDirectory()) return htmlFiles(path)
      return entry.isFile() && entry.name.endsWith('.html') ? [path] : []
    }),
  )

  return files.flat()
}

function repoPathForEditUrl(href: string): string | undefined {
  const url = new URL(href)
  if (url.hostname !== 'github.com' || !url.pathname.startsWith(repoEditPathPrefix)) {
    return undefined
  }

  return decodeURIComponent(url.pathname.slice(repoEditPathPrefix.length))
}

function resolveInsideRepo(repoPath: string): string | undefined {
  const path = resolve(repoRoot, repoPath)
  const pathRelativeToRepo = relative(repoRoot, path)

  if (
    pathRelativeToRepo === '' ||
    pathRelativeToRepo.startsWith(`..${sep}`) ||
    pathRelativeToRepo === '..' ||
    isAbsolute(pathRelativeToRepo)
  ) {
    return undefined
  }

  return path
}

const failures: string[] = []
let checked = 0

for (const htmlFile of await htmlFiles(distRoot)) {
  const html = await readFile(htmlFile, 'utf8')
  const page = relative(distRoot, htmlFile)

  for (const match of html.matchAll(editLinkPattern)) {
    const href = match[1]
    if (!href) continue

    const repoPath = repoPathForEditUrl(href)
    if (!repoPath) continue

    checked += 1
    const sourcePath = resolveInsideRepo(repoPath)
    if (!sourcePath) {
      failures.push(`${page}: ${href} escapes the repository root`)
      continue
    }

    if (!existsSync(sourcePath)) {
      failures.push(`${page}: ${href} does not map to an existing file`)
    }
  }
}

if (checked === 0) {
  failures.push('No generated GitHub edit links were found in docs/dist.')
}

if (failures.length > 0) {
  console.error('Invalid generated edit links:')
  for (const failure of failures) {
    console.error(`- ${failure}`)
  }
  process.exit(1)
}

console.log(`Checked ${checked} generated edit links.`)
