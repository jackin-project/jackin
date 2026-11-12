import { EarlyDevelopmentNotice } from './EarlyDevelopmentNotice';

export function InstallBlock() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">08 · Jack in</div>
        <h2 className="landing-sec-title">Install.</h2>
        <p className="landing-sec-intro">Build from source. Then run an OpenShell gateway, add a role, an account, and a workspace, and load — the quick start walks you through it.</p>

        <div className="landing-install">
          <div className="landing-install-line"><span className="k">brew</span> install openshell</div>
          <div className="landing-install-line"><span className="k">git</span> clone https://github.com/jackin-project/jackin.git && cd jackin</div>
          <div className="landing-install-line"><span className="k">mise</span> install && mise exec -- mbx +1.99.1 build -p jackin --release</div>
        </div>

        <div className="landing-install-warning">
          <EarlyDevelopmentNotice />
        </div>

        <div className="landing-install-ctas">
          <a className="landing-btn-primary" href="/getting-started/why">Read the Docs →</a>
          <a className="landing-btn-ghost" href="https://github.com/jackin-project/jackin" target="_blank" rel="noopener">
            <svg className="landing-star-icon" width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
              <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
            </svg>
            Star on GitHub
          </a>
        </div>
      </div>
    </section>
  );
}
