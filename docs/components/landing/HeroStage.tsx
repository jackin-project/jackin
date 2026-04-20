// docs/components/landing/HeroStage.tsx
import { DigitalRain } from './DigitalRain';
import { HeroContent } from './HeroContent';

export function HeroStage() {
  return (
    <section className="landing-hero-stage">
      <DigitalRain opacity={0.32} />
      <nav className="landing-topnav">
        <div className="landing-logo"><span className="mark" />jackin'</div>
        <div className="landing-nav-right">
          <a className="landing-star" href="https://github.com/jackin-project/jackin" target="_blank" rel="noopener">★ Star on GitHub</a>
        </div>
      </nav>
      <div className="landing-shell">
        <section className="landing-hero">
          <HeroContent />
        </section>
      </div>
    </section>
  );
}
