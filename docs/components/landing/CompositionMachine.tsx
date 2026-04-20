// docs/components/landing/CompositionMachine.tsx
import { useState } from 'react';
import { orgs } from './machineData';

export function CompositionMachine() {
  const orgKeys = Object.keys(orgs);
  const [activeOrg, setActiveOrg] = useState(orgKeys[0]);
  const org = orgs[activeOrg];
  const classKeys = Object.keys(org.classes);
  const wsKeys    = Object.keys(org.workspaces);
  const [activeClass, setActiveClass] = useState(classKeys[0]);
  const [activeWs, setActiveWs] = useState(wsKeys[0]);

  function switchOrg(o: string) {
    setActiveOrg(o);
    const next = orgs[o];
    setActiveClass(Object.keys(next.classes)[0]);
    setActiveWs(Object.keys(next.workspaces)[0]);
  }

  const cl = org.classes[activeClass];
  const ws = org.workspaces[activeWs];
  const denied = ws?.allowed && !ws.allowed.includes(activeClass);
  const shortClass = activeClass.split('/').pop() ?? activeClass;

  return (
    <section id="concepts" className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">06 · Mental Model</div>
        <h2 className="landing-sec-title">Think in <span className="accent">two dimensions</span>.</h2>
        <p className="landing-sec-intro">Same agent in different workspaces. Same workspace with different agents. Pick both — see what runs.</p>

        <div className="landing-machine-wrapper">
          <div className="landing-org-tabs">
            {orgKeys.map(o => (
              <div
                key={o}
                className={'landing-org-tab' + (o === activeOrg ? ' active' : '')}
                onClick={() => switchOrg(o)}
              >
                <span className="at">@</span>{o}
              </div>
            ))}
          </div>

          <div className="landing-machine">
            <div className="landing-machine-panel">
              <div className="landing-machine-label">Agent Class</div>
              <div className="landing-machine-sublabel">the tool profile</div>
              <div className="landing-machine-options">
                {classKeys.map(name => (
                  <div
                    key={name}
                    className={'landing-machine-opt' + (name === activeClass ? ' active' : '')}
                    onClick={() => setActiveClass(name)}
                  >
                    <span className="landing-radio" />{name}
                  </div>
                ))}
              </div>
            </div>

            <div className="landing-machine-op">×</div>

            <div className="landing-machine-panel">
              <div className="landing-machine-label">Workspace</div>
              <div className="landing-machine-sublabel">workdir + mounts</div>
              <div className="landing-machine-options">
                {wsKeys.map(name => (
                  <div
                    key={name}
                    className={'landing-machine-opt' + (name === activeWs ? ' active' : '')}
                    onClick={() => setActiveWs(name)}
                  >
                    <span className="landing-radio" />{name}
                  </div>
                ))}
              </div>
            </div>

            <div className="landing-machine-op">=</div>

            <div className="landing-machine-panel preview">
              <div className="landing-machine-label">Running Agent</div>
              <div className="landing-machine-sublabel">the resulting container</div>
              <div className="landing-preview">
                {denied ? (
                  <div className="landing-preview-denied">
                    <span className="label">✕ not loaded</span>
                    Workspace "{activeWs}" declares <code>allowed-agents: [{ws?.allowed?.join(', ')}]</code>.
                    Rejected before the container starts.
                  </div>
                ) : cl && ws ? (
                  <>
                    <PreviewRow k="container"><span className="hl">jackin-{shortClass}</span></PreviewRow>
                    <PreviewRow k="class">{activeClass}</PreviewRow>
                    <PreviewRow k="repo">github.com/{cl.repo}</PreviewRow>
                    <PreviewRow k="tools">{cl.tools}</PreviewRow>
                    <PreviewRow k="plugins">{cl.plugins}</PreviewRow>
                    <PreviewRow k="workdir">{ws.workdir}</PreviewRow>
                    <PreviewRow k="mounts">
                      <div className="landing-mount-list">
                        {ws.mounts.map((m, i) => (
                          <div key={i} className="landing-mount-item">
                            {m.src === m.dst ? (
                              <span className="src">{m.src}</span>
                            ) : (
                              <>
                                <span className="src">{m.src}</span>
                                <span className="arrow">→</span>
                                <span className="dst">{m.dst}</span>
                              </>
                            )}
                            <span className={'perm ' + (m.ro ? 'ro' : 'rw')}>{m.ro ? 'ro' : 'rw'}</span>
                          </div>
                        ))}
                      </div>
                    </PreviewRow>
                    <PreviewRow k="network">jackin-{shortClass}-net</PreviewRow>
                  </>
                ) : null}
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}

function PreviewRow({ k, children }: { k: string; children: React.ReactNode }) {
  return (
    <div className="landing-preview-row">
      <span className="k">{k}</span>
      <span className="v">{children}</span>
    </div>
  );
}
