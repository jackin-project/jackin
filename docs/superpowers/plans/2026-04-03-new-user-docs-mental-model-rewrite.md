# New-User Docs Mental Model Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the beginner-facing docs so new users quickly understand why jackin separates agent classes from workspaces, why one project can use multiple specialized agent classes, and why saved workspaces are worth creating.

**Architecture:** Keep the current README and docs page structure, but rewrite the copy so it leads with practical workflow and repeats one mental model consistently: workspaces isolate files, agent classes isolate tools and behavior, and narrower environments produce better agent results. Standardize third-party examples around role-persona names like `chainargos/backend-engineer` instead of reusing project-owned names such as `the-architect`.

**Tech Stack:** Markdown, MDX, Astro Starlight, Bun

---

### Task 1: Rewrite README Beginner Framing

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Replace the `## Mental Model` section with a practical-first explanation**

Replace the block that starts with `There are three core ideas in jackin:` and ends with `That can be a practical way to keep certain workflows or hooks completely out of scope for specific tasks.` with the following content:

```md
There are three core ideas in `jackin`:

- **Agent class** — a reusable tool profile defined by a GitHub repo and loaded by name, such as `agent-smith`, `the-architect`, `chainargos/frontend-engineer`, or `chainargos/backend-engineer`
- **Workspace** — the project access boundary: which host directories are mounted, where they appear in the container, and which agent classes are allowed to use them
- **Agent instance** — one running container created from an agent class and attached to one workspace

`agent-smith` is just the default starter class name in this project. It is not magic syntax. In a real company you might have classes like `frontend-engineer`, `backend-engineer`, `infra-operator`, or `security-reviewer`.

This distinction matters because `jackin` isolates two different things on purpose.

- A workspace answers: **which files can this agent see?**
- An agent class answers: **which tools, defaults, plugins, and runtime behavior does this agent have?**

That separation is useful even when the project stays the same. One project can intentionally use multiple agent classes:

- `chainargos/frontend-engineer` can mount the same monorepo workspace but carry Node, Playwright, design-system tooling, and UI-focused plugins
- `chainargos/backend-engineer` can mount that same workspace but carry Rust or Go tooling, database clients, and backend-oriented plugins

This is not duplication. It is how you create a smaller, more relevant ecosystem for the agent. A kitchen-sink image with every tool and every plugin gives the model more surface area to inspect and react to. A narrower environment usually produces better results because more of what the agent sees is relevant to the task.

This is also useful for controlling Claude Code plugin behavior. If one agent class includes something powerful like Superpowers and another agent class does not, the second container genuinely cannot load it because it is not installed there. That is often more reliable than trying to "mostly disable" tools in one giant shared image.
```

- [ ] **Step 2: Rewrite the `## Workspaces` explanation so saved workspaces feel like reusable project boundaries**

Replace the block that starts with `` `jackin launch` is the fastest way to start work.`` and ends with `Another pattern is the opposite: reuse one agent class across many workspaces when the tooling stays the same but the projects differ.` with the following content:

```md
`jackin launch` is the fastest way to start work. It shows two kinds of workspace choices:

- `Current directory` — a synthetic workspace that mounts the current directory to the same absolute path inside the container and uses that path as `workdir`
- saved workspaces — named local definitions stored in `~/.config/jackin/config.toml`

The current-directory flow is great when you want to move fast from inside a project. But saved workspaces are more than shortcuts. They let you name a project boundary once and reuse it predictably.

A saved workspace is useful when you want to:

- launch the same project from anywhere without retyping mounts
- keep a multi-mount layout consistent across sessions
- let `jackin launch` auto-detect and preselect the right project
- set a default agent class for that project
- restrict sensitive workspaces to a smaller set of agent classes

`launch` is the human-first flow: pick a workspace, preview mounts and `workdir`, then choose an agent. `load` stays the explicit terminal-first path: pass a path, a `path:container-dest` mapping, or a saved workspace name as the optional second argument. Use `--mount` to layer additional mounts on top of any target type.

Saved workspaces are local operator config. They define mounts, `workdir`, and optional allowed/default agents.

One useful pattern is to reuse the same workspace with different agent classes:

- `jackin load chainargos/frontend-engineer big-monorepo` for UI work
- `jackin load chainargos/backend-engineer big-monorepo` for API or database work

Another pattern is the opposite: reuse one agent class across many workspaces when the tooling stays the same but the projects differ.
```

- [ ] **Step 3: Verify the README uses the new role-persona examples**

Run: `rg -n 'chainargos/the-architect|agent-brown' README.md`

Expected: no matches

- [ ] **Step 4: Commit the README rewrite**

```bash
git add README.md
git commit -m "docs: sharpen README mental model"
```

### Task 2: Rewrite Core Concepts For Focused Environments

**Files:**
- Modify: `docs/src/content/docs/getting-started/concepts.mdx`

- [ ] **Step 1: Update the namespaced agent example list to use role-persona names**

Replace this bullet:

```mdx
- `chainargos/the-oracle` — a namespaced agent from the chainargos organization
```

with this bullet:

```mdx
- `chainargos/backend-engineer` — a namespaced agent for backend work in the chainargos organization
```

- [ ] **Step 2: Expand the `### Agent classes` section so it explains why multiple classes exist for one project**

Insert the following paragraph immediately after the question list that ends with `Which defaults should this agent start with?`:

```mdx
One project may intentionally use several agent classes. That is not redundancy. It is how you keep the agent's world focused. A frontend-oriented class can carry UI tooling and plugins, while a backend-oriented class can carry server tooling and database clients, even if both point at the same workspace. The smaller and more relevant that environment is, the less out-of-scope context the agent has to inspect.
```

- [ ] **Step 3: Expand the `## Workspaces` section so saved workspaces feel operationally useful**

Replace the paragraph that begins `Think of a workspace as answering:` through the numbered list ending with `Created with jackin workspace create and reusable across sessions.` with the following content:

```mdx
Think of a workspace as answering: "Which project files can this agent see, and where do they appear in the container?"

This is why agent classes and workspaces are separate instead of being one concept.

- agent class = the environment
- workspace = the accessible files

That separation gives you two kinds of control. You can keep the same project boundary while changing the agent's tool profile, or keep the same tool profile while changing the project boundary.

Example: one monorepo workspace can be shared by two different agent classes.

- `chainargos/frontend-engineer` sees the monorepo files and has Node, Playwright, and UI plugins
- `chainargos/backend-engineer` sees the same monorepo files but has Rust, Postgres tools, and backend-specific plugins

That keeps tool and plugin scope small even when the project scope is shared.

Workspaces can be:

1. **Implicit** — the current directory, mounted at the same path. This is what you get with `jackin load agent-smith` from any directory.

2. **Saved** — named configurations stored in `~/.config/jackin/config.toml`. Saved workspaces are useful when you want to reuse the same project boundary across sessions, launch from anywhere, preserve a multi-mount layout, or keep a preferred/default agent attached to that project.
```

- [ ] **Step 4: Verify the concepts page no longer uses the old namespaced example**

Run: `rg -n 'chainargos/the-oracle|chainargos/the-architect|agent-brown' docs/src/content/docs/getting-started/concepts.mdx`

Expected: no matches

- [ ] **Step 5: Commit the concepts rewrite**

```bash
git add docs/src/content/docs/getting-started/concepts.mdx
git commit -m "docs: clarify concepts mental model"
```

### Task 3: Rewrite Why Jackin Around Blast Radius And Context Isolation

**Files:**
- Modify: `docs/src/content/docs/getting-started/why.mdx`

- [ ] **Step 1: Replace the `## Why separate agent classes from workspaces?` section with stronger context-isolation copy**

Replace everything from the heading `## Why separate agent classes from workspaces?` through the paragraph ending `but to use a different agent class where it simply is not installed.` with the following content:

```mdx
## Why separate agent classes from workspaces?

This is the part new users usually need spelled out.

A **workspace** controls file visibility. It says which project paths are mounted into the container.

An **agent class** controls environment and behavior. It says which tools, shells, plugins, and defaults exist in that container image.

Those are different concerns, and separating them is useful for two reasons.

First, it keeps the blast radius clear. The workspace decides which files the agent can reach.

Second, it keeps the agent's working environment focused. The agent class decides which tools, conventions, helper scripts, and plugins are even present. That matters because a giant all-in-one image creates noise. The model can inspect too many tools, too many conventions, and too much irrelevant context before it decides what to do.

Imagine one company with a shared monorepo:

- the frontend team uses a `chainargos/frontend-engineer` agent class with Node, Playwright, design-system utilities, and UI-oriented Claude plugins
- the backend team uses a `chainargos/backend-engineer` agent class with Rust or Go toolchains, database clients, and API-oriented plugins
- both teams can still point those agent classes at the same monorepo workspace when needed

That split keeps the visible project files and the installed toolchain independently controllable.

In practice, this often produces better results than one kitchen-sink agent image. A narrower environment gives the agent fewer irrelevant cues and makes it more likely that the tools and plugins it discovers actually match the task.

It also helps with Claude Code plugins. A tool like Superpowers can be extremely useful, but it brings hooks and behavior that you may not want in every task. The most reliable way to keep it out of scope is not to "mostly disable" it, but to use a different agent class where it simply is not installed.
```

- [ ] **Step 2: Tighten the `## The solution` section to mention both host isolation and focused environments**

Insert this paragraph immediately after `Think of it this way: you're not restricting the agent's capabilities. You're restricting its blast radius.`:

```mdx
Jackin also helps you restrict the agent's *context*. You are not forced to put every language toolchain, helper script, and Claude plugin into one universal environment. You can build smaller agent classes for specific kinds of work and point them at the same workspace when needed.
```

- [ ] **Step 3: Verify the page includes the role-persona examples**

Run: `rg -n 'frontend-engineer|backend-engineer' docs/src/content/docs/getting-started/why.mdx`

Expected: matches for both names

- [ ] **Step 4: Commit the why-page rewrite**

```bash
git add docs/src/content/docs/getting-started/why.mdx
git commit -m "docs: emphasize focused agent environments"
```

### Task 4: Rewrite Workspaces Around Reuse And Predictability

**Files:**
- Modify: `docs/src/content/docs/guides/workspaces.mdx`

- [ ] **Step 1: Expand the introduction so workspaces read as reusable project boundaries**

Replace the block from `A workspace is a saved configuration` through `That means one workspace can be reused with multiple agent classes when the file access should stay the same but the runtime profile should differ.` with the following content:

```mdx
A workspace is a saved configuration that tells jackin how to mount your project directories into an agent container. Instead of typing long mount paths every time, you save a project boundary once and reference it by name.

A workspace is **not** the same thing as an agent class:

- **workspace** = which project files are available
- **agent class** = which tools, plugins, and defaults are installed

That means one workspace can be reused with multiple agent classes when the file access should stay the same but the runtime profile should differ.

In practice, that is one of the main reasons workspaces exist. They let you preserve the same file boundary while swapping in the agent class that best matches the work.
```

- [ ] **Step 2: Add a `## Why save a workspace?` section after `## The current directory workspace`**

Insert this section immediately after the paragraph ending `The agent sees the exact same directory layout you do.`:

```mdx
## Why save a workspace?

Loading the current directory is perfect when you are already standing in the project and only need a simple one-directory mount.

Saved workspaces become useful when you want that setup to be reusable and predictable.

They let you:

- name a project boundary once and launch it from anywhere
- keep extra mounts consistent across sessions
- reuse the same project boundary with different specialized agent classes
- let `jackin launch` auto-detect and preselect the project
- set a default agent or restrict which agent classes may use the workspace

If you regularly work on the same project, a saved workspace turns your mount layout into durable operator configuration instead of something you rebuild from memory each time.
```

- [ ] **Step 3: Add a same-workspace multi-agent example in the `## Saving workspaces` section**

Insert this paragraph immediately after the `jackin load the-architect my-app` example block:

```mdx
This is the key pattern to keep in mind: the workspace stays the same because the project boundary stays the same, but the agent class changes because the job changes. You might use `chainargos/frontend-engineer` for UI work in the morning and `chainargos/backend-engineer` for API work in the afternoon, both against the same saved workspace.
```

- [ ] **Step 4: Tighten the `## Workspace auto-detection` section to connect it back to saved-workspace value**

Replace the existing paragraph under `## Workspace auto-detection` with this paragraph:

```mdx
When you run `jackin launch`, the TUI launcher checks if your current directory matches any saved workspace's `workdir`. If it does, that workspace is preselected — saving you a selection step and reinforcing the value of naming project boundaries once. You can always override this by choosing a different workspace or "Current directory."
```

- [ ] **Step 5: Verify the workspaces page mentions both predictability and multi-agent reuse**

Run: `rg -n 'predictable|frontend-engineer|backend-engineer|project boundary' docs/src/content/docs/guides/workspaces.mdx`

Expected: matches for all of those ideas

- [ ] **Step 6: Commit the workspace guide rewrite**

```bash
git add docs/src/content/docs/guides/workspaces.mdx
git commit -m "docs: explain why saved workspaces matter"
```

### Task 5: Rewrite Agent Repos Around Role-Specific Environments

**Files:**
- Modify: `docs/src/content/docs/guides/agent-repos.mdx`

- [ ] **Step 1: Rewrite the opening explanation so agent repos are explicitly role-specific environments**

Insert this paragraph immediately after `When you run jackin load agent-smith, jackin looks for a GitHub repo named jackin-agent-smith and builds a container from it.`:

```mdx
An agent repo is where you decide what kind of worker you are creating. In most teams, that means creating role-specific environments rather than one universal image. A frontend-oriented agent repo can install browser tooling and UI-focused plugins, while a backend-oriented repo can install server toolchains and database clients.
```

- [ ] **Step 2: Replace the namespaced agent table with role-persona examples**

Replace the `### Namespaced agents` table with this table:

```mdx
### Namespaced agents

Organizations can create agents under their namespace:

| Class name | GitHub repo | Load command |
|---|---|---|
| `chainargos/frontend-engineer` | `chainargos/jackin-frontend-engineer` | `jackin load chainargos/frontend-engineer` |
| `chainargos/backend-engineer` | `chainargos/jackin-backend-engineer` | `jackin load chainargos/backend-engineer` |
| `chainargos/security-reviewer` | `chainargos/jackin-security-reviewer` | `jackin load chainargos/security-reviewer` |
```

- [ ] **Step 3: Add a `## When should you create another agent class?` section before `## Agent identity`**

Insert this section immediately before `## Agent identity`:

```mdx
## When should you create another agent class?

Create another agent class when the work needs a meaningfully different environment.

Common reasons include:

- frontend work needs Node, browser tooling, and UI-focused plugins
- backend work needs Rust or Go toolchains, API clients, and database tools
- infra work needs Terraform, cloud CLIs, and deployment helpers
- security review work should have a smaller, audit-focused tool and plugin set
- docs work may need writing or publishing tools without bringing in unrelated build systems

The goal is not to create lots of arbitrary images. The goal is to avoid one kitchen-sink image that exposes every tool and plugin to every task.
```

- [ ] **Step 4: Update the example agent repo identity to match the new naming style**

Replace this manifest snippet:

```toml
[identity]
name = "Node Agent"
```

with this snippet:

```toml
[identity]
name = "Frontend Engineer"
```

- [ ] **Step 5: Verify the page no longer uses the confusing third-party names**

Run: `rg -n 'chainargos/the-architect|agent-brown' docs/src/content/docs/guides/agent-repos.mdx`

Expected: no matches

- [ ] **Step 6: Commit the agent-repos rewrite**

```bash
git add docs/src/content/docs/guides/agent-repos.mdx
git commit -m "docs: use role-based agent repo examples"
```

### Task 6: Verify Docs Consistency And Build Cleanly

**Files:**
- Modify: `README.md`
- Modify: `docs/src/content/docs/getting-started/concepts.mdx`
- Modify: `docs/src/content/docs/getting-started/why.mdx`
- Modify: `docs/src/content/docs/guides/workspaces.mdx`
- Modify: `docs/src/content/docs/guides/agent-repos.mdx`

- [ ] **Step 1: Run a global grep for old confusing example names**

Run: `rg -n 'chainargos/the-architect|chainargos/the-oracle|agent-brown' README.md docs/src/content/docs`

Expected: no matches

- [ ] **Step 2: Review the final diff for the five planned files**

Run: `git diff -- README.md docs/src/content/docs/getting-started/concepts.mdx docs/src/content/docs/getting-started/why.mdx docs/src/content/docs/guides/workspaces.mdx docs/src/content/docs/guides/agent-repos.mdx`

Expected: the diff consistently emphasizes focused environments, saved workspace value, and role-persona example names

- [ ] **Step 3: Build the docs site**

Run: `bun run build`

Working directory: `docs/`

Expected: Astro/Starlight build completes successfully with no content or MDX errors

- [ ] **Step 4: Commit the final polish and verification state**

```bash
git add README.md \
  docs/src/content/docs/getting-started/concepts.mdx \
  docs/src/content/docs/getting-started/why.mdx \
  docs/src/content/docs/guides/workspaces.mdx \
  docs/src/content/docs/guides/agent-repos.mdx
git commit -m "docs: improve new-user mental model"
```
