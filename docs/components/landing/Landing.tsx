// docs/components/landing/Landing.tsx
import { PillCards } from './PillCards';
import { ApproachCards } from './ApproachCards';
import { CastRoster } from './CastRoster';
import { CompositionMachine } from './CompositionMachine';
import { InstallBlock } from './InstallBlock';
import { VocabularyDictionary } from './VocabularyDictionary';
import { WordmarkFooter } from './WordmarkFooter';

export function Landing() {
  return (
    <div className="landing-root">
      <VocabularyDictionary />
      <PillCards />
      <ApproachCards />
      <CastRoster />
      <CompositionMachine />
      <InstallBlock />
      <WordmarkFooter />
    </div>
  );
}
