import { describe, expect, test } from 'bun:test'
import {
  docsContentRoute,
  escapeMdxOutsideFences,
  metaCompletenessError,
  normalizeRepoPath,
  renderMdxPage,
  rewriteLinks,
  siblingCrateRoute,
  stripH1,
  transformReadmeBody,
  type CrateReadme,
} from './gen-crate-pages'

const CORE: CrateReadme = {
  name: 'jackin-core',
  dir: 'crates/core/jackin-core',
  body: '',
}

describe('stripH1', () => {
  test('removes leading H1 and following blank line', () => {
    expect(stripH1('# jackin-core\n\nBody here.\n')).toBe('Body here.\n')
  })
})

describe('normalizeRepoPath', () => {
  test('src/x.rs under crate', () => {
    expect(normalizeRepoPath(CORE.dir, 'src/lib.rs')).toBe(
      'crates/core/jackin-core/src/lib.rs',
    )
  })

  test('src/dir relative path', () => {
    expect(normalizeRepoPath(CORE.dir, 'src/agent')).toBe(
      'crates/core/jackin-core/src/agent',
    )
  })

  test('sibling README is not a RepoFile path', () => {
    expect(
      normalizeRepoPath(CORE.dir, '../jackin-protocol/README.md'),
    ).toBeNull()
    expect(
      normalizeRepoPath(CORE.dir, '../../core/jackin-protocol/README.md'),
    ).toBeNull()
  })
})

describe('siblingCrateRoute', () => {
  test('maps sibling README to generated route', () => {
    expect(siblingCrateRoute('../jackin-protocol/README.md')).toBe(
      '/reference/crates/jackin-protocol/',
    )
  })

  test('maps cross-group sibling README to generated route', () => {
    expect(siblingCrateRoute('../../ui/jackin-tui/README.md')).toBe(
      '/reference/crates/jackin-tui/',
    )
  })
})

describe('docsContentRoute', () => {
  test('maps docs content path to site route', () => {
    expect(
      docsContentRoute('../../../docs/content/reference/capsule/index.mdx'),
    ).toBe('/reference/capsule/')
  })
})

describe('rewriteLinks', () => {
  test('rewrites three link shapes', () => {
    const input = [
      '[`lib.rs`](src/lib.rs)',
      '[agent](src/agent)',
      '[protocol](../jackin-protocol/README.md)',
    ].join('\n')
    const out = rewriteLinks(CORE.dir, input)
    expect(out).toContain(
      '<RepoFile path="crates/core/jackin-core/src/lib.rs">`lib.rs`</RepoFile>',
    )
    // Directory links resolve to the paired .rs module file.
    expect(out).toContain(
      '<RepoFile path="crates/core/jackin-core/src/agent.rs">agent</RepoFile>',
    )
    expect(out).toContain('[protocol](/reference/crates/jackin-protocol/)')
  })
})

describe('escapeMdxOutsideFences', () => {
  test('escapes { and < in prose', () => {
    expect(escapeMdxOutsideFences('use {foo} and <bar>')).toBe(
      'use \\{foo} and &lt;bar>',
    )
  })

  test('leaves fence contents unchanged', () => {
    const md = 'prose {x}\n```\n{keep} <raw>\n```\nafter {y}'
    const out = escapeMdxOutsideFences(md)
    expect(out).toContain('prose \\{x}')
    expect(out).toContain('{keep} <raw>')
    expect(out).toContain('after \\{y}')
  })
})

describe('transformReadmeBody', () => {
  test('strips H1 then rewrites + escapes', () => {
    const body = '# Title\n\nSee [`lib.rs`](src/lib.rs) and {brace}.\n'
    const out = transformReadmeBody(CORE, body)
    expect(out.startsWith('#')).toBe(false)
    expect(out).toContain('<RepoFile path="crates/core/jackin-core/src/lib.rs">')
    expect(out).toContain('\\{brace}')
  })
})

describe('renderMdxPage', () => {
  test('uses the product brand for the root crate display title', () => {
    const root: CrateReadme = {
      name: 'jackin',
      dir: 'crates/apps/jackin',
      body: '',
    }
    expect(renderMdxPage(root, '# jackin\n').split('\n')[1]).toBe(
      'title: "jackin❯"',
    )
  })
})

describe('metaCompletenessError', () => {
  test('null when sets match', () => {
    expect(metaCompletenessError(['a', 'b'], ['a', 'b'])).toBeNull()
  })

  test('names missing and extra entries', () => {
    const err = metaCompletenessError(['a', 'b'], ['a', 'c'])
    expect(err).toContain('missing entries: b')
    expect(err).toContain('extra entries (no README): c')
  })
})
