// docs/components/landing/HeroStage.tsx
import { DigitalRain } from './DigitalRain';
import { HeroContent } from './HeroContent';
import { ThemeToggle } from './ThemeToggle';

export function HeroStage() {
  return (
    <section className="landing-hero-stage">
      <DigitalRain opacity={0.32} />
      <nav className="landing-topnav">
        <div className="landing-logo">jackin<span className="tick">'</span></div>
        <div className="landing-nav-right">
          <a className="landing-star" href="/getting-started/why">
            <svg className="landing-star-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
              <path d="M4 19.5A2.5 2.5 0 0 1 6.5 17H20" />
              <path d="M4 4.5A2.5 2.5 0 0 1 6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5z" />
            </svg>
            Docs
          </a>
          <a className="landing-star" href="https://github.com/jackin-project/jackin" target="_blank" rel="noopener">
            <svg className="landing-star-icon" width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
              <path d="M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01L12 2z"/>
            </svg>
            Star on GitHub
          </a>
        </div>
      </nav>
      <div className="landing-shell">
        <div className="landing-hero">
          <HeroContent />
        </div>
      </div>
      <ThemeToggle />
    </section>
  );
}
