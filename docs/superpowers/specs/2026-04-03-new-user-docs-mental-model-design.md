# New-User Docs Mental Model Design

Date: 2026-04-03

## Background

The current docs already explain the major parts of jackin well enough for an experienced reader, but they still lean too abstract in places for a new user trying to answer two practical questions:

- Why would I use multiple agent classes for the same project?
- Why should I save a workspace instead of just loading the current directory each time?

The docs also currently blur the difference between project-specific named agents such as `the-architect` and third-party example agents. That makes the examples less clear than they should be.

## Audience

This rewrite targets readers who are already comfortable with Docker and AI coding agents, but who are new to jackin's specific model.

They do not need container basics explained from first principles. They do need jackin's boundaries, terminology, and practical workflow made obvious very quickly.

## Goals

- Make the first-time mental model easier to understand.
- Explain that one project can intentionally use multiple specialized agent classes.
- Explain that this specialization improves results by creating a smaller, more relevant environment for the agent.
- Explain that workspaces isolate file access, while agent classes isolate tools, plugins, defaults, and behavior.
- Explain the practical value of saved workspaces as reusable project boundaries.
- Standardize examples so third-party or company examples use role-persona names instead of reusing jackin's own named agents.

## Core Message

The docs should reinforce the same message everywhere:

- A workspace isolates files.
- An agent class isolates tools, plugins, defaults, and behavior.
- An agent instance is one running container created from an agent class and attached to one workspace.
- Using multiple agent classes for the same project is a feature, not overhead, because it gives the agent a smaller and more relevant ecosystem.
- Saved workspaces matter because they make a project boundary reusable, predictable, and easy to relaunch.

## Naming Strategy

Project-owned examples should stay project-owned:

- `agent-smith`
- `the-architect`

Company examples should use role-persona names that sound like agents but still reveal their purpose immediately:

- `chainargos/frontend-engineer`
- `chainargos/backend-engineer`
- `chainargos/infra-operator`
- `chainargos/security-reviewer`

This avoids confusion with jackin's own named agent classes while still teaching that an agent class is a purpose-built environment for a kind of work.

## Documentation Strategy

The rewrite should be practical-first for new users, but still keep concise definitions near the top of each page.

The recommended teaching order is:

1. Show the day-one workflow and why it exists.
2. Define the concepts in short, precise language.
3. Show how the concepts fit together in realistic examples.

## Planned File Changes

### README.md

Update the opening mental-model language so the first practical takeaway is that one project can use multiple specialized agent classes, and that this is desirable because each agent should operate in a focused environment.

Add an example that reuses one workspace with two agent classes such as `chainargos/frontend-engineer` and `chainargos/backend-engineer`.

Tighten the workspace explanation so saved workspaces are described as reusable project boundaries, not just command shortcuts.

### docs/src/content/docs/getting-started/concepts.mdx

Keep the definitions concise, but expand the explanation of why agent classes and workspaces are separate.

Replace third-party namespaced examples such as `chainargos/the-architect` with role-persona examples.

Add a short scenario showing the same workspace reused by multiple specialized agent classes to keep tool and plugin scope focused.

### docs/src/content/docs/getting-started/why.mdx

Strengthen the argument that jackin improves more than host isolation.

Explain that specialized agent classes also improve context isolation: a kitchen-sink image gives the model too many tools, plugins, conventions, and cues, which can produce worse results than a smaller environment shaped around the task.

Keep the host-isolation story, but raise the importance of the focused-environment story.

### docs/src/content/docs/guides/workspaces.mdx

Add a stronger section near the top explaining why saved workspaces exist.

Explain their practical benefits:

- name a project boundary once
- relaunch from anywhere
- keep mount layout predictable
- let `jackin launch` auto-detect and preselect the project
- optionally restrict or default allowed agent classes

Make it clear that a saved workspace is how an operator preserves a file-access pattern and then reuses it with different agent classes over time.

### docs/src/content/docs/guides/agent-repos.mdx

Frame agent repos as the place where an operator or team builds a role-specific environment for a class of work.

Use role-persona examples consistently.

Add guidance on when to create another agent class, such as when frontend, backend, infra, docs, or security work need different tools or plugins.

## Content Tone

The language should assume the reader understands Docker and agent tooling already.

It should avoid over-explaining basic container mechanics and instead focus on operational clarity:

- what problem the separation solves
- what belongs in each concept
- how an experienced user should think about using jackin in practice

## Out Of Scope

- Changing runtime behavior or command semantics
- Reorganizing the entire docs information architecture
- Documenting unsupported runtimes as if they are already available

## Success Criteria

The rewrite is successful if a new reader can quickly understand all of the following:

- why jackin separates agent classes from workspaces
- why one project may intentionally use multiple agent classes
- why a narrower environment can produce better agent results than a single all-in-one image
- why saving a workspace is useful even when loading the current directory already works
- how jackin's example naming maps to real-world team usage
