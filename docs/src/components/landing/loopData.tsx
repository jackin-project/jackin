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
    desc: 'Enter your project and jack in. Jackin builds the role image with Docker BuildKit and delivers it to your OpenShell gateway, creates a sandbox, uploads a copy of the project, runs the role hooks, and drops you into Codex.',
    terminal: (
      <>
        <span className="c"># enter your project</span>{'\n'}
        <span className="p">$</span> <span className="cmd">cd ~/Projects/my-app</span>{'\n\n'}
        <span className="c"># jack in</span>{'\n'}
        <span className="p">$</span> <span className="cmd">jackin load agent-smith</span>{'\n'}
        <span className="arrow">{'  → Image                         '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Provider                      '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Sandbox   jackin-my-app-3f9a  '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Upload                        '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Hooks                         '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Codex                         '}</span><span className="ok">OK</span>{'\n\n'}
        <span className="check">✓</span> <span className="done">Codex is running. You're inside.</span>
      </>
    ),
  },
  {
    id: '02',
    name: 'ls',
    mythos: 'More of me.',
    desc: 'Same role, another project. Every launch gets its own sandbox, its own upload, and its own policy. Jackin keeps no local index: it finds your sandboxes by their labels on the gateway.',
    terminal: (
      <>
        <span className="c"># second project · payments v2</span>{'\n'}
        <span className="p">$</span> <span className="cmd">cd ~/Projects/payments-v2</span>{'\n'}
        <span className="p">$</span> <span className="cmd">jackin load agent-smith</span>{'\n'}
        <span className="check">✓</span> <span className="done">Codex is running. You're inside.</span>{'\n\n'}
        <span className="c"># list every sandbox</span>{'\n'}
        <span className="p">$</span> <span className="cmd">jackin ls</span>{'\n'}
        <span className="arrow">{'  jackin-my-app-3f9a   my-app       agent-smith  work  Ready'}</span>{'\n'}
        <span className="arrow">{'  jackin-payment-b71e  payments-v2  agent-smith  work  Ready'}</span>
      </>
    ),
  },
  {
    id: '03',
    name: 'attach',
    mythos: 'The hardline.',
    desc: 'Reattach your terminal to a running Codex. Detach with Ctrl-D and Codex keeps running. The gateway replays the last 1 MiB of output when you come back, so you pick up where you left off.',
    terminal: (
      <>
        <span className="p">$</span> <span className="cmd">jackin attach jackin-my-app-3f9a</span>{'\n'}
        <span className="arrow">{'  → Opening session               '}</span><span className="ok">OK</span>{'\n'}
        <span className="arrow">{'  → Replaying output              '}</span><span className="ok">OK</span>{'\n\n'}
        <span className="check">✓</span> <span className="done">Session restored. Codex kept running.</span>
      </>
    ),
  },
  {
    id: '04',
    name: 'export',
    mythos: 'Pulling out.',
    desc: 'Fetch the work back. Jackin bundles each sandbox branch, checks the bundle, and writes it into your repository as a branch under refs/heads/jackin. Your working tree is never touched.',
    terminal: (
      <>
        <span className="p">$</span> <span className="cmd">jackin export jackin-my-app-3f9a</span>{'\n'}
        <span className="cmd">refs/heads/jackin/jackin-my-app-3f9a/main</span>{'  '}<span className="c">9f3c2a1</span>{'\n\n'}
        <span className="c"># review it like any other branch</span>{'\n'}
        <span className="p">$</span> <span className="cmd">git log jackin/jackin-my-app-3f9a/main</span>{'\n'}
        <span className="p">$</span> <span className="cmd">git merge jackin/jackin-my-app-3f9a/main</span>
      </>
    ),
  },
  {
    id: '05',
    name: 'purge',
    mythos: 'Casting out.',
    desc: 'Delete the sandbox when the work is out. Jackin refuses while work is unexported, unless you force it. End of day, or a clean slate.',
    terminal: (
      <>
        <span className="p">$</span> <span className="cmd">jackin purge jackin-payment-b71e</span>{'\n'}
        <span className="arrow">{'  the work in sandbox jackin-payment-b71e is not exported\n  (branch main not exported); run jackin export jackin-payment-b71e\n  or pass --force\n\n'}</span>
        <span className="p">$</span> <span className="cmd">jackin export jackin-payment-b71e</span>{'\n'}
        <span className="p">$</span> <span className="cmd">jackin purge jackin-payment-b71e</span>{'\n'}
        <span className="arrow">  deleted sandbox jackin-payment-b71e</span>
      </>
    ),
  },
];
