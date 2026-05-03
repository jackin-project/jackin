// docs/components/landing/vocabularyData.ts
export interface DefSegment {
  t: string;
  b?: boolean;
}

export interface VocabularyEntry {
  id: string;
  term: string;
  pos: 'noun' | 'verb';
  def: DefSegment[];
  cmd?: string;
  cmdLabel?: string;
}

export const vocabularyEntries: VocabularyEntry[] = [
  {
    id: '01', term: 'Operator', pos: 'noun',
    def: [
      { t: 'You.', b: true },
      { t: ' Running the CLI from your host machine. The one who decides what gets loaded into a container, and when agents are pulled back out.' },
    ],
  },
  {
    id: '02', term: 'The Construct', pos: 'noun',
    def: [
      { t: 'The ' },
      { t: 'shared base Docker image', b: true },
      { t: ' every agent extends. Debian plus the jackin\u2019 runtime \u2014 the empty white space where programs get loaded before a mission.' },
    ],
    cmd: 'projectjackin/construct:trixie', cmdLabel: 'image',
  },
  {
    id: '03', term: 'Role', pos: 'noun',
    def: [
      { t: 'A reusable tool profile built on top of the Construct.', b: true },
      { t: ' A git repo with a Dockerfile that extends the base image, plus a small manifest \u2014 adds the toolchains, Claude plugins, shell setup, and conventions layered on top. Answers \u201cwhat kind of role is this?\u201d' },
    ],
    cmd: 'chainargos/backend-engineer', cmdLabel: 'identifier',
  },
  {
    id: '04', term: 'Workspace', pos: 'noun',
    def: [
      { t: 'A named list of mounts and access rules.', b: true },
      { t: ' Each workspace pairs a name with: the host directories that mount into the container, where they land inside, per-mount permission (read-only or read-write), the role\u2019s starting directory (workdir), and which roles are allowed to load it. Answers \u201cwhat can this agent see, and where?\u201d' },
    ],
    cmd: '{ name, workdir, mounts[], allowed-roles[] }', cmdLabel: 'declares',
  },
  {
    id: '05', term: 'Jacking in', pos: 'verb',
    def: [
      { t: 'Loading an agent into a workspace.', b: true },
      { t: ' Clones the role repo, builds the derived image, applies the workspace\u2019s mounts, drops you into Claude Code running inside.' },
    ],
    cmd: 'jackin load agent-smith [my-project-workspace]', cmdLabel: 'cli',
  },
  {
    id: '06', term: 'The agent inside', pos: 'noun',
    def: [
      { t: 'Claude Code running with full permissions', b: true },
      { t: ' inside the container boundary. It thinks the container is the whole world \u2014 and the world ends at the container wall.' },
    ],
  },
  {
    id: '07', term: 'Hardline', pos: 'verb',
    def: [
      { t: 'Reattach your terminal', b: true },
      { t: ' to a running agent. Closed the window? Agent\u2019s still running \u2014 hardline back in and pick up where you left off.' },
    ],
    cmd: 'jackin hardline agent-smith', cmdLabel: 'cli',
  },
  {
    id: '08', term: 'Pulling out', pos: 'verb',
    def: [
      { t: 'Stop an agent cleanly.', b: true },
      { t: ' State persists on disk for next time \u2014 the operator decides when a construct is torn down.' },
    ],
    cmd: 'jackin eject agent-smith', cmdLabel: 'cli',
  },
  {
    id: '09', term: 'Exile', pos: 'verb',
    def: [
      { t: 'Pull everyone out at once.', b: true },
      { t: ' Every running agent, every network, stopped in a single command.' },
    ],
    cmd: 'jackin exile', cmdLabel: 'cli',
  },
];
