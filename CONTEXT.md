# Glossary

These are the terms that jackin uses, with the meaning that `rewrite/DESIGN.md` gives each one. The decision ID after each meaning names where the design defines it.

## operator

The person who runs jackin and owns the gateway. The operator registers the gateway, chooses its compute driver, and starts any Docker engine that the Docker capability uses. (D13, D24)

## role

A repository that holds a `Dockerfile` whose final stage starts `FROM construct`, a `jackin.role.toml` manifest, and a native OpenShell `policy.yaml`. Jackin builds one image from each role. (D05, D11)

## workspace

A configuration entry for a project that the operator works on. A workspace can set the `docker` flag and can bind one account for each provider. Each sandbox belongs to one workspace. (D07, D24, D26)

## provider

An AI coding service that has accounts and usage, such as Codex, Claude, or OpenRouter. A provider is not an OpenShell provider. (D26)

## agent

A CLI that runs in a sandbox. Codex is the only agent. An agent uses accounts of its provider. An agent is not a provider. (D04, D26)

## account

A named credential for one provider. The operator can add an account for any provider. A Codex account also gets an OpenShell provider. Other accounts stay on the host for usage only. (D10, D26)

## default account

The account that jackin uses for a provider when no flag or binding names one. Each provider has at most one. (D26)

## binding

The link between an account and a scope. A workspace binding holds one account for each provider. A role binding holds the default account of the role for each agent. At launch, the `--account` flag wins, then the workspace binding, the role binding, the default account, and the only account of the provider. (D26)

## usage

The limit data that each provider reports for each account, such as the 5-hour window, the weekly window, the percentage used, and the reset time. `jackin usage` shows it. Usage runs on the host and never sends a credential into a sandbox. (D25)

## gateway

The OpenShell service that creates and runs sandboxes and holds credentials. The operator installs, starts, and registers it. Jackin never manages it. (D13, D10)

## sandbox

The OpenShell environment where one launch runs one agent. Jackin creates one sandbox for each launch and finds it again by its labels. OpenShell enforces the isolation. (D01, D07)

## OpenShell provider

The OpenShell credential object that holds the credential of one account and agent pair, named `jackin-<account>-codex`. The gateway keeps the real credential and gives the sandbox only a placeholder. It is not a provider in the sense of an AI coding service. (D10)

## policy

The native OpenShell policy in a role's `policy.yaml`. Jackin sends it to OpenShell without change and adds only the rules for the OpenShell provider and the Docker capability. (D12)

## image

The container image that jackin builds for a role with Docker BuildKit and delivers to the gateway. Its identity is the Docker image ID, and its tag is the cache key. (D05, D23)

## construct

The base image that jackin builds and owns. The final stage of every role `Dockerfile` starts `FROM construct`. (D06)

## Docker capability

An opt-in setting that gives the agent in a sandbox access to an external Docker engine. A role enables it with `[docker] enabled = true`, and a workspace enables it with `docker = true`. It is disabled by default. (D24)

## launch

The operation of `jackin load` that builds the image when needed, then creates or restores a sandbox, uploads the project, runs the hooks, and starts the agent. A launch has a launch ID, and a retry with the same ID continues from the observed state. (D07, D09, D22)

## attach

The operation that opens the main terminal of the agent in a sandbox. Detaching leaves the agent running, and `jackin attach --restart` starts the agent again after it exits. (D09)

## shell

A new interactive process in a sandbox, opened by `jackin shell`. It has no detach and no replay, and it ends when the stream closes. (D09)

## export

The operation that fetches each branch of a sandbox into the host repository as `refs/heads/jackin/<sandbox-name>/<branch>`, through a git bundle. It never writes to the host working tree. (D08)

## purge

The operation that deletes a sandbox. It refuses when the work is not exported, unless the operator passes `--force`. (D22)
