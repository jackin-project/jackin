// docs/components/landing/loopData.tsx
import type { ReactNode } from 'react';

export interface LoopFrame {
  id: string;
  name: string;
  mythos: string;
  desc: string;
  terminal: ReactNode;
}

export const loopFrames: LoopFrame[] = [
  {
    id: '01',
    name: 'load',
    mythos: 'Jacking in.',
    desc: 'Enter your project and jack in. Clones the agent repo, builds the derived image, launches the container, drops you into Claude Code.',
    terminal: (
      <>
        <span className="c"># enter your project</span>{'\n'}
        <span className="p">$</span> <span className="cmd">cd ~/Projects/my-app</span>{'\n\n'}
        <span className="c"># jack in</span>{'\n'}
        <span className="p">$</span> <span className="cmd">jackin load agent-smith</span>{'\n'}
        <span className="arrow">{'  → Pulling construct:trixie      '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Cloning agent-smith           '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Building derived image        '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Launching DinD sidecar        '}</span><span className="ok">OK</span>{'\n\n'}
        <span className="check">✓</span> <span className="done">Agent loaded. You're inside.</span>
      </>
    ),
  },
  {
    id: '02',
    name: 'clone',
    mythos: 'More of me.',
    desc: 'Same class, another instance. Spin up a second agent-smith on a different branch or service — separate container, separate state, its own network.',
    terminal: (
      <>
        <span className="c"># first clone · auth redesign</span>{'\n'}
        <span className="p">$</span> <span className="cmd">cd ~/Projects/auth-redesign</span>{'\n'}
        <span className="p">$</span> <span className="cmd">jackin load agent-smith</span>{'\n'}
        <span className="check">✓</span> <span className="done">agent-smith #1 loaded.</span>{'\n\n'}
        <span className="c"># second clone · payments v2</span>{'\n'}
        <span className="p">$</span> <span className="cmd">cd ~/Projects/payment-v2</span>{'\n'}
        <span className="p">$</span> <span className="cmd">jackin load agent-smith</span>{'\n'}
        <span className="check">✓</span> <span className="done">agent-smith #2 loaded.</span>{'\n\n'}
        <span className="arrow">→ 2 agents running. Separate containers, DinD, networks.</span>
      </>
    ),
  },
  {
    id: '03',
    name: 'hardline',
    mythos: 'The hardline.',
    desc: 'Reattach your terminal to a running agent. Closed the window, switched machines, back from a break — pick up where you left off.',
    terminal: (
      <>
        <span className="p">$</span> <span className="cmd">jackin hardline agent-smith</span>{'\n'}
        <span className="arrow">{'  → Locating container            '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Attaching to session          '}</span><span className="ok">OK</span>{'\n\n'}
        <span className="check">✓</span> <span className="done">Session restored. History intact.</span>
      </>
    ),
  },
  {
    id: '04',
    name: 'eject',
    mythos: 'Pulling out.',
    desc: 'Stop one agent cleanly. State persists on disk for next time — the operator decides when a construct is torn down.',
    terminal: (
      <>
        <span className="p">$</span> <span className="cmd">jackin eject agent-smith</span>{'\n'}
        <span className="arrow">{'  → Saving agent state            '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Stopping container            '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Removing network              '}</span><span className="ok">OK</span>{'\n\n'}
        <span className="check">✓</span> <span className="done">Ejected. State preserved.</span>
      </>
    ),
  },
  {
    id: '05',
    name: 'exile',
    mythos: 'Casting out.',
    desc: 'Pull everyone out at once. Every agent stopped, every network removed. End of day, or a panic button.',
    terminal: (
      <>
        <span className="p">$</span> <span className="cmd">jackin exile</span>{'\n'}
        <span className="arrow">  → Exiling 3 agents</span>{'\n'}
        <span className="c">{'     agent-smith                  '}</span><span className="ok">OK</span>{'\n'}
        <span className="c">{'     the-architect                '}</span><span className="ok">OK</span>{'\n'}
        <span className="c">{'     docs-writer                  '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">  → All networks removed</span>{'\n\n'}
        <span className="check">✓</span> <span className="done">All clear. Host untouched.</span>
      </>
    ),
  },
];
