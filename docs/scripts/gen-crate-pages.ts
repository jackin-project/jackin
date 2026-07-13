/**
 * Build-time pipeline: render each crate README as a Fumadocs page under
 * content/docs/reference/crates/. The README is the source of truth; generated
 * pages are gitignored.
 */
import { existsSync, statSync } from 'node:fs'
import { mkdir, readdir, readFile, writeFile } from 'node:fs/promises'
import { join, normalize, relative, sep } from 'node:path'

const docsRoot = join(import.meta.dirname, '..')
const repoRoot = join(docsRoot, '..')
const cratesRoot = join(repoRoot, 'crates')
const outDir = join(docsRoot, 'content', 'docs', 'reference', 'crates')
const metaPath = join(outDir, 'meta.json')

export type CrateReadme = { name: string; body: string }

/** List workspace crates that carry a README.md (sorted). */
export async function listCrateReadmes(): Promise<CrateReadme[]> {
  const entries = await readdir(cratesRoot, { withFileTypes: true })
  const crates: CrateReadme[] = []
  for (const entry of entries) {
    if (!entry.isDirectory()) continue
    const readmePath = join(cratesRoot, entry.name, 'README.md')
    try {
      const body = await readFile(readmePath, 'utf8')
      crates.push({ name: entry.name, body })
    } catch {
      // no README — skip (agents gate enforces presence separately)
    }
  }
  crates.sort((a, b) => a.name.localeCompare(b.name))
  return crates
}

/** Strip the leading H1 (frontmatter title replaces it). */
export function stripH1(markdown: string): string {
  const lines = markdown.split('\n')
  if (lines[0]?.startsWith('# ')) {
    lines.shift()
    if (lines[0] === '') lines.shift()
  }
  return lines.join('\n')
}

/**
 * Normalize a relative link target from a crate README into a repo path under
 * crates/<name>/, or null if it should not become a RepoFile.
 */
export function normalizeRepoPath(
  crateName: string,
  href: string,
): string | null {
  if (!href || href.startsWith('#') || href.startsWith('http://') || href.startsWith('https://') || href.startsWith('mailto:')) {
    return null
  }
  // Sibling crate README → handled separately as a site route.
  const sibling = href.match(/^\.\.\/([^/]+)\/README\.md(?:#.*)?$/)
  if (sibling) return null

  // Docs content path → site route, not RepoFile.
  if (href.includes('docs/content/docs/') || href.startsWith('../../docs/')) {
    return null
  }

  // Strip optional anchors for path resolution.
  const bare = href.split('#')[0] ?? href
  if (!bare) return null

  const crateRoot = join('crates', crateName)
  let joined: string
  if (bare.startsWith('src/') || bare.startsWith('./src/')) {
    joined = join(crateRoot, bare.replace(/^\.\//, ''))
  } else if (bare.startsWith('../')) {
    // Resolve relative to crates/<name>/README.md location.
    joined = normalize(join(crateRoot, bare))
  } else if (bare.startsWith('/')) {
    return null
  } else {
    joined = join(crateRoot, bare)
  }
  // Reject escapes outside the repo crates tree (and allow other repo roots
  // that resolve via ../ outside crates/<name>).
  const norm = joined.split(sep).join('/')
  if (norm.includes('..')) return null
  return norm
}

/**
 * Repo-links only accepts files. Map directory targets to the paired `*.rs`
 * module file when present; return null when no existing file can be linked.
 */
export function resolveExistingFile(
  repoPath: string,
  root: string = repoRoot,
): string | null {
  const abs = join(root, repoPath)
  if (!existsSync(abs)) return null
  const st = statSync(abs)
  if (st.isFile()) return repoPath
  if (st.isDirectory()) {
    const rsRel = `${repoPath}.rs`
    const rsAbs = join(root, rsRel)
    if (existsSync(rsAbs) && statSync(rsAbs).isFile()) return rsRel
  }
  return null
}

/** Sibling crate README → /reference/crates/<other>/. */
export function siblingCrateRoute(href: string): string | null {
  const m = href.match(/^\.\.\/([^/]+)\/README\.md(?:#.*)?$/)
  if (!m) return null
  return `/reference/crates/${m[1]}/`
}

/** Docs MDX relative path → site-absolute route. */
export function docsContentRoute(href: string): string | null {
  const m = href.match(/(?:^|\/)docs\/content\/docs\/(.+?)(?:\.mdx)?(?:#.*)?$/)
  if (!m) return null
  let slug = m[1]
    .split('/')
    .filter((part) => !(part.startsWith('(') && part.endsWith(')')))
    .join('/')
  if (slug.endsWith('/index')) slug = slug.slice(0, -'/index'.length)
  return `/${slug}/`.replace(/\/+/g, '/')
}

/**
 * Escape MDX-hostile characters outside fenced code blocks.
 * `{` → `\{`, raw `<` that is not a known component start → `&lt;`.
 */
export function escapeMdxOutsideFences(markdown: string): string {
  const lines = markdown.split('\n')
  let inFence = false
  const out: string[] = []
  for (const line of lines) {
    if (/^\s*```/.test(line)) {
      inFence = !inFence
      out.push(line)
      continue
    }
    if (inFence) {
      out.push(line)
      continue
    }
    out.push(escapeMdxProseLine(line))
  }
  return out.join('\n')
}

function escapeMdxProseLine(line: string): string {
  // Leave JSX-like RepoFile tags alone once we've rewritten links.
  let result = ''
  let i = 0
  while (i < line.length) {
    const ch = line[i]!
    if (ch === '{') {
      result += '\\{'
      i += 1
      continue
    }
    if (ch === '<') {
      // Preserve known MDX components and HTML that we emit.
      const rest = line.slice(i)
      if (
        rest.startsWith('<RepoFile') ||
        rest.startsWith('</RepoFile') ||
        rest.startsWith('<Aside') ||
        rest.startsWith('</Aside') ||
        rest.startsWith('<Callout') ||
        rest.startsWith('</Callout')
      ) {
        const close = line.indexOf('>', i)
        if (close !== -1) {
          result += line.slice(i, close + 1)
          i = close + 1
          continue
        }
      }
      result += '&lt;'
      i += 1
      continue
    }
    result += ch
    i += 1
  }
  return result
}

const MD_LINK = /\[([^\]]*)\]\(([^)\s]+)(?:\s+"[^"]*")?\)/g
// Bare code spans that are existing repo-relative paths (repo-links gate),
// including root files like ENGINEERING.md.
const BARE_REPO_PATH =
  /`((?:crates|docs|src|docker|scripts|plans)\/[^`\s]+|[A-Z][A-Z0-9_.-]*\.md)`/g

/** Rewrite markdown links according to plan 049 rules. */
export function rewriteLinks(crateName: string, markdown: string): string {
  return markdown.replace(MD_LINK, (full, text: string, href: string) => {
    if (href.startsWith('http://') || href.startsWith('https://') || href.startsWith('mailto:') || href.startsWith('#')) {
      return full
    }
    const sibling = siblingCrateRoute(href)
    if (sibling) return `[${text}](${sibling})`

    const docsRoute = docsContentRoute(href)
    if (docsRoute) return `[${text}](${docsRoute})`

    const repoPath = normalizeRepoPath(crateName, href)
    if (repoPath) {
      const filePath = resolveExistingFile(repoPath)
      if (filePath) {
        const label = text.trim() || filePath
        return `<RepoFile path="${filePath}">${label}</RepoFile>`
      }
      // Directory without a paired file, or missing path: keep prose, drop link.
      const label = text.trim() || repoPath
      return `\`${label.replace(/`/g, '')}\``
    }
    return full
  })
}

/** Turn bare repo-path code spans into RepoFile when the file exists. */
export function rewriteBareRepoPaths(markdown: string): string {
  return markdown.replace(BARE_REPO_PATH, (full, path: string) => {
    if (path.includes('*/') || path.includes('<') || path.includes('>')) return full
    const filePath = resolveExistingFile(path)
    if (!filePath) return full
    return `<RepoFile path="${filePath}">${path}</RepoFile>`
  })
}

/** Full body transform: strip H1 → rewrite links → bare paths → escape MDX. */
export function transformReadmeBody(crateName: string, body: string): string {
  const stripped = stripH1(body)
  const linked = rewriteLinks(crateName, stripped)
  const bare = rewriteBareRepoPaths(linked)
  return escapeMdxOutsideFences(bare)
}

export function renderMdxPage(crateName: string, body: string): string {
  const transformed = transformReadmeBody(crateName, body)
  return [
    '---',
    `title: "${crateName}"`,
    '---',
    '',
    `{/* GENERATED from crates/${crateName}/README.md — edit the README, not this file */}`,
    '',
    transformed.trimEnd(),
    '',
  ].join('\n')
}

export type MetaFile = { title: string; pages: string[] }

export function expectedMeta(crateNames: string[]): MetaFile {
  return {
    title: 'Behind jackin❯ — crates',
    pages: [...crateNames].sort((a, b) => a.localeCompare(b)),
  }
}

/**
 * Completeness check: every crate README has a meta.json page entry and vice
 * versa. Returns an error message or null when OK.
 */
export function metaCompletenessError(
  crateNames: string[],
  metaPages: string[],
): string | null {
  const expected = new Set(crateNames)
  const actual = new Set(metaPages)
  const missing: string[] = []
  const extra: string[] = []
  for (const name of expected) {
    if (!actual.has(name)) missing.push(name)
  }
  for (const name of actual) {
    if (!expected.has(name)) extra.push(name)
  }
  if (missing.length === 0 && extra.length === 0) return null
  const parts: string[] = []
  if (missing.length) parts.push(`meta.json missing entries: ${missing.join(', ')}`)
  if (extra.length) parts.push(`meta.json extra entries (no README): ${extra.join(', ')}`)
  return parts.join('; ')
}

async function main(): Promise<void> {
  const crates = await listCrateReadmes()
  if (crates.length === 0) {
    console.error('gen-crate-pages: no crates/*/README.md found')
    process.exit(1)
  }

  const names = crates.map((c) => c.name)
  let meta: MetaFile
  try {
    meta = JSON.parse(await readFile(metaPath, 'utf8')) as MetaFile
  } catch {
    console.error(`gen-crate-pages: missing checked-in meta.json at ${relative(repoRoot, metaPath)}`)
    process.exit(1)
  }

  const completeness = metaCompletenessError(names, meta.pages)
  if (completeness) {
    console.error(`gen-crate-pages: ${completeness}`)
    console.error(`expected pages (alphabetical): ${names.join(', ')}`)
    process.exit(1)
  }

  await mkdir(outDir, { recursive: true })
  for (const crate of crates) {
    const mdx = renderMdxPage(crate.name, crate.body)
    const outPath = join(outDir, `${crate.name}.mdx`)
    await writeFile(outPath, mdx, 'utf8')
  }

  // Emit page list for Step 2 verification / humans.
  console.log(
    JSON.stringify(
      { generated: names.length, pages: names, outDir: relative(repoRoot, outDir) },
      null,
      2,
    ),
  )
}

if (import.meta.main) {
  main().catch((err) => {
    console.error(err)
    process.exit(1)
  })
}
