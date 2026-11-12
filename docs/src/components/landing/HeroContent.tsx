import { CodePanel } from './CodePanel';

export function HeroContent() {
  function scrollToGetStarted(e: React.MouseEvent<HTMLAnchorElement>) {
    e.preventDefault();
    document
      .getElementById('getstarted')
      ?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }

  return (
    <>
      <div className="landing-hero-grid">
        <div className="landing-hero-left">
          <h1 className="landing-hero-headline">
            You're the <span className="accent">Operator</span>.<br />
            <span className="soft">They're already</span> inside.
          </h1>
          <p className="landing-hero-deck">
            jackin❯ runs Codex in NVIDIA OpenShell sandboxes: full autonomy inside, your host untouched outside. Your project goes in as a copy, credentials stay on your gateway, and the work comes back as git branches.
          </p>
          <div className="landing-hero-ctas">
            <a
              className="landing-btn-primary"
              href="#getstarted"
              onClick={scrollToGetStarted}
            >
              Get Started →
            </a>
          </div>
        </div>
        <div className="landing-hero-right">
          <CodePanel />
        </div>
      </div>
    </>
  );
}
