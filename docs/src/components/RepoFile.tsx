interface RepoFileProps {
  path: string
  label?: string
}

export function RepoFile({ path, label = path }: RepoFileProps) {
  const repoPath = path.replace(/^\/+/, '')
  const href = `https://github.com/jackin-project/jackin/blob/main/${encodeURI(repoPath)}`

  return (
    <a href={href} target="_blank" rel="noopener noreferrer">
      <code>{label}</code>
    </a>
  )
}
