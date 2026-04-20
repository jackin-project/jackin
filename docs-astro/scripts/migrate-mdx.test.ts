import { describe, test, expect } from 'bun:test'
import { transformMdx } from './migrate-mdx'

describe('transformMdx', () => {
  test('replaces :::note block with Aside component', () => {
    const input = [
      'Some text.',
      '',
      ':::note',
      'A note body.',
      ':::',
      '',
      'More text.',
    ].join('\n')

    const { content, imports } = transformMdx(input)

    expect(content).toContain('<Aside type="note">')
    expect(content).toContain('A note body.')
    expect(content).toContain('</Aside>')
    expect(imports).toContain('Aside')
  })

  test('replaces :::tip block with Aside type="tip"', () => {
    const input = ':::tip\nTip body.\n:::'
    const { content, imports } = transformMdx(input)
    expect(content).toContain('<Aside type="tip">')
    expect(content).toContain('Tip body.')
    expect(imports).toContain('Aside')
  })

  test('replaces :::warning block with Aside type="caution"', () => {
    const input = ':::warning\nWarning body.\n:::'
    const { content, imports } = transformMdx(input)
    expect(content).toContain('<Aside type="caution">')
    expect(imports).toContain('Aside')
  })

  test('replaces ::::steps with Steps wrapper', () => {
    const input = [
      '::::steps',
      '',
      '### Step one',
      'Do the first thing.',
      '',
      '### Step two',
      'Do the second thing.',
      '',
      '::::',
    ].join('\n')

    const { content, imports } = transformMdx(input)
    expect(content).toContain('<Steps>')
    expect(content).toContain('</Steps>')
    expect(content).toContain('### Step one')
    expect(imports).toContain('Steps')
  })

  test('replaces :::code-group with Tabs/TabItem wrapper', () => {
    const input = [
      ':::code-group',
      '',
      '```sh [macOS]',
      'brew install jackin',
      '```',
      '',
      '```sh [Linux]',
      'brew install jackin',
      '```',
      '',
      ':::',
    ].join('\n')

    const { content, imports } = transformMdx(input)
    expect(content).toContain('<Tabs>')
    expect(content).toContain('<TabItem label="macOS">')
    expect(content).toContain('<TabItem label="Linux">')
    expect(content).toContain('</Tabs>')
    expect(imports).toContain('Tabs')
    expect(imports).toContain('TabItem')
  })

  test('returns imports as a sorted unique set', () => {
    const input = ':::note\nA.\n:::\n\n:::tip\nB.\n:::'
    const { imports } = transformMdx(input)
    expect(imports).toEqual(['Aside'])
  })

  test('emits no imports when no directives are used', () => {
    const input = '# Heading\n\nJust plain markdown.'
    const { imports } = transformMdx(input)
    expect(imports).toEqual([])
  })

  test('preserves existing frontmatter', () => {
    const input = '---\ntitle: My Page\n---\n\nBody.'
    const { content } = transformMdx(input)
    expect(content).toMatch(/^---\ntitle: My Page\n---/)
  })

  test('extracts title from first H1 when frontmatter missing', () => {
    const input = '# Extracted Title\n\nBody.'
    const { content } = transformMdx(input)
    expect(content).toMatch(/^---\ntitle: Extracted Title\n---/)
  })

  test('injects import line after frontmatter', () => {
    const input = '---\ntitle: Page\n---\n\n:::note\nA.\n:::'
    const { content } = transformMdx(input)
    expect(content).toContain("import { Aside } from '@astrojs/starlight/components'")
    // Import line must be after closing --- and before body
    const frontmatterEnd = content.indexOf('---', 4) + 3
    const importIdx = content.indexOf('import')
    const asideIdx = content.indexOf('<Aside')
    expect(importIdx).toBeGreaterThan(frontmatterEnd)
    expect(importIdx).toBeLessThan(asideIdx)
  })
})
