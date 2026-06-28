import { readdir, readFile } from 'node:fs/promises'
import { basename, dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// Every MDX file under content/docs/roadmap/ (excluding index.mdx and
// files inside parenthesized group subdirectories) must be referenced in at least
// one meta.json under the same directory tree, so it appears in the docs sidebar.
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

// Collect all roadmap-root page slugs referenced across all meta.json files.
// Page references are relative to the meta.json's directory; we resolve them
// and keep only those that land directly in roadmapDir (not in subdirectories).
async function collectSidebarSlugs(metaFiles: string[]): Promise<Set<string>> {
  const slugs = new Set<string>()
  for (const metaFile of metaFiles) {
    const metaDir = dirname(metaFile)
    const raw = await readFile(metaFile, 'utf8')
    const meta = JSON.parse(raw) as MetaJson
    for (const page of meta.pages ?? []) {
      if (!page.startsWith('.')) continue // skip group dirs and 'index'
      const absolute = resolve(metaDir, page)
      const rel = relative(roadmapDir, absolute)
      // Only include pages that sit directly in roadmapDir (no path separator = no subdir)
      if (!rel.includes('/') && !rel.includes('\\') && !rel.startsWith('..')) {
        slugs.add(rel)
      }
    }
  }
  return slugs
}

// Collect all MDX file slugs in the roadmap root (exclude index, exclude subdirs).
async function collectRoadmapSlugs(): Promise<Set<string>> {
  const entries = await readdir(roadmapDir, { withFileTypes: true })
  const slugs = new Set<string>()
  for (const entry of entries) {
    if (entry.isFile() && entry.name.endsWith('.mdx') && entry.name !== 'index.mdx') {
      slugs.add(basename(entry.name, '.mdx'))
    }
  }
  return slugs
}

const failures: string[] = []

const metaFiles = await collectMetaJsonFiles(roadmapDir)
const [sidebarSlugs, roadmapSlugs] = await Promise.all([
  collectSidebarSlugs(metaFiles),
  collectRoadmapSlugs(),
])

for (const slug of [...roadmapSlugs].sort()) {
  if (!sidebarSlugs.has(slug)) {
    failures.push(`MDX file not in any sidebar meta.json: content/docs/roadmap/${slug}.mdx`)
  }
}

for (const slug of [...sidebarSlugs].sort()) {
  if (!roadmapSlugs.has(slug)) {
    failures.push(
      `meta.json references non-existent MDX file: content/docs/roadmap/${slug}.mdx`,
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
  `Checked ${roadmapSlugs.size} roadmap pages against ${metaFiles.length} meta.json files. Sidebar is complete.`,
)
