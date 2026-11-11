interface Character {
  avatar: string;
  role: string;
  name: string;
  tagline: string;
}

const characters: Character[] = [
  { avatar: 'AS', role: 'Code review',   name: 'Agent Smith',   tagline: 'Reads the repo, fetches from GitHub, never pushes.' },
  { avatar: 'TA', role: 'jackin❯ dev',   name: 'The Architect', tagline: 'Builds jackin❯ with a locked Rust toolchain.' },
  { avatar: 'SE', role: 'Verification',  name: 'Sentinel',      tagline: 'Proves the path from your host to Codex, with no network rule of its own.' },
];

export function CastRoster() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">05 · Cast</div>
        <h2 className="landing-sec-title">A role for every <span className="accent">job</span>.</h2>
        <p className="landing-sec-intro">Smith, the Architect, the Sentinel — roles to adopt. Every other role, yours to cast.</p>

        <div className="landing-roster">
          {characters.map(c => (
            <div key={c.avatar} className="landing-agent-card">
              <div className="landing-avatar">{c.avatar}</div>
              <div className="landing-role-label">{c.role}</div>
              <div className="landing-character">{c.name}</div>
              <p>{c.tagline}</p>
            </div>
          ))}
        </div>

        <div className="landing-cast-invite">
          <div className="landing-invite-avatar">+</div>
          <div className="landing-invite-body">
            <h3>Cast your own role.</h3>
            <p>Platform engineer, SRE, security reviewer, ML researcher — whatever your team needs. Write the Dockerfile, declare the manifest, write the policy, push the repo.</p>
          </div>
          <a className="landing-invite-cta" href="/developing/creating-roles/">Read the guide →</a>
        </div>
      </div>
    </section>
  );
}
