export function ArchitectureDiagram() {
  return (
    <figure
      className="jk-arch not-content"
      aria-label="jackin❯ runtime architecture: on the host, the jackin CLI builds the role image with Docker BuildKit and delivers it to an OpenShell gateway that the operator runs. The gateway runs one sandbox with Codex and keeps credentials as OpenShell providers, so the sandbox sees only placeholders. The project goes in as a working tree and a git bundle, and the work comes back as a bundle."
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
              <span className="jk-arch-note">Jackin Console · Jackin Capsule</span>
            </div>
            <div className="jk-arch-sub">
              <div className="jk-arch-sub-label">
                Docker BuildKit
                <span className="jk-arch-note">builds the role image and delivers it to the gateway, never runs the agent</span>
              </div>
            </div>
          </section>

          <hr className="jk-arch-rule" />

          <section className="jk-arch-section">
            <h3 className="jk-arch-section-label">OpenShell gateway · run by the operator</h3>
            <div className="jk-arch-pair">
              <article className="jk-arch-service">
                <h4 className="jk-arch-service-name">Sandbox</h4>
                <p className="jk-arch-service-role">Codex · native OpenShell policy · /sandbox/project</p>
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
                <span className="jk-arch-link-label">placeholder</span>
              </div>
              <article className="jk-arch-service">
                <h4 className="jk-arch-service-name">OpenShell provider</h4>
                <p className="jk-arch-service-role">the real credential stays on the gateway</p>
              </article>
            </div>
          </section>

          <hr className="jk-arch-rule" />

          <section className="jk-arch-section">
            <h3 className="jk-arch-section-label">Between host and sandbox</h3>
            <div className="jk-arch-mounts-grid">
              <div className="jk-arch-mount">
                <code>working tree + git bundle</code>
                <span>uploaded in, never a bind mount</span>
              </div>
              <div className="jk-arch-mount">
                <code>refs/heads/jackin/&lt;sandbox&gt;/&lt;branch&gt;</code>
                <span>work exported back as a bundle</span>
              </div>
              <div className="jk-arch-mount">
                <code>Docker capability</code>
                <span>off by default, opt-in per role or workspace</span>
              </div>
            </div>
          </section>
        </div>
      </div>
    </figure>
  )
}
