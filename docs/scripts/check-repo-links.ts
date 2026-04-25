import { statSync } from 'node:fs'
import { readdir, readFile } from 'node:fs/promises'
import { dirname, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

// Lychee checks links after the docs are rendered, but it cannot see plain
// MDX code spans like `src/runtime/launch.rs` because those are just text.
// This script makes repo-file references explicit by requiring <RepoFile />.
// Then Astro renders a real GitHub link and lychee can verify it.

const __dirname = dirname(fileURLToPath(import.meta.url))
const docsRoot = resolve(__dirname, '..')
const repoRoot = resolve(docsRoot, '..')
const contentRoot = resolve(docsRoot, 'src', 'content', 'docs')

const repoPathPrefixes = ['src/', 'docs/', 'docker/', '.github/']
const repoTopLevelFiles = new Set([
  'Cargo.lock',
  'Cargo.toml',
  'Justfile',
  'build.rs',
  'docker-bake.hcl',
  'mise.toml',
  'release.toml',
  'renovate.json',
])
const internalBlobUrl = /https:\/\/github\.com\/jackin-project\/jackin\/blob\/main\/([^\s)>"']+)/g
const internalTreeUrl = /https:\/\/github\.com\/jackin-project\/jackin\/tree\/main\/[^\s)>"']+/g
const inlineCode = /`([^`\n]+)`/g
const repoFileComponent = /<RepoFile\s+[^>]*path=(?:"([^"]+)"|'([^']+)')/g

async function mdxFiles(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { withFileTypes: true })
  const files = await Promise.all(
    entries.map(async (entry) => {
      const path = join(dir, entry.name)
      if (entry.isDirectory()) return mdxFiles(path)
      return entry.isFile() && entry.name.endsWith('.mdx') ? [path] : []
    }),
  )

  return files.flat()
}

function existingRepoFile(path: string): boolean {
  const normalized = path.replace(/^\/+/, '')
  const absolute = resolve(repoRoot, normalized)
  const relativePath = relative(repoRoot, absolute)
  if (relativePath === '..' || relativePath.startsWith(`..${sep}`)) return false

  try {
    return statSync(absolute).isFile()
  } catch {
    return false
  }
}

function repoPathCandidate(value: string): string | undefined {
  const path = value.trim()
  if (/[\s,*]/.test(path)) return undefined
  if (repoPathPrefixes.some((prefix) => path.startsWith(prefix))) return path
  if (repoTopLevelFiles.has(path)) return path
  return undefined
}

function isMarkdownLinkText(line: string, matchStart: number, matchLength: number): boolean {
  const before = line[matchStart - 1]
  const after = line.slice(matchStart + matchLength, matchStart + matchLength + 2)
  return before === '[' && after === ']('
}

const failures: string[] = []

for (const file of await mdxFiles(contentRoot)) {
  const lines = (await readFile(file, 'utf8')).split('\n')
  const displayPath = relative(repoRoot, file)
  let inFence = false

  lines.forEach((line, index) => {
    if (/^\s*(```|~~~)/.test(line)) {
      inFence = !inFence
      return
    }

    if (inFence) return

    for (const match of line.matchAll(repoFileComponent)) {
      const path = match[1] ?? match[2]
      if (!path) continue

      if (!existingRepoFile(path)) {
        failures.push(`${displayPath}:${index + 1}: RepoFile path does not exist in the repository: ${path}`)
      }
    }

    for (const match of line.matchAll(internalBlobUrl)) {
      const path = match[1]
      if (!path) continue

      failures.push(
        `${displayPath}:${index + 1}: use <RepoFile path="${path}" /> instead of a full GitHub blob URL`,
      )
    }

    for (const match of line.matchAll(internalTreeUrl)) {
      failures.push(
        `${displayPath}:${index + 1}: use a blob/main file link instead of tree/main so CI can verify it: ${match[0]}`,
      )
    }

    for (const match of line.matchAll(inlineCode)) {
      const fullMatch = match[0]
      const value = match[1]
      const matchStart = match.index
      if (value === undefined || matchStart === undefined) continue
      if (isMarkdownLinkText(line, matchStart, fullMatch.length)) continue

      const path = repoPathCandidate(value)
      if (!path || !existingRepoFile(path)) continue

      failures.push(`${displayPath}:${index + 1}: link existing repo file \`${path}\` with <RepoFile path="${path}" />`)
    }
  })
}

if (failures.length > 0) {
  console.error('Repository file references must be verifiable links:')
  for (const failure of failures) {
    console.error(`- ${failure}`)
  }
  process.exit(1)
}

console.log('Checked repository file references.')
