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
  const slug = activeWs.slice(0, 7).replace(/-+$/, '');

  return (
    <section id="concepts" className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">06 · Mental Model</div>
        <h2 className="landing-sec-title">Think in <span className="accent">two dimensions</span>.</h2>
        <p className="landing-sec-intro">Same role in different workspaces. Same workspace with different roles. Pick both — see what launches.</p>

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
              <div className="landing-machine-sublabel">the tool profile and egress rules</div>
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
              <div className="landing-machine-sublabel">the saved project</div>
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
              <div className="landing-machine-label">Running Sandbox</div>
              <div className="landing-machine-sublabel">the resulting sandbox</div>
              <div className="landing-preview">
                {denied ? (
                  <div className="landing-preview-denied">
                    <span className="label">✕ not loaded</span>
                    Workspace "{activeWs}" declares <code>allowed roles: [{ws?.allowed?.join(', ')}]</code>.
                    Rejected before the sandbox is created.
                  </div>
                ) : role && ws ? (
                  <>
                    <PreviewRow k="sandbox"><span className="hl">jackin-{slug}-3f9a</span></PreviewRow>
                    <PreviewRow k="agent">Codex</PreviewRow>
                    <PreviewRow k="role">{activeRole}</PreviewRow>
                    <PreviewRow k="repo">github.com/{role.repo}</PreviewRow>
                    <PreviewRow k="tools">{role.tools}</PreviewRow>
                    <PreviewRow k="egress">{role.egress}</PreviewRow>
                    <PreviewRow k="upload">
                      <div className="landing-mount-list">
                        <div className="landing-mount-item">
                          <span className="src">{ws.path}</span>
                          <span className="arrow">→</span>
                          <span className="dst">/sandbox/project</span>
                          <span className="perm rw">copy</span>
                        </div>
                      </div>
                    </PreviewRow>
                    <PreviewRow k="docker">{ws.docker ? 'on · engine you run' : 'off'}</PreviewRow>
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
