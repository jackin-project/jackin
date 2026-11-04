export interface Workspace {
  path: string;
  docker: boolean;
  allowed: string[] | null;
}

export interface Role {
  repo: string;
  tools: string;
  egress: string;
}

export interface Org {
  roles: Record<string, Role>;
  workspaces: Record<string, Workspace>;
}

export const orgs: Record<string, Org> = {
  'jackin-project': {
    roles: {
      'agent-smith':   { repo: 'jackin-project/jackin-agent-smith',   tools: 'git, review instructions',          egress: 'github.com · git fetch only' },
      'the-architect': { repo: 'jackin-project/jackin-the-architect', tools: 'Rust, mise, cargo-nextest, cmake', egress: 'crates.io read-only · github.com git fetch' },
      'sentinel':      { repo: 'jackin-project/jackin-sentinel',      tools: 'jq, sentinel probe',                egress: 'none beyond the Codex provider' },
    },
    workspaces: {
      'my-app': {
        path: '~/Projects/my-app',
        docker: false,
        allowed: null,
      },
      'jackin': {
        path: '~/Projects/jackin-project/jackin',
        docker: false,
        allowed: null,
      },
    },
  },
  'chainargos': {
    roles: {
      'backend-engineer':  { repo: 'chainargos/jackin-backend-engineer',  tools: 'Go 1.23, Postgres, grpcurl', egress: 'proxy.golang.org · github.com git fetch' },
      'frontend-engineer': { repo: 'chainargos/jackin-frontend-engineer', tools: 'Node 22, Playwright, pnpm',  egress: 'registry.npmjs.org read-only' },
      'docs-writer':       { repo: 'chainargos/jackin-docs-writer',       tools: 'MDX, Vale, prettier',        egress: 'none beyond the Codex provider' },
    },
    workspaces: {
      'monorepo': {
        path: '~/Projects/chainargos/monorepo',
        docker: true,
        allowed: null,
      },
      'docs-only': {
        path: '~/Projects/chainargos/monorepo/docs',
        docker: false,
        allowed: ['docs-writer'],
      },
    },
  },
  'your-org': {
    roles: {
      'frontend-engineer': { repo: 'your-org/jackin-frontend-engineer', tools: 'Node 22, Playwright, pnpm',  egress: 'registry.npmjs.org read-only' },
      'backend-engineer':  { repo: 'your-org/jackin-backend-engineer',  tools: 'Go 1.23, Postgres, grpcurl', egress: 'proxy.golang.org · github.com git fetch' },
    },
    workspaces: {
      'web-app': {
        path: '~/Projects/your-org/web-app',
        docker: false,
        allowed: ['frontend-engineer'],
      },
      'api-service': {
        path: '~/Projects/your-org/api-service',
        docker: true,
        allowed: ['backend-engineer'],
      },
    },
  },
};
