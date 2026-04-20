// docs/components/landing/Landing.tsx
import { HeroStage } from './HeroStage';
import { VocabularyDictionary } from './VocabularyDictionary';
import { PillCards } from './PillCards';
import { ApproachCards } from './ApproachCards';
import { CastRoster } from './CastRoster';
import { CompositionMachine } from './CompositionMachine';
import { DailyLoop } from './DailyLoop';
import { InstallBlock } from './InstallBlock';
import { WordmarkFooter } from './WordmarkFooter';

export function Landing() {
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
