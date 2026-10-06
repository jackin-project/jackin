// docs/components/landing/CompositionMachine.tsx
import { useState } from 'react';
import { orgs } from './machineData';
import { FocusCallout } from './FocusCallout';

export function CompositionMachine() {
  const orgKeys = Object.keys(orgs);
  const [activeOrg, setActiveOrg] = useState(orgKeys[0]);
  const org = orgs[activeOrg];
  const roleKeys = Object.keys(org.roles);
  const wsKeys   = Object.keys(org.workspaces);
  const [activeRole, setActiveRole] = useState(roleKeys[0]);
  const [activeWs, setActiveWs] = useState(wsKeys[0]);

  function switchOrg(o: string) {
    setActiveOrg(o);
    const next = orgs[o];
    setActiveRole(Object.keys(next.roles)[0]);
    setActiveWs(Object.keys(next.workspaces)[0]);
  }

  const role = org.roles[activeRole];
  const ws = org.workspaces[activeWs];
  const denied = ws?.allowed && !ws.allowed.includes(activeRole);
  const shortRole = activeRole.split('/').pop() ?? activeRole;

  return (
    <section id="concepts" className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">06 · Mental Model</div>
        <h2 className="landing-sec-title">Think in <span className="accent">two dimensions</span>.</h2>
        <p className="landing-sec-intro">Same agent in different workspaces. Same workspace with different agents. Pick both — see what runs.</p>

        <div className="landing-machine-wrapper">
          <div className="landing-org-tabs">
            {orgKeys.map(o => (
              <button
                type="button"
                key={o}
                className={'landing-org-tab' + (o === activeOrg ? ' active' : '')}
                onClick={() => switchOrg(o)}
              >
                <span className="at">@</span>{o}
              </button>
            ))}
          </div>

          <div className="landing-machine">
            <div className="landing-machine-panel">
              <div className="landing-machine-label">Role</div>
              <div className="landing-machine-sublabel">the tool profile</div>
              <div className="landing-machine-options">
                {roleKeys.map(name => (
                  <button
                    type="button"
                    key={name}
                    className={'landing-machine-opt' + (name === activeRole ? ' active' : '')}
                    onClick={() => setActiveRole(name)}
                  >
                    <span className="landing-radio" />{name}
                  </button>
                ))}
              </div>
            </div>

            <div className="landing-machine-op">×</div>

            <div className="landing-machine-panel">
              <div className="landing-machine-label">Workspace</div>
              <div className="landing-machine-sublabel">workdir + mounts</div>
              <div className="landing-machine-options">
                {wsKeys.map(name => (
                  <button
                    type="button"
                    key={name}
                    className={'landing-machine-opt' + (name === activeWs ? ' active' : '')}
                    onClick={() => setActiveWs(name)}
                  >
                    <span className="landing-radio" />{name}
                  </button>
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
                    Workspace "{activeWs}" declares <code>allowed-roles: [{ws?.allowed?.join(', ')}]</code>.
                    Rejected before the container starts.
                  </div>
                ) : role && ws ? (
                  <>
                    <PreviewRow k="container"><span className="hl">jackin-{shortRole}</span></PreviewRow>
                    <PreviewRow k="role">{activeRole}</PreviewRow>
                    <PreviewRow k="repo">github.com/{role.repo}</PreviewRow>
                    <PreviewRow k="tools">{role.tools}</PreviewRow>
                    <PreviewRow k="plugins">{role.plugins}</PreviewRow>
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
                    <PreviewRow k="network">jackin-{shortRole}-net</PreviewRow>
                  </>
                ) : null}
              </div>
            </div>
          </div>
        </div>
        <FocusCallout />
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
