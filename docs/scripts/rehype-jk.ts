import { visit, SKIP } from 'unist-util-visit'
import type { Element, Root, Text } from 'hast'

// Tag prose mentions of the brand wordmark with a split-colour span.
// Styling lives in src/styles/docs-theme.css under .jk-name.
//
// Skip inside <code> and <pre>: command lines and code blocks
// already render in monospace and shouldn't double up. Stay out
// of <style> and <script> for safety.
const NAME_PATTERN = /jackin❯/g
const SKIPPED_TAGS = new Set(['code', 'pre', 'style', 'script'])

export default function rehypeJk() {
  return (tree: Root) => {
    visit(tree, 'text', (node: Text, index, parent) => {
      if (index === undefined || !parent || parent.type !== 'element') return
      const parentEl = parent as Element
      if (SKIPPED_TAGS.has(parentEl.tagName)) return

      const value = node.value
      if (!value) return
      if (!value.includes('jackin❯')) return

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
          children: [
            { type: 'element', tagName: 'span', properties: { className: ['jk-name__text'] }, children: [{ type: 'text', value: 'jackin' }] },
            { type: 'element', tagName: 'span', properties: { className: ['jk-name__chevron'] }, children: [{ type: 'text', value: '❯' }] },
          ],
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
