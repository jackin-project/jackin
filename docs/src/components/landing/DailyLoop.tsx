// docs/components/landing/DailyLoop.tsx
import { loopFrames } from './loopData';

export function DailyLoop() {
  return (
    <section id="commands" className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">07 · How it Works</div>
        <h2 className="landing-sec-title">The <span className="accent">daily loop</span>.</h2>
        <p className="landing-sec-intro">Five moves. Any number of agents. A full day's flow with jackin'.</p>

        <div className="landing-loop">
          {loopFrames.map(f => (
            <div key={f.id} className="landing-loop-frame">
              <div className="landing-loop-info">
                <div className="landing-loop-num">№ {f.id}</div>
                <div className="landing-loop-name">{f.name}</div>
                <div className="landing-loop-mythos">{f.mythos}</div>
                <p className="landing-loop-desc">{f.desc}</p>
              </div>
              <div className="landing-loop-term">
                <div className="landing-loop-term-bar">
                  <span className="landing-dot r" />
                  <span className="landing-dot y" />
                  <span className="landing-dot g" />
                </div>
                <pre className="landing-loop-term-body">{f.terminal}</pre>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
