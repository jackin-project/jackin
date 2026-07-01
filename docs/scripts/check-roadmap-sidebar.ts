import { readdir, readFile } from 'node:fs/promises'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// Every MDX file under content/docs/roadmap/ (excluding index.mdx) must be
// referenced in at least one meta.json under the same directory tree, so it
// appears in the docs sidebar.
// This script enforces that invariant both ways: no MDX without a meta.json entry,
// and no meta.json entry without a matching MDX file.

const __dirname = dirname(fileURLToPath(import.meta.url))
const roadmapDir = resolve(__dirname, '..', 'content', 'docs', 'roadmap')

interface MetaJson {
  pages?: string[]
}

async function collectMetaJsonFiles(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { withFileTypes: true })
  const results: string[] = []
  for (const entry of entries) {
    if (entry.isDirectory()) {
      results.push(...(await collectMetaJsonFiles(join(dir, entry.name))))
    } else if (entry.name === 'meta.json') {
      results.push(join(dir, entry.name))
    }
  }
  return results
}

// Collect all roadmap page paths referenced across all meta.json files. Page
// references are relative to the meta.json's directory; we resolve them and keep
// references that land under roadmapDir.
async function collectSidebarPages(metaFiles: string[]): Promise<Set<string>> {
  const pages = new Set<string>()
  for (const metaFile of metaFiles) {
    const metaDir = dirname(metaFile)
    const raw = await readFile(metaFile, 'utf8')
    const meta = JSON.parse(raw) as MetaJson
    for (const page of meta.pages ?? []) {
      if (page === 'index' || page.startsWith('(')) continue
      const absolute = resolve(metaDir, page)
      const rel = relative(roadmapDir, absolute)
      if (!rel.startsWith('..')) {
        pages.add(`${rel}.mdx`)
      }
    }
  }
  return pages
}

async function collectRoadmapPages(dir = roadmapDir): Promise<Set<string>> {
  const entries = await readdir(dir, { withFileTypes: true })
  const pages = new Set<string>()
  for (const entry of entries) {
    const absolute = join(dir, entry.name)
    if (entry.isDirectory()) {
      for (const page of await collectRoadmapPages(absolute)) {
        pages.add(page)
      }
    } else if (entry.name.endsWith('.mdx') && absolute !== join(roadmapDir, 'index.mdx')) {
      pages.add(relative(roadmapDir, absolute))
    }
  }
  return pages
}

const failures: string[] = []

const metaFiles = await collectMetaJsonFiles(roadmapDir)
const [sidebarPages, roadmapPages] = await Promise.all([
  collectSidebarPages(metaFiles),
  collectRoadmapPages(),
])

for (const page of [...roadmapPages].sort()) {
  if (!sidebarPages.has(page)) {
    failures.push(`MDX file not in any sidebar meta.json: content/docs/roadmap/${page}`)
  }
}

for (const page of [...sidebarPages].sort()) {
  if (!roadmapPages.has(page)) {
    failures.push(
      `meta.json references non-existent MDX file: content/docs/roadmap/${page}`,
    )
  }
}

if (failures.length > 0) {
  console.error('Roadmap sidebar is incomplete:')
  for (const failure of failures) {
    console.error(`- ${failure}`)
  }
  process.exit(1)
}

console.log(
  `Checked ${roadmapPages.size} roadmap pages against ${metaFiles.length} meta.json files. Sidebar is complete.`,
)
