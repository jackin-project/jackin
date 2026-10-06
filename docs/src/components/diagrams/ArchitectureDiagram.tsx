export function ArchitectureDiagram() {
  return (
    <figure
      className="jk-arch not-content"
      aria-label="jackin❯ runtime architecture: host machine running Docker Engine, with a per-agent network containing an Agent container linked to a DinD sidecar, plus three host-side persisted paths"
    >
      <div className="jk-arch-panel">
        <header className="jk-arch-header">
          <span className="jk-arch-dot" aria-hidden="true" />
          <span className="jk-arch-dot" aria-hidden="true" />
          <span className="jk-arch-dot" aria-hidden="true" />
          <span className="jk-arch-title">runtime topology</span>
        </header>

        <div className="jk-arch-body">
          <section className="jk-arch-section">
            <h3 className="jk-arch-section-label">Host</h3>
            <div className="jk-arch-row">
              <span className="jk-arch-chip jk-arch-chip--accent">jackin CLI</span>
              <span className="jk-arch-note">operator interface</span>
            </div>
            <div className="jk-arch-sub">
              <div className="jk-arch-sub-label">Docker Engine</div>
              <div className="jk-arch-sub">
                <div className="jk-arch-sub-label jk-arch-sub-label--mono">
                  jackin-agent-smith-net
                  <span className="jk-arch-note">per-agent network</span>
                </div>
                <div className="jk-arch-pair">
                  <article className="jk-arch-service">
                    <h4 className="jk-arch-service-name">Agent Container</h4>
                    <p className="jk-arch-service-role">Claude Code · mounted dirs</p>
                  </article>
                  <div className="jk-arch-link" aria-hidden="true">
                    <svg width="64" height="22" viewBox="0 0 64 22" fill="none">
                      <path d="M0 11 H56" stroke="currentColor" strokeWidth="1.5" />
                      <path
                        d="M50 6 L56 11 L50 16"
                        stroke="currentColor"
                        strokeWidth="1.5"
                        fill="none"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                      />
                    </svg>
                    <span className="jk-arch-link-label">tcp://dind:2376</span>
                  </div>
                  <article className="jk-arch-service">
                    <h4 className="jk-arch-service-name">DinD Sidecar</h4>
                    <p className="jk-arch-service-role">docker:dind · TLS certs volume</p>
                  </article>
                </div>
              </div>
            </div>
          </section>

          <hr className="jk-arch-rule" />

          <section className="jk-arch-section">
            <h3 className="jk-arch-section-label">Persisted on host</h3>
            <div className="jk-arch-mounts-grid">
              <div className="jk-arch-mount">
                <code>~/.jackin/data/</code>
                <span>runtime state per instance</span>
              </div>
              <div className="jk-arch-mount">
                <code>~/.jackin/agents/</code>
                <span>cached agent repos</span>
              </div>
              <div className="jk-arch-mount">
                <code>~/.config/jackin/</code>
                <span>operator config</span>
              </div>
            </div>
          </section>
        </div>
      </div>
    </figure>
  )
}
