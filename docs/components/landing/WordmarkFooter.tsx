// docs/components/landing/WordmarkFooter.tsx
export function WordmarkFooter() {
  return (
    <footer className="landing-footer">
      <div className="landing-footer-meta">
        <a href="https://github.com/jackin-project/jackin" target="_blank" rel="noopener">GitHub</a>
        <span className="sep">·</span>
        <a href="/getting-started/why">Docs</a>
        <span className="sep">·</span>
        <span>Apache 2.0</span>
      </div>
      <div className="landing-footer-wordmark">jackin<span className="tick">'</span></div>
    </footer>
  );
}
