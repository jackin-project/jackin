import { Callout } from 'fumadocs-ui/components/callout'

export function EarlyDevelopmentNotice() {
  return (
    <Callout className="jk-early-development-notice" type="warn" title="Active early development">
      <p>
        <strong>jackin' is not production-ready.</strong> It is intentionally in active early development while the
        core concept, runtime integrations, CLI/TUI workflows, schemas, and documentation are still being refined.
        Major breaking changes are expected before a stable release; features may be redesigned, replaced, or removed
        when that leads to a better project. Early adopters are welcome, but the priority right now is concept quality
        and fast iteration rather than freezing today's behavior.
      </p>
    </Callout>
  )
}
