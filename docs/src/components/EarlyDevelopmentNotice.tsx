export function EarlyDevelopmentNotice() {
  return (
    <aside className="jk-early-development-notice" aria-label="Active early development warning">
      <div className="jk-early-development-notice-label">
        <span aria-hidden="true">🚧</span>
        Active early development
      </div>
      <p>
        <strong>jackin❯ is not production-ready.</strong> Major breaking changes are expected while the core concept,
        runtime integrations, CLI/TUI workflows, schemas, and docs are still being refined. Early adopters are welcome;
        the priority right now is concept quality and fast iteration, not freezing today's behavior.
      </p>
    </aside>
  )
}
