import { useEffect, useRef, useState } from 'react';

interface Line {
  cls: string;
  text: string;
}

const tabs = ['load', 'attach', 'export'] as const;
type Tab = (typeof tabs)[number];

const scripts: Record<Tab, Line[]> = {
  load: [
    { cls: 'c',      text: '# Launch Codex in an OpenShell sandbox\n' },
    { cls: 'cmd',    text: 'j' },
    { cls: 'prompt', text: '❯ ' },
    { cls: 'cmd',    text: 'jackin ' },
    { cls: 'k',      text: 'load' },
    { cls: 'cmd',    text: ' agent-smith\n' },
    { cls: 'dim',    text: '  → Image        ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  → Provider     ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  → Sandbox      ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  → Upload       ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  → Hooks        ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  → Codex        ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'cmd',    text: '\n✓ Codex is running. You’re inside.' },
  ],
  attach: [
    { cls: 'c',      text: '# Reattach to a running Codex session\n' },
    { cls: 'cmd',    text: 'j' },
    { cls: 'prompt', text: '❯ ' },
    { cls: 'cmd',    text: 'jackin ' },
    { cls: 'k',      text: 'attach' },
    { cls: 'cmd',    text: ' jackin-my-app-3f9a\n' },
    { cls: 'dim',    text: '  → Opening session   ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  → Replaying output  ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'cmd',    text: '\n✓ Session restored. Codex kept running.' },
  ],
  export: [
    { cls: 'c',      text: '# Fetch the work back as git branches\n' },
    { cls: 'cmd',    text: 'j' },
    { cls: 'prompt', text: '❯ ' },
    { cls: 'cmd',    text: 'jackin ' },
    { cls: 'k',      text: 'export' },
    { cls: 'cmd',    text: ' jackin-my-app-3f9a\n' },
    { cls: 'cmd',    text: 'refs/heads/jackin/jackin-my-app-3f9a/main' },
    { cls: 'dim',    text: '  9f3c2a1\n' },
    { cls: 'cmd',    text: '\n✓ Exported. Your working tree is untouched.' },
  ],
};

const typingSpeedCharMs  = 11;
const typingSpeedLineMs  = 110;
const holdDurationMs     = 3500;

export function CodePanel() {
  const [active, setActive] = useState<Tab>('load');
  const bodyRef = useRef<HTMLDivElement>(null);
  const tokenRef = useRef(0);

  useEffect(() => {
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    tokenRef.current += 1;
    const myToken = tokenRef.current;
    const body = bodyRef.current;
    if (!body) return;

    async function run(host: HTMLDivElement) {
      while (myToken === tokenRef.current) {
        while (host.firstChild) host.removeChild(host.firstChild);
        for (const line of scripts[active]) {
          if (myToken !== tokenRef.current) return;
          const span = document.createElement('span');
          span.className = line.cls;
          host.appendChild(span);
          if (reducedMotion) {
            span.appendChild(document.createTextNode(line.text));
            continue;
          }
          for (const ch of line.text) {
            if (myToken !== tokenRef.current) return;
            span.appendChild(document.createTextNode(ch));
            await new Promise(r => setTimeout(r, ch === '\n' ? typingSpeedLineMs : typingSpeedCharMs));
          }
        }
        const cursor = document.createElement('span');
        cursor.className = 'cursor';
        host.appendChild(cursor);
        if (reducedMotion) return;
        await new Promise(r => setTimeout(r, holdDurationMs));
      }
    }
    run(body);

    return () => { tokenRef.current += 1; };
  }, [active]);

  return (
    <div className="landing-code-panel">
      <div className="landing-code-head">
        <div className="landing-code-tabs">
          {tabs.map(k => (
            <button
              type="button"
              key={k}
              className={'landing-code-tab' + (k === active ? ' active' : '')}
              onClick={() => setActive(k)}
            >
              $ {k}
            </button>
          ))}
        </div>
      </div>
      <div ref={bodyRef} className="landing-code-body" />
    </div>
  );
}
