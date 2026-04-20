// docs/components/landing/ApproachCards.tsx
import { TabbedBuilder } from './TabbedBuilder';

const manifestBody = (
  <>
    <span className="b-k">dockerfile</span> = <span className="b-s">"Dockerfile"</span>{'\n\n'}
    <span className="b-k">[identity]</span>{'\n'}
    <span className="b-k">name</span> = <span className="b-s">"Backend Engineer"</span>{'\n\n'}
    <span className="b-k">[claude]</span>{'\n'}
    <span className="b-k">plugins</span> = [{'\n'}
    {'  '}<span className="b-s">"superpowers@superpowers-marketplace"</span>,{'\n'}
    ]{'\n\n'}
    <span className="b-k">[[claude.marketplaces]]</span>{'\n'}
    <span className="b-k">source</span> = <span className="b-s">"obra/superpowers-marketplace"</span>
  </>
);

const dockerfileBody = (
  <>
    <span className="b-k">FROM</span> projectjackin/construct:trixie{'\n\n'}
    <span className="b-c"># language toolchains via mise</span>{'\n'}
    <span className="b-k">RUN</span> mise install go@1.23 \{'\n'}
    {'    && mise use --global go@1.23\n\n'}
    <span className="b-c"># system packages</span>{'\n'}
    <span className="b-k">USER</span> root{'\n'}
    <span className="b-k">RUN</span> apt-get update && apt-get install -y \{'\n'}
    {'    postgresql-client redis-tools\n'}
    <span className="b-k">USER</span> claude
  </>
);

export function ApproachCards() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">04 · The Approach</div>
        <h2 className="landing-sec-title">Draw the boundary <span className="accent">yourself</span>.</h2>
        <p className="landing-sec-intro">jackin' gives you exactly one move: a clear boundary around an AI agent. You decide what's inside — toolchains, plugins, conventions — and what it can reach — paths, tokens, exposed ports. Two ways to get there.</p>

        <div className="landing-approach-grid">
          <div className="landing-approach-card">
            <div className="landing-route">Route 01 · Reuse</div>
            <h3>Pick up an opinionated one</h3>
            <p>Some orgs publish agent classes for their stack. The jackin team ships <em>the-architect</em> — with everything the jackin ecosystem requires to build jackin itself. Zero config: load and start working.</p>
            <div className="landing-toolset">
              <span className="landing-toolset-chip">Rust stable</span>
              <span className="landing-toolset-chip">cargo-nextest</span>
              <span className="landing-toolset-chip">cargo-watch</span>
              <span className="landing-toolset-chip chip-plugin">code-review</span>
              <span className="landing-toolset-chip chip-plugin">feature-dev</span>
              <span className="landing-toolset-chip chip-plugin">superpowers</span>
              <span className="landing-toolset-chip chip-plugin">jackin-dev</span>
            </div>
            <p className="landing-approach-note">Your framework's team can ship one just like it for yours.</p>
            <div className="landing-approach-cmd"><span className="lbl">cli</span>jackin load the-architect</div>
            <div className="landing-approach-repo">
              <span className="lbl">repo</span>
              <a
                className="repo-path"
                href="https://github.com/jackin-project/jackin-the-architect"
                target="_blank"
                rel="noopener noreferrer"
              >github.com/jackin-project/jackin-the-architect</a>
            </div>
          </div>

          <div className="landing-approach-card">
            <div className="landing-route">Route 02 · Build</div>
            <h3>Cast your own</h3>
            <p>Two files, one git repo. A short <code>jackin.agent.toml</code> declares identity and Claude plugins. A Dockerfile installs your language toolchains and system packages. Versioned, reviewable, <em>self-contained</em>:</p>
            <TabbedBuilder
              tabs={[
                { id: 'manifest',   title: 'jackin.agent.toml', body: manifestBody   },
                { id: 'dockerfile', title: 'Dockerfile',        body: dockerfileBody },
              ]}
              statusLabel="Self-contained ✓"
              statusVariant="built"
            />
            <div className="landing-approach-cmd"><span className="lbl">cli</span>jackin load your-org/backend</div>
            <div className="landing-approach-repo"><span className="lbl">repo</span><span className="repo-path">github.com/your-org/jackin-backend</span></div>
          </div>
        </div>

        <div className="landing-approach-transition">Either way — <span className="accent">you</span> draw the boundary.</div>
      </div>
    </section>
  );
}
