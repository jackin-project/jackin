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
