




import { existsSync, statSync } from 'node:fs'
import { mkdir, readdir, readFile, writeFile } from 'node:fs/promises'
import { join, normalize, relative, sep } from 'node:path'

const docsRoot = join(import.meta.dirname, '..')
const repoRoot = join(docsRoot, '..')
const cratesRoot = join(repoRoot, 'crates')
const outDir = join(docsRoot, 'content', 'reference', 'crates')
const metaPath = join(outDir, 'meta.json')

export type CrateReadme = { name: string; dir: string; body: string }


export async function listCrateReadmes(): Promise<CrateReadme[]> {
  const groups = await readdir(cratesRoot, { withFileTypes: true })
  const crates: CrateReadme[] = []
  for (const group of groups) {
    if (!group.isDirectory()) continue
    const entries = await readdir(join(cratesRoot, group.name), {
      withFileTypes: true,
    })
    for (const entry of entries) {
      if (!entry.isDirectory()) continue
      const dir = join('crates', group.name, entry.name)
      const readmePath = join(repoRoot, dir, 'README.md')
      try {
        const body = await readFile(readmePath, 'utf8')
        crates.push({ name: entry.name, dir: dir.split(sep).join('/'), body })
      } catch {

      }
    }
  }
  crates.sort((a, b) => a.name.localeCompare(b.name))
  return crates
}


export function stripH1(markdown: string): string {
  const lines = markdown.split('\n')
  if (lines[0]?.startsWith('# ')) {
    lines.shift()
    if (lines[0] === '') lines.shift()
  }
  return lines.join('\n')
}





export function normalizeRepoPath(
  crateDir: string,
  href: string,
): string | null {
  if (!href || href.startsWith('#') || href.startsWith('http://') || href.startsWith('https://') || href.startsWith('mailto:')) {
    return null
  }

  const sibling = href.match(/^(?:\.\.\/)+(?:[^/]+\/)?([^/]+)\/README\.md(?:#.*)?$/)
  if (sibling) return null


  if (
    href.includes('docs/content/') ||
    href.startsWith('../../docs/') ||
    href.startsWith('../../../docs/')
  ) {
    return null
  }


  const bare = href.split('#')[0] ?? href
  if (!bare) return null

  let joined: string
  if (bare.startsWith('src/') || bare.startsWith('./src/')) {
    joined = join(crateDir, bare.replace(/^\.\//, ''))
  } else if (bare.startsWith('../')) {

    joined = normalize(join(crateDir, bare))
  } else if (bare.startsWith('/')) {
    return null
  } else {
    joined = join(crateDir, bare)
  }


  const norm = joined.split(sep).join('/')
  if (norm.includes('..')) return null
  return norm
}





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


export function siblingCrateRoute(href: string): string | null {
  const m = href.match(/^(?:\.\.\/)+(?:[^/]+\/)?([^/]+)\/README\.md(?:#.*)?$/)
  if (!m) return null
  return `/reference/crates/${m[1]}/`
}


export function docsContentRoute(href: string): string | null {
  const m = href.match(/(?:^|\/)docs\/content\/(.+?)(?:\.mdx)?(?:#.*)?$/)
  if (!m) return null
  let slug = m[1]
    .split('/')
    .filter((part) => !(part.startsWith('(') && part.endsWith(')')))
    .join('/')
  if (slug.endsWith('/index')) slug = slug.slice(0, -'/index'.length)
  return `/${slug}/`.replace(/\/+/g, '/')
}





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


const BARE_REPO_PATH =
  /`((?:crates|docs|src|docker|scripts|plans)\/[^`\s]+|[A-Z][A-Z0-9_.-]*\.md)`/g


export function rewriteLinks(crateDir: string, markdown: string): string {
  return markdown.replace(MD_LINK, (full, text: string, href: string) => {
    if (href.startsWith('http://') || href.startsWith('https://') || href.startsWith('mailto:') || href.startsWith('#')) {
      return full
    }
    const sibling = siblingCrateRoute(href)
    if (sibling) return `[${text}](${sibling})`

    const docsRoute = docsContentRoute(href)
    if (docsRoute) return `[${text}](${docsRoute})`

    const repoPath = normalizeRepoPath(crateDir, href)
    if (repoPath) {
      const filePath = resolveExistingFile(repoPath)
      if (filePath) {
        const label = text.trim() || filePath
        return `<RepoFile path="${filePath}">${label}</RepoFile>`
      }

      const label = text.trim() || repoPath
      return `\`${label.replace(/`/g, '')}\``
    }
    return full
  })
}


export function rewriteBareRepoPaths(markdown: string): string {
  return markdown.replace(BARE_REPO_PATH, (full, path: string) => {
    if (path.includes('*/') || path.includes('<') || path.includes('>')) return full
    const filePath = resolveExistingFile(path)
    if (!filePath) return full
    return `<RepoFile path="${filePath}">${path}</RepoFile>`
  })
}


export function transformReadmeBody(crate: CrateReadme, body: string): string {
  const stripped = stripH1(body)
  const linked = rewriteLinks(crate.dir, stripped)
  const bare = rewriteBareRepoPaths(linked)
  return escapeMdxOutsideFences(bare)
}

export function renderMdxPage(crate: CrateReadme, body: string): string {
  const transformed = transformReadmeBody(crate, body)
  const title = crate.name === 'jackin' ? 'jackin❯' : crate.name
  return [
    '---',
    `title: "${title}"`,
    '---',
    '',
    `{/* GENERATED from ${crate.dir}/README.md — edit the README, not this file */}`,
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
    console.error('gen-crate-pages: no crates/*/*/README.md found')
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
    const mdx = renderMdxPage(crate, crate.body)
    const outPath = join(outDir, `${crate.name}.mdx`)
    await writeFile(outPath, mdx, 'utf8')
  }


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
