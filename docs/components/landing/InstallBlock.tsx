// docs/components/landing/InstallBlock.tsx
export function InstallBlock() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">08 · Jack in</div>
        <h2 className="landing-sec-title">Install.</h2>
        <p className="landing-sec-intro">Homebrew on Mac and Linux. Tap, install, load — you're in.</p>

        <div className="landing-install">
          <div className="landing-install-line"><span className="k">brew</span> tap jackin-project/tap</div>
          <div className="landing-install-line"><span className="k">brew</span> install jackin</div>
          <div className="landing-install-line"><span className="k">jackin</span> load agent-smith</div>
        </div>

        <div className="landing-install-ctas">
          <a className="landing-btn-primary" href="https://jackin.tailrocks.com/" target="_blank" rel="noopener">Read the Docs →</a>
          <a className="landing-btn-ghost" href="https://github.com/jackin-project/jackin" target="_blank" rel="noopener">★ Star on GitHub</a>
        </div>
      </div>
    </section>
  );
}
