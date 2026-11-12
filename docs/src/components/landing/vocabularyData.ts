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
      { t: ' Running the CLI or the console from your host machine. The one who runs the gateway, decides what gets loaded into a sandbox, and when the work comes back out.' },
    ],
  },
  {
    id: '02', term: 'The Gateway', pos: 'noun',
    def: [
      { t: 'The ' },
      { t: 'NVIDIA OpenShell service you run.', b: true },
      { t: ' It creates the sandboxes, enforces their policy, and holds your credentials. Jackin never installs, starts, or stops it.' },
    ],
    cmd: 'jackin gateway add jackin-local --default', cmdLabel: 'cli',
  },
  {
    id: '03', term: 'The Construct', pos: 'noun',
    def: [
      { t: 'The ' },
      { t: 'shared base image', b: true },
      { t: ' every role starts from. Jackin builds it from the OpenShell default sandbox image — the empty white space where Codex gets loaded before a mission.' },
    ],
    cmd: 'FROM construct', cmdLabel: 'dockerfile',
  },
  {
    id: '04', term: 'Role', pos: 'noun',
    def: [
      { t: 'A reusable tool profile built on top of the Construct.', b: true },
      { t: ' A git repo with three files: a Dockerfile that extends the base image, a small manifest, and a native OpenShell policy. Adds the toolchains, hooks, and network rules for one kind of work. Answers “what kind of role is this?”' },
    ],
    cmd: 'jackin role add agent-smith <source>', cmdLabel: 'cli',
  },
  {
    id: '05', term: 'Workspace', pos: 'noun',
    def: [
      { t: 'A saved project.', b: true },
      { t: ' A name and a directory. Jackin uploads the directory into the sandbox as a working tree and a git bundle — never a bind mount. A workspace can bind accounts and turn on the Docker capability. Answers “what can this Codex see?”' },
    ],
    cmd: 'jackin workspace add my-app ~/Projects/my-app', cmdLabel: 'cli',
  },
  {
    id: '06', term: 'Jacking in', pos: 'verb',
    def: [
      { t: 'Launching Codex into a workspace.', b: true },
      { t: ' Builds the role image with Docker BuildKit, delivers it to the gateway, creates the sandbox, uploads the project, runs the role hooks, and attaches you to Codex.' },
    ],
    cmd: 'jackin load agent-smith', cmdLabel: 'cli',
  },
  {
    id: '07', term: 'The agent inside', pos: 'noun',
    def: [
      { t: 'Codex, with full autonomy', b: true },
      { t: ' inside the sandbox wall. OpenShell enforces filesystem, process, and network policy. Your credentials stay on the gateway — Codex only ever sees placeholders. Docker is off unless the role or workspace opts in.' },
    ],
  },
  {
    id: '08', term: 'Hardline', pos: 'verb',
    def: [
      { t: 'Reattach your terminal', b: true },
      { t: ' to a running Codex. Detach and it keeps running — hardline back in, or open Codex and shell panes side by side in Jackin Capsule, the multiplexer of the console.' },
    ],
    cmd: 'jackin attach <sandbox>', cmdLabel: 'cli',
  },
  {
    id: '09', term: 'Pulling out', pos: 'verb',
    def: [
      { t: 'Fetch the work back.', b: true },
      { t: ' Each sandbox branch arrives in your repository as a branch under jackin/<sandbox>/. Your working tree is never touched.' },
    ],
    cmd: 'jackin export <sandbox>', cmdLabel: 'cli',
  },
  {
    id: '10', term: 'Exile', pos: 'verb',
    def: [
      { t: 'Delete the sandbox.', b: true },
      { t: ' Jackin refuses while the work is unexported, unless you force it.' },
    ],
    cmd: 'jackin purge <sandbox>', cmdLabel: 'cli',
  },
];
