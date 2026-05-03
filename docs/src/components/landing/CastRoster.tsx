// docs/components/landing/CastRoster.tsx
interface Character {
  avatar: string;
  role: string;
  name: string;
  tagline: string;
}

const characters: Character[] = [
  { avatar: 'AS', role: 'General-purpose',   name: 'Agent Smith',  tagline: 'The default starter. Clone, compile, commit.' },
  { avatar: 'AJ', role: 'Backend engineer',  name: 'Agent Jones',  tagline: "Server-side in your company's stack." },
  { avatar: 'AB', role: 'Frontend engineer', name: 'Agent Brown',  tagline: "UI with your team's conventions." },
];

export function CastRoster() {
  return (
    <section className="landing-section">
      <div className="landing-shell">
        <div className="landing-sec-label">05 · Cast</div>
        <h2 className="landing-sec-title">A role for every <span className="accent">job</span>.</h2>
        <p className="landing-sec-intro">Smith, Jones, Brown — archetypes to adopt. Every other role, yours to cast.</p>

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
            <p>Platform engineer, SRE, security reviewer, ML researcher — whatever your team needs. Write the Dockerfile, declare the manifest, push the repo.</p>
          </div>
          <a className="landing-invite-cta" href="/developing/creating-roles/">Read the guide →</a>
        </div>
      </div>
    </section>
  );
}
