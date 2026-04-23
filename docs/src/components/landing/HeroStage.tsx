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
          <ThemeToggle />
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
    </section>
  );
}
