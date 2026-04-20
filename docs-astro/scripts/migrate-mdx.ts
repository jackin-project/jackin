// One-shot codemod: transforms Vocs MDX directives to Starlight components.
// Delete this file (and its test) after the migration commits in Task 11.

export interface TransformResult {
  content: string
  imports: string[]
}

export function transformMdx(input: string): TransformResult {
  const usedImports = new Set<string>()
  let body = input

  // Split frontmatter from body so transformations don't touch frontmatter
  let frontmatter = ''
  const fmMatch = body.match(/^---\n([\s\S]*?)\n---\n?/)
  if (fmMatch) {
    frontmatter = fmMatch[0]
    body = body.slice(fmMatch[0].length)
  } else {
    // No frontmatter — try to extract title from first H1
    const h1 = body.match(/^#\s+(.+?)\s*$/m)
    if (h1) {
      frontmatter = `---\ntitle: ${h1[1]}\n---\n\n`
    }
  }

  // Transform ::::steps ... :::: → <Steps>...</Steps>
  body = body.replace(/^::::steps\s*\n([\s\S]*?)^::::\s*$/gm, (_m, inner) => {
    usedImports.add('Steps')
    return `<Steps>\n${inner.trim()}\n</Steps>`
  })

  // Transform :::code-group ... ::: → <Tabs>...</Tabs>
  body = body.replace(/^:::code-group\s*\n([\s\S]*?)^:::\s*$/gm, (_m, inner) => {
    usedImports.add('Tabs')
    usedImports.add('TabItem')
    // Inside, find ```lang [Label]\n...\n``` blocks and wrap each in TabItem
    const tabItems = inner.replace(
      /```(\w+)\s+\[([^\]]+)\]\n([\s\S]*?)```/g,
      (_mm: string, _lang: string, label: string, code: string) => {
        return `<TabItem label="${label}">\n\n\`\`\`${_lang}\n${code}\`\`\`\n\n</TabItem>`
      }
    )
    return `<Tabs>\n${tabItems.trim()}\n</Tabs>`
  })

  // Transform :::note / :::tip / :::warning blocks → <Aside>
  body = body.replace(
    /^:::(note|tip|warning)\s*\n([\s\S]*?)^:::\s*$/gm,
    (_m, kind, inner) => {
      usedImports.add('Aside')
      const type = kind === 'warning' ? 'caution' : kind
      return `<Aside type="${type}">\n${inner.trim()}\n</Aside>`
    }
  )

  // Assemble output
  const imports = Array.from(usedImports).sort()
  const importLine =
    imports.length > 0
      ? `import { ${imports.join(', ')} } from '@astrojs/starlight/components'\n\n`
      : ''

  return {
    content: frontmatter + importLine + body,
    imports,
  }
}

import { readdirSync, readFileSync, writeFileSync, mkdirSync, statSync } from 'node:fs'
import { join, dirname, relative } from 'node:path'

function walkMdx(root: string): string[] {
  const out: string[] = []
  for (const name of readdirSync(root)) {
    const full = join(root, name)
    const st = statSync(full)
    if (st.isDirectory()) out.push(...walkMdx(full))
    else if (name.endsWith('.mdx') || name.endsWith('.md')) out.push(full)
  }
  return out
}

function main() {
  const args = process.argv.slice(2)
  const dry = args.includes('--dry')
  const srcRoot = '../docs/pages'
  const dstRoot = './src/content/docs'

  const files = walkMdx(srcRoot)
    .filter((f) => !f.endsWith('/index.mdx')) // landing skipped — handled in Phase 3
    .sort()

  let changed = 0
  for (const src of files) {
    const rel = relative(srcRoot, src)
    const dst = join(dstRoot, rel)
    const input = readFileSync(src, 'utf8')
    const { content } = transformMdx(input)

    if (dry) {
      console.log(`\n=== ${rel} ===`)
      if (input === content) console.log('(unchanged)')
      else console.log(content.slice(0, 400) + (content.length > 400 ? '\n...' : ''))
      continue
    }

    mkdirSync(dirname(dst), { recursive: true })
    writeFileSync(dst, content)
    changed++
    console.log(`wrote ${dst}`)
  }

  if (!dry) console.log(`\n${changed} files written to ${dstRoot}`)
}

if (import.meta.main) main()
