// docs/components/landing/Landing.tsx
import { useEffect } from 'react';
import { HeroStage } from './HeroStage';
import { VocabularyDictionary } from './VocabularyDictionary';
import { PillCards } from './PillCards';
import { ApproachCards } from './ApproachCards';
import { CastRoster } from './CastRoster';
import { CompositionMachine } from './CompositionMachine';
import { DailyLoop } from './DailyLoop';
import { InstallBlock } from './InstallBlock';
import { WordmarkFooter } from './WordmarkFooter';

const FONTS_HREF =
  'https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;600&family=Inter:wght@400;500;600;700;800;900&family=Fraunces:opsz,wght@9..144,400;9..144,500;9..144,700&display=swap';

function ensureFontsLink() {
  if (typeof document === 'undefined') return;
  if (document.querySelector('link[data-landing-fonts]')) return;

  const pc1 = document.createElement('link');
  pc1.rel = 'preconnect';
  pc1.href = 'https://fonts.googleapis.com';
  pc1.dataset.landingFonts = 'preconnect-1';
  document.head.appendChild(pc1);

  const pc2 = document.createElement('link');
  pc2.rel = 'preconnect';
  pc2.href = 'https://fonts.gstatic.com';
  pc2.crossOrigin = 'anonymous';
  pc2.dataset.landingFonts = 'preconnect-2';
  document.head.appendChild(pc2);

  const sheet = document.createElement('link');
  sheet.rel = 'stylesheet';
  sheet.href = FONTS_HREF;
  sheet.dataset.landingFonts = 'stylesheet';
  document.head.appendChild(sheet);
}

export function Landing() {
  useEffect(() => {
    ensureFontsLink();
  }, []);

  return (
    <div className="landing-root">
      <HeroStage />
      <VocabularyDictionary />
      <PillCards />
      <ApproachCards />
      <CastRoster />
      <CompositionMachine />
      <DailyLoop />
      <InstallBlock />
      <WordmarkFooter />
    </div>
  );
}
