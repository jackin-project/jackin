// docs/components/landing/machineData.ts

export interface Mount {
  src: string;
  dst: string;
  ro: boolean;
}

export interface Workspace {
  workdir: string;
  mounts: Mount[];
  allowed: string[] | null;
}

export interface AgentClass {
  repo: string;
  tools: string;
  plugins: string;
}

export interface Org {
  classes: Record<string, AgentClass>;
  workspaces: Record<string, Workspace>;
}

export const orgs: Record<string, Org> = {
  'jackin-project': {
    classes: {
      'agent-smith':   { repo: 'jackin-project/jackin-agent-smith',   tools: 'git, gh, mise, zsh',              plugins: 'default starter' },
      'the-architect': { repo: 'jackin-project/jackin-the-architect', tools: 'Rust 1.87, cargo, ripgrep, just', plugins: 'superpowers · rust' },
    },
    workspaces: {
      'current-dir': {
        workdir: '$(pwd)',
        mounts: [{ src: '$(pwd)', dst: '$(pwd)', ro: false }],
        allowed: null,
      },
      'jackin-dev': {
        workdir: '~/Projects/jackin-project/jackin',
        mounts: [{ src: '~/Projects/jackin-project/jackin', dst: '~/Projects/jackin-project/jackin', ro: false }],
        allowed: null,
      },
    },
  },
  'chainargos': {
    classes: {
      'chainargos/backend-engineer':  { repo: 'chainargos/jackin-backend-engineer',  tools: 'Go 1.23, Postgres, grpcurl', plugins: 'API · SQL' },
      'chainargos/frontend-engineer': { repo: 'chainargos/jackin-frontend-engineer', tools: 'Node 22, Playwright, pnpm',  plugins: 'UI · a11y' },
      'chainargos/docs-writer':       { repo: 'chainargos/jackin-docs-writer',       tools: 'MDX, Vale, prettier',        plugins: 'writing' },
    },
    workspaces: {
      'monorepo': {
        workdir: '~/Projects/chainargos/monorepo',
        mounts: [{ src: '~/Projects/chainargos/monorepo', dst: '~/Projects/chainargos/monorepo', ro: false }],
        allowed: null,
      },
      'docs-only': {
        workdir: '~/Projects/chainargos/monorepo/docs',
        mounts: [{ src: '~/Projects/chainargos/monorepo/docs', dst: '~/Projects/chainargos/monorepo/docs', ro: true }],
        allowed: ['chainargos/docs-writer'],
      },
    },
  },
  'your-org': {
    classes: {
      'your-org/frontend-engineer': { repo: 'your-org/jackin-frontend-engineer', tools: 'Node 22, Playwright, pnpm',  plugins: 'UI · a11y' },
      'your-org/backend-engineer':  { repo: 'your-org/jackin-backend-engineer',  tools: 'Go 1.23, Postgres, grpcurl', plugins: 'API · SQL' },
    },
    // Project-scoped allowlists: each workspace pins the role that
    // belongs in that codebase. Loading a frontend agent against the
    // backend repo (or vice versa) is refused, keeping roles and
    // projects lined up without relying on operator discipline.
    workspaces: {
      'web-app': {
        workdir: '~/Projects/your-org/web-app',
        mounts: [
          { src: '~/Projects/your-org/web-app',    dst: '~/Projects/your-org/web-app', ro: false },
          { src: '~/Projects/your-org/shared-lib', dst: '/shared',                     ro: true },
        ],
        allowed: ['your-org/frontend-engineer'],
      },
      'api-service': {
        workdir: '~/Projects/your-org/api-service',
        mounts: [
          { src: '~/Projects/your-org/api-service', dst: '~/Projects/your-org/api-service', ro: false },
          { src: '~/Projects/your-org/proto',       dst: '/proto',                          ro: true },
        ],
        allowed: ['your-org/backend-engineer'],
      },
    },
  },
};
