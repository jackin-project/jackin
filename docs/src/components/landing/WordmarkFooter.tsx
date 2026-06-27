// docs/components/landing/WordmarkFooter.tsx
import { BrandMark } from '../brand/BrandMark';

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
      <BrandMark className="landing-footer-wordmark" />
    </footer>
  );
}
