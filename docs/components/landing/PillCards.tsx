// docs/components/landing/PillCards.tsx
export function PillCards() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">03 · The Problem</div>
        <h2 className="landing-sec-title">The <span className="accent">false</span> choice.</h2>
        <p className="landing-sec-intro">Every AI coding agent offers you a pill. The false choice is thinking you have to swallow either one.</p>

        <div className="landing-pills">
          <div className="landing-pill-card blue">
            <div className="landing-pill-visual">
              <div className="landing-pill-color" />
              <div className="landing-pill-white" />
            </div>
            <div className="landing-pill-meta">Blue pill</div>
            <h3>Babysit every prompt</h3>
            <ul className="landing-choice-lines">
              <li>"Are you sure?" dialogs every ten seconds.</li>
              <li>Permission gates on every action.</li>
              <li>The agent waits on you, constantly.</li>
              <li>Flow interrupted a hundred times a day.</li>
            </ul>
            <div className="landing-choice-verdict">Productivity · destroyed</div>
          </div>
          <div className="landing-pill-card red">
            <div className="landing-pill-visual">
              <div className="landing-pill-color" />
              <div className="landing-pill-white" />
            </div>
            <div className="landing-pill-meta">Red pill</div>
            <h3>Full YOLO on host</h3>
            <ul className="landing-choice-lines">
              <li>Agent reads every file — SSH keys, <code>.env</code>, cookies.</li>
              <li>Runs any command on your machine.</li>
              <li>Installs any package it wants — supply chain and all.</li>
              <li>One bad prompt is an unrecoverable bad day.</li>
            </ul>
            <div className="landing-choice-verdict">Risk · maximum</div>
          </div>
        </div>

        <div className="landing-choice-transition">
          Refuse the pill. <span className="accent">You're the Operator</span> — define the construct instead.
        </div>
      </div>
    </section>
  );
}
