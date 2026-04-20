// docs/components/landing/HeroContent.tsx
import { CodePanel } from './CodePanel';

export function HeroContent() {
  return (
    <div className="landing-hero-grid">
      <div className="landing-hero-left">
        <h1 className="landing-hero-headline">
          You're the <span className="accent">Operator</span>.<br />
          <span className="soft">They're already</span> inside.
        </h1>
        <p className="landing-hero-deck">
          jackin' drops AI coding agents into isolated Docker containers — full autonomy inside, your host untouched outside. One CLI. Same-path mounts. Per-agent state.
        </p>
        <div className="landing-hero-ctas">
          <a className="landing-btn-primary" href="#getstarted">Get Started →</a>
        </div>
      </div>
      <div className="landing-hero-right">
        <CodePanel />
      </div>
    </div>
  );
}
