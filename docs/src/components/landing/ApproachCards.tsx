import { TabbedBuilder } from './TabbedBuilder';

const manifestBody = (
  <>
    <span className="b-k">schema</span> = 1{'\n\n'}
    <span className="b-k">[role]</span>{'\n'}
    <span className="b-k">name</span> = <span className="b-s">"backend-engineer"</span>{'\n'}
    <span className="b-k">dockerfile</span> = <span className="b-s">"Dockerfile"</span>{'\n'}
    <span className="b-k">policy</span> = <span className="b-s">"policy.yaml"</span>{'\n\n'}
    <span className="b-k">[docker]</span>{'\n'}
    <span className="b-k">enabled</span> = false{'\n\n'}
    <span className="b-k">[hooks]</span>{'\n'}
    <span className="b-k">preflight</span> = <span className="b-s">"hooks/preflight.sh"</span>
  </>
);

const dockerfileBody = (
  <>
    <span className="b-k">FROM</span> construct{'\n\n'}
    <span className="b-c"># system packages</span>{'\n'}
    <span className="b-k">USER</span> root{'\n'}
    <span className="b-k">RUN</span> apt-get update && apt-get install -y \{'\n'}
    {'    postgresql-client redis-tools\n\n'}
    <span className="b-k">USER</span> sandbox
  </>
);

const policyBody = (
  <>
    <span className="b-k">version</span>: 1{'\n'}
    <span className="b-k">filesystem_policy</span>:{'\n'}
    {'  '}<span className="b-k">read_only</span>: [<span className="b-s">/usr</span>, <span className="b-s">/etc</span>, <span className="b-s">/lib</span>]{'\n'}
    {'  '}<span className="b-k">read_write</span>: [<span className="b-s">/tmp</span>, <span className="b-s">/sandbox</span>]{'\n'}
    <span className="b-k">landlock</span>:{'\n'}
    {'  '}<span className="b-k">compatibility</span>: <span className="b-s">hard_requirement</span>{'\n'}
    <span className="b-k">process</span>:{'\n'}
    {'  '}<span className="b-k">run_as_user</span>: <span className="b-s">sandbox</span>{'\n'}
    {'  '}<span className="b-k">run_as_group</span>: <span className="b-s">sandbox</span>{'\n'}
    <span className="b-k">network_policies</span>:{'\n'}
    {'  '}<span className="b-k">github</span>:{'\n'}
    {'    '}<span className="b-k">name</span>: <span className="b-s">github-git-fetch</span>{'\n'}
    {'    '}<span className="b-k">endpoints</span>:{'\n'}
    {'      - '}<span className="b-k">host</span>: <span className="b-s">github.com</span>{'\n'}
    {'        '}<span className="b-k">port</span>: 443{'\n'}
    {'        '}<span className="b-k">protocol</span>: <span className="b-s">rest</span>{'\n'}
    {'        '}<span className="b-k">enforcement</span>: <span className="b-s">enforce</span>{'\n'}
    {'        '}<span className="b-k">rules</span>:{'\n'}
    {'          - '}<span className="b-k">allow</span>: {'{ '}<span className="b-k">method</span>: GET, <span className="b-k">path</span>: <span className="b-s">"/**/info/refs*"</span>{' }'}{'\n'}
    {'          - '}<span className="b-k">allow</span>: {'{ '}<span className="b-k">method</span>: POST, <span className="b-k">path</span>: <span className="b-s">"/**/git-upload-pack"</span>{' }'}{'\n'}
    {'    '}<span className="b-k">binaries</span>:{'\n'}
    {'      - '}<span className="b-k">path</span>: <span className="b-s">/usr/bin/git</span>
  </>
);

export function ApproachCards() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">04 · The Approach</div>
        <h2 className="landing-sec-title">Draw the boundary <span className="accent">yourself</span>.</h2>
        <p className="landing-sec-intro">jackin❯ gives you exactly one move: a clear boundary around Codex. You decide what's inside — toolchains, hooks, conventions — and what it can reach — filesystem paths, network hosts, even whether it may use Docker. Two ways to get there.</p>

        <div className="landing-approach-grid">
          <div className="landing-approach-card">
            <div className="landing-route">Route 01 · Reuse</div>
            <h3>Pick up an opinionated one</h3>
            <p>Some teams publish roles for their stack. The jackin❯ team ships <em>the-architect</em> — with everything the jackin❯ ecosystem requires to build jackin❯ itself. Add it once with <code>jackin role add</code>, then load it and start working.</p>
            <div className="landing-toolset">
              <span className="landing-toolset-chip">Rust</span>
              <span className="landing-toolset-chip">mr-boxington</span>
              <span className="landing-toolset-chip">cargo-nextest</span>
              <span className="landing-toolset-chip">mise</span>
              <span className="landing-toolset-chip chip-plugin">crates.io read-only</span>
              <span className="landing-toolset-chip chip-plugin">github.com git fetch</span>
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
            <p>Three files, one git repo. A short <code>jackin.role.toml</code> declares the role, its hooks, and the Docker setting. A Dockerfile installs your toolchains and system packages. A <code>policy.yaml</code> is a native OpenShell policy. Versioned, reviewable, <em>self-contained</em>:</p>
            <TabbedBuilder
              tabs={[
                { id: 'manifest',   title: 'jackin.role.toml', body: manifestBody   },
                { id: 'dockerfile', title: 'Dockerfile',        body: dockerfileBody },
                { id: 'policy',     title: 'policy.yaml',       body: policyBody     },
              ]}
              statusLabel="Self-contained ✓"
              statusVariant="built"
            />
            <div className="landing-approach-cmd"><span className="lbl">cli</span>jackin load backend-engineer</div>
            <div className="landing-approach-repo"><span className="lbl">repo</span><span className="repo-path">github.com/your-org/jackin-backend-engineer</span></div>
          </div>
        </div>

        <div className="landing-approach-transition">Either way — <span className="accent">you</span> draw the boundary.</div>
      </div>
    </section>
  );
}
