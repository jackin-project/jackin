// docs/components/landing/EarlyDevelopmentNotice.tsx
export function EarlyDevelopmentNotice() {
  return (
    <aside className="landing-dev-warning" aria-label="Active early development warning">
      <div className="landing-dev-warning-label">
        <span aria-hidden="true">🚧</span>
        Active early development
      </div>
      <p>
        <strong>jackin❯ is not production-ready.</strong> Major breaking changes are expected while the core concept, runtime integrations, CLI/TUI workflows, schemas, and docs are still being refined. Early adopters are welcome; the priority right now is concept quality and fast iteration, not freezing today's behavior.
      </p>
    </aside>
  );
}
