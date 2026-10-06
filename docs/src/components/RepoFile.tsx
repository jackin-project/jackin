import type { ReactNode } from 'react'

interface RepoFileProps {
  path: string
  label?: string
  children?: ReactNode
}

export function RepoFile({ path, label, children }: RepoFileProps) {
  const repoPath = path.replace(/^\/+/, '')
  const href = `https://github.com/jackin-project/jackin/blob/main/${encodeURI(repoPath)}`
  const text = label ?? children ?? path

  return (
    <a href={href} target="_blank" rel="noopener noreferrer">
      <code>{text}</code>
    </a>
  )
}
