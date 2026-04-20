// docs/components/landing/CodePanel.tsx
import { useEffect, useRef, useState } from 'react';

interface Line {
  cls: string;
  text: string;
}

const scripts: Record<string, Line[]> = {
  load: [
    { cls: 'c',      text: '# Load an isolated agent into your current project\n' },
    { cls: 'prompt', text: '$ ' },
    { cls: 'cmd',    text: 'jackin ' },
    { cls: 'k',      text: 'load' },
    { cls: 'cmd',    text: ' agent-smith\n' },
    { cls: 'dim',    text: '  \u2192 Pulling construct:trixie     ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  \u2192 Building derived image       ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  \u2192 Per-agent Docker network     ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  \u2192 Claude Code ready             ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'cmd',    text: '\n\u2713 Agent loaded. You\u2019re inside.' },
  ],
  hardline: [
    { cls: 'c',      text: '# Reattach to a running agent session\n' },
    { cls: 'prompt', text: '$ ' },
    { cls: 'cmd',    text: 'jackin ' },
    { cls: 'k',      text: 'hardline' },
    { cls: 'cmd',    text: ' agent-smith\n' },
    { cls: 'dim',    text: '  \u2192 Locating container...         ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  \u2192 Attaching to session...        ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'cmd',    text: '\n\u2713 Session restored. History intact.' },
  ],
  eject: [
    { cls: 'c',      text: '# Stop an agent cleanly (state persists)\n' },
    { cls: 'prompt', text: '$ ' },
    { cls: 'cmd',    text: 'jackin ' },
    { cls: 'k',      text: 'eject' },
    { cls: 'cmd',    text: ' agent-smith\n' },
    { cls: 'dim',    text: '  \u2192 Saving agent state...          ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  \u2192 Stopping container...          ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'dim',    text: '  \u2192 Removing network...            ' },
    { cls: 'ok',     text: 'OK\n' },
    { cls: 'cmd',    text: '\n\u2713 Ejected. Host clean.' },
  ],
};

const typingSpeedCharMs  = 11;
const typingSpeedLineMs  = 110;
const holdDurationMs     = 3500;

export function CodePanel() {
  const [active, setActive] = useState<'load' | 'hardline' | 'eject'>('load');
  const bodyRef = useRef<HTMLDivElement>(null);
  const tokenRef = useRef(0);

  useEffect(() => {
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    tokenRef.current += 1;
    const myToken = tokenRef.current;
    const body = bodyRef.current;
    if (!body) return;

    async function run() {
      while (myToken === tokenRef.current) {
        // Clear
        while (body!.firstChild) body!.removeChild(body!.firstChild);
        for (const line of scripts[active]) {
          if (myToken !== tokenRef.current) return;
          const span = document.createElement('span');
          span.className = line.cls;
          body!.appendChild(span);
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
        body!.appendChild(cursor);
        if (reducedMotion) return;
        await new Promise(r => setTimeout(r, holdDurationMs));
      }
    }
    run();

    return () => { tokenRef.current += 1; };
  }, [active]);

  return (
    <div className="landing-code-panel">
      <div className="landing-code-head">
        <div className="landing-code-tabs">
          {(['load', 'hardline', 'eject'] as const).map(k => (
            <span
              key={k}
              className={'landing-code-tab' + (k === active ? ' active' : '')}
              onClick={() => setActive(k)}
            >
              $ {k}
            </span>
          ))}
        </div>
      </div>
      <div ref={bodyRef} className="landing-code-body" />
    </div>
  );
}
