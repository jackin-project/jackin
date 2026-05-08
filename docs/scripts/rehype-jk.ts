import { visit, SKIP } from 'unist-util-visit'
import type { Element, Root, Text } from 'hast'

// Tag every prose mention of the jackin' project name (and the
// possessive form) with a brand-styled span so the name picks up
// --jk-brand in both light and dark modes. Styling lives in
// src/styles/docs-theme.css under the .jk-name selector.
//
// SmartyPants in the markdown pipeline rewrites the straight ASCII
// apostrophe (U+0027) into the typographic right single quotation
// mark (U+2019) before the rehype stage runs, so the regex has to
// accept both forms. The trailing `s` is optional to cover the
// possessive `jackin's`.
//
// Skip inside <code> and <pre>: command lines and code blocks
// already render in monospace and shouldn't double up. Stay out
// of <style> and <script> for safety.
const NAME_PATTERN = /jackin['’](?:s)?/g
const SKIPPED_TAGS = new Set(['code', 'pre', 'style', 'script'])

export default function rehypeJk() {
  return (tree: Root) => {
    visit(tree, 'text', (node: Text, index, parent) => {
      if (index === undefined || !parent || parent.type !== 'element') return
      const parentEl = parent as Element
      if (SKIPPED_TAGS.has(parentEl.tagName)) return

      const value = node.value
      if (!value) return
      if (!value.includes("jackin'") && !value.includes('jackin’')) return

      const parts: Array<Text | Element> = []
      let last = 0
      NAME_PATTERN.lastIndex = 0
      let match: RegExpExecArray | null
      while ((match = NAME_PATTERN.exec(value)) !== null) {
        if (match.index > last) {
          parts.push({ type: 'text', value: value.slice(last, match.index) })
        }
        parts.push({
          type: 'element',
          tagName: 'span',
          properties: { className: ['jk-name'] },
          children: [{ type: 'text', value: match[0] }],
        })
        last = match.index + match[0].length
      }
      if (parts.length === 0) return
      if (last < value.length) {
        parts.push({ type: 'text', value: value.slice(last) })
      }

      parentEl.children.splice(index, 1, ...parts)
      return [SKIP, index + parts.length]
    })
  }
}
