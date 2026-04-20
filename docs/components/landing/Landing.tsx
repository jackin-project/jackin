// docs/components/landing/Landing.tsx
import { PillCards } from './PillCards';
import { ApproachCards } from './ApproachCards';
import { InstallBlock } from './InstallBlock';
import { WordmarkFooter } from './WordmarkFooter';

export function Landing() {
  return (
    <div className="landing-root">
      <PillCards />
      <ApproachCards />
      <InstallBlock />
      <WordmarkFooter />
    </div>
  );
}
