# Selectable Sandbox Backends: DinD and MicroVM

**Status**: Deferred - requires a dedicated design pass before implementation

## Problem

jackin currently has exactly one runtime model:

- build the agent image on the host Docker engine
- create a per-agent Docker network
- start a privileged `docker:dind` sidecar
- start the agent container on that network
- point the agent at the sidecar with `DOCKER_HOST=tcp://...:2375`

That model is coherent, but it leaves a product gap against microVM-based tools such as Docker Sandboxes. Operators who want stronger local isolation cannot choose a hypervisor-backed runtime, and operators who are happy with Docker cannot explicitly select and manage the current mode as a first-class feature.

The requested feature is a single product-level capability:

- the operator should be able to choose how an agent is loaded into a workspace
- the two supported approaches should be `dind` and `microvm`
- the same agent/workspace concepts should continue to work in both modes

## Why It Matters

- `dind` is the shortest path and already matches the current architecture
- `microvm` is the path that narrows the gap with Docker Sandboxes
- a user-visible mode switch makes the isolation tradeoff explicit instead of implicit
- the project can keep its current Docker-first ergonomics while adding a stronger boundary where the host supports it

## Current State

Today the runtime is tightly coupled to Docker and DinD:

- `src/runtime.rs` builds the image, starts the network, starts `docker:dind`, and launches the agent container
- `src/docker.rs` shells out to the Docker CLI for all lifecycle operations
- `src/instance.rs` derives persisted state paths from Docker-style container names
- `docker/construct/Dockerfile` installs Docker CLI and Compose in the agent environment
- `docker/runtime/entrypoint.sh` launches Claude inside a Docker-oriented runtime contract

Important current assumptions:

- the agent can talk to a Docker-compatible daemon from inside its sandbox
- workspace access is delivered through direct host bind mounts
- agent state persistence is separate from runtime filesystem persistence
- runtime attach/eject/list behavior is discovered from Docker container state

These assumptions are reasonable for `dind`, but they are not backend-neutral.

## Goal

Add a first-class sandbox mode abstraction with these operator-visible outcomes:

1. The operator can choose `dind`, `microvm`, or `auto`.
2. Existing agent repos remain usable without forcing every agent author to redesign their Dockerfile.
3. Workspaces, mounts, last-used agent tracking, and persisted Claude/GitHub state continue to work in both modes.
4. Unsupported hosts fail clearly or fall back intentionally rather than half-working.

## Non-Goals

- Replacing Docker-based image builds in the first phase
- Designing a cloud sandbox product
- Guaranteeing identical low-level runtime behavior across all providers
- Claiming that a hardened container runtime is equivalent to a microVM

## User Experience Requirements

The feature should be visible in three places:

### CLI

Examples:

```sh
jackin load agent-smith --sandbox-mode dind
jackin load agent-smith --sandbox-mode microvm
jackin load agent-smith --sandbox-mode auto
```

### Config

Suggested global shape:

```toml
[runtime]
default_mode = "auto"
microvm_provider = "auto"
persist_engine_state = false
```

Suggested workspace override shape:

```toml
[workspaces.big-monorepo.runtime]
mode = "microvm"
```

### Runtime Output

The launch summary should tell the operator which backend is being used, for example:

- `sandbox mode: dind`
- `sandbox mode: microvm (kata)`
- `sandbox mode: microvm (apple)`

## High-Level Design Options

### Option 1: DinD Only, Hardened and Explicit

This makes the current architecture first-class without adding a second backend yet.

Pros:

- smallest implementation
- low migration risk
- immediately improves current security posture if TLS/rootless work is added

Cons:

- does not solve the Docker Sandboxes comparison gap
- still shares the host kernel

### Option 2: Two User Modes, One Generic MicroVM Abstraction

Expose `dind` and `microvm` at the product level. Under `microvm`, pick an implementation per platform.

Recommended provider strategy:

- Linux with KVM: `kata-containers`
- macOS Apple Silicon on modern macOS: Apple Containerization
- unsupported host: explicit fallback to `dind` or hard failure

Pros:

- matches the user-facing requirement directly
- keeps the UX stable while allowing different providers underneath
- avoids making `kata` itself the top-level product contract

Cons:

- larger design surface
- requires backend-neutral lifecycle management

### Option 3: Provider-Specific User Modes

Expose `dind`, `kata`, `apple`, and later other providers.

Pros:

- transparent about implementation

Cons:

- leaks infrastructure choices into the operator UX
- makes cross-platform defaults and docs more complex
- encourages provider-specific branching too early

## Recommendation

Choose Option 2.

Make `microvm` the user-facing mode and treat the actual provider as an internal decision. This keeps the feature aligned with the isolation model the operator cares about while preserving room for different host-specific implementations.

## Critical Compatibility Question: Will Existing Agent Dockerfiles Work?

Mostly yes for the agent images themselves.

The current agent repo contract only requires that the final stage use the construct image. The existing agent Dockerfiles are standard OCI-style environment definitions and should remain valid inputs for both `dind` and `microvm` modes.

The real compatibility issue is not the Dockerfiles. It is the runtime contract around them:

- current agents expect Docker CLI tooling in the sandbox
- current launch flow injects `DOCKER_HOST`
- current runtime assumes direct bind mounts and `docker attach`

So the correct conclusion is:

- agent Dockerfiles are reusable
- the runtime backend is what must change
- the product should preserve a Docker-compatible inner engine where possible so current agents continue to function

## Backend-Specific Design

### DinD Mode

`dind` mode should formalize and harden the current design.

Required improvements:

- move the current runtime into a named backend module
- stop using unauthenticated plain TCP DinD
- prefer TLS or a private socket transport
- add a backend-neutral instance registry instead of inferring all state from Docker names
- only persist last-used agent metadata on successful launch

Possible hardening layers to evaluate:

- `docker:dind-rootless`
- `sysbox-runc` on Linux hosts

`sysbox` is especially relevant as a Linux-only improvement path because it can support Docker-in-Docker without the usual privileged container model. It is not a microVM and should not be presented as one.

### MicroVM Mode

`microvm` mode should provide a stronger isolation boundary while keeping the same high-level operator workflow.

Required properties:

- private engine inside the VM boundary
- reusable agent image or equivalent runnable artifact
- workspace access delivered into the VM
- persisted Claude/GitHub/plugin state mounted or synchronized into the VM

There are two realistic local providers:

## MicroVM Implementation Primer

The most important implementation question is not "can jackin use a VM?" It is:

- what should run inside the VM
- what should stay on the host
- what parts of the current Docker contract need to survive inside the sandbox

For this project, the most practical mental model is:

- keep using Dockerfiles and OCI images as the packaging format
- change the isolation boundary from host containers to a VM
- provide a private Docker-compatible engine inside that VM when the agent needs Docker workflows

### Build / Run Matrix

The build path and the runtime path are separate decisions.

| Build path | Run path | Viable for jackin? | Notes |
|---|---|---|---|
| Host Docker | Host container | Yes | Current implementation |
| Host Docker | MicroVM | Yes | Best first prototype for `microvm` |
| VM-local Docker | MicroVM | Yes | Closer to Docker Sandboxes |
| Host Docker | Remote Linux microVM | Yes | Useful if local host cannot provide a microVM backend |

The strongest short-term recommendation is:

- keep host-side image builds in the first phase
- run the resulting agent image inside a microVM
- provide the private engine inside the VM boundary, not on the host

That gives a meaningful security improvement without redesigning the whole build pipeline on day one.

### What Should Run Inside The VM

The cleanest microVM model for jackin is not "replace Docker with a VM". It is:

- run the agent inside the VM
- run a private Docker-compatible engine inside the same VM
- expose the workspace into the VM
- mount or synchronize the persisted Claude/GitHub/plugin state into the VM

That means the VM should contain at least:

- the agent runtime environment
- Claude entrypoint support
- a Docker-compatible daemon such as `dockerd` or possibly `containerd` + compatibility tooling
- guest-local writable storage for engine state

This is the closest match to Docker Sandboxes' architecture while still reusing jackin's current agent image model.

### Why Existing Agent Dockerfiles Still Matter

Agent Dockerfiles are still useful because they define the userland environment:

- language runtimes
- development tools
- shell environment
- plugins and conventions

So the likely design is not "replace Dockerfiles with VM images." It is:

- Dockerfile builds agent filesystem/tooling layer
- microVM provider decides how to execute that layer safely

In other words, the Dockerfile remains the environment definition, while the microVM becomes the runtime boundary.

#### Linux Provider: Kata Containers

Why it fits:

- integrates with containerd for VM-backed OCI workloads
- already targets secure container-style workflows
- is heavily Rust-based today, including `runtime-rs`, `agent`, and `dragonball`

Why it is not a drop-in replacement:

- Kata integrates naturally through containerd, not the Docker socket API that jackin currently assumes
- Docker-in-Kata has an official storage caveat: `virtio-fs` cannot be used as the OverlayFS upper layer in the normal way for DinD workloads

Official workaround categories to account for:

- tmpfs-backed `/var/lib/docker` for small ephemeral workloads
- loop-mounted ext4 disk at `/var/lib/docker` for better performance
- custom guest kernel work if trying to avoid those workarounds

Practical implication:

- a Linux Kata backend is workable, but not by simply swapping out the current sidecar without other runtime changes

##### Likely Linux Implementation Shape

The most practical Linux design is:

1. jackin continues to build the derived agent image with host Docker
2. the image is made available to `containerd`
3. jackin launches the workload with Kata's runtime handler such as `io.containerd.kata.v2`
4. inside the VM-backed sandbox, the agent process runs with a private Docker-compatible engine available locally

Important detail:

- Kata gives jackin the VM-backed OCI runtime
- it does not automatically solve the "private Docker daemon inside the sandbox" requirement by itself

So Linux `microvm` mode still needs an internal execution strategy such as:

- `dockerd` running inside the Kata guest
- or a lighter `containerd`-based inner engine if Docker API compatibility can be preserved well enough

##### Recommended Linux Prototype

For the first Linux spike, prefer:

- host Docker build
- Kata-backed run
- VM-local `dockerd`
- guest-local `/var/lib/docker`

Do not try to share `/var/lib/docker` over the same shared filesystem used for the workspace.

##### Linux Integration Notes

What jackin would likely need to do on Linux:

- detect KVM availability and a usable Kata/containerd installation
- register or require a configured containerd runtime handler for Kata
- add a backend launcher that shells out to `ctr` / `nerdctl` first, before considering a full client-library implementation
- create an attach/reconnect path that does not assume `docker attach`
- treat the sandbox as an instance tracked in jackin state, not inferred from Docker names

##### Why Kata Is Still Attractive

Even with the extra work, Kata remains attractive because it removes the need for jackin to directly orchestrate a hypervisor, guest kernel, vsock, and VM boot sequence. It is the fastest path to a real Linux microVM backend that still consumes OCI images.

#### macOS Provider: Apple Containerization

Why it fits:

- native lightweight VM model on Apple Silicon
- designed specifically for running Linux containers in isolated per-container VMs on macOS
- OCI-compatible model aligns better with existing agent images than trying to force Kata onto macOS

Practical implication:

- if `microvm` is meant to work well on Mac laptops, Apple Containerization is the right local provider, not Kata itself

##### What Apple Containerization Actually Provides

Apple's `containerization` framework and the `container` CLI implement a VM-backed OCI container runtime on macOS.

Important properties relevant to jackin:

- each Linux container runs in its own lightweight VM
- the implementation is optimized for Apple Silicon
- it consumes OCI-compatible images
- it can build from Dockerfiles, but can also run already-built OCI images
- the public tooling includes a system service and CLI (`container system start`, `container run`, etc.)

This means Apple Containerization is not just a documentation curiosity. It is a credible provider for a real local `microvm` mode on macOS.

##### Why Apple Containerization Matters For This Project

Without it, a microVM story for macOS would likely mean one of these awkward options:

- ask macOS users to install and manage a Linux VM manually
- hide a Linux VM behind another local tool
- make microVM mode effectively Linux-only

Apple Containerization avoids that and gives jackin a host-native macOS path.

##### Likely macOS Implementation Shapes

There are two realistic ways to integrate Apple Containerization into jackin.

###### Option A: Shell out to the `container` CLI first

This is the easiest first implementation.

Shape:

1. jackin builds the derived agent image with host Docker
2. jackin ensures the Apple `container` system service is running
3. jackin uses `container` commands to create/start the sandboxed workload from the OCI image
4. jackin manages the sandbox instance in its own registry and state files

Pros:

- lowest implementation cost
- fast way to test whether the provider is good enough
- avoids adding Swift framework embedding work immediately

Cons:

- one more external CLI dependency
- less control over detailed lifecycle behavior
- output parsing and attach semantics may be awkward

###### Option B: Use a dedicated helper around the `containerization` framework

This is the cleaner long-term path.

Shape:

- keep jackin as the main Rust CLI
- add a small macOS-only helper binary or service written in Swift
- that helper owns the Apple Containerization API calls
- jackin talks to it through a stable command or RPC boundary

Pros:

- much more control over lifecycle, attach, state, and path handling
- avoids overloading the public `container` CLI as an implementation detail forever
- easier place to model jackin-specific behavior later

Cons:

- more engineering work up front
- introduces a second implementation language in the project

##### Recommended macOS Path

For this project, the recommended sequence is:

1. prototype with the `container` CLI
2. validate agent image compatibility, path sharing, attach/reconnect, and VM-local Docker workflows
3. if the provider is a keeper, replace the CLI shell-out with a small Swift helper later

##### macOS Constraints To Capture Early

- Apple Silicon should be treated as the primary supported target
- modern macOS is required; older macOS should fall back to `dind`
- the provider should be treated as optional and capability-detected, not assumed
- `microvm` mode on macOS must have a documented install/setup path for Apple's `container` tooling

##### What Needs Special Research On macOS

Before implementation, the next design should validate:

- how best to map a host-built agent OCI image into the Apple runtime
- whether the workspace can be exposed at the same absolute path or needs a managed guest path
- how to provide a VM-local Docker-compatible engine for the agent
- how attach/reconnect should work for an interactive Claude session
- how sandbox-local state and jackin-managed persisted state should be combined

##### Why This Is The Best macOS Story

If jackin wants a serious answer to Docker Sandboxes on macOS, Apple Containerization is the closest natural fit because it already shares the same core product idea:

- OCI-compatible workloads
- lightweight VM per sandboxed Linux environment
- strong local isolation on a Mac developer machine

That makes it a much better match than trying to force Kata into a host environment it does not naturally target.

### Direct Hypervisor Paths To Defer

It is possible to imagine jackin owning more of the VM runtime stack directly using technologies like Firecracker or Cloud Hypervisor.

This is not the recommended first implementation path.

Why to defer it:

- it pushes jackin toward becoming a sandbox runtime product rather than an operator CLI
- it would require jackin to own more low-level VM orchestration concerns directly
- Kata and Apple Containerization already solve a meaningful portion of that work in their respective environments

This should remain future research unless the provider-based design proves too limiting.

## Comparison To Docker Sandboxes

Docker Sandboxes combines three important ideas:

- microVM isolation boundary
- private Docker daemon inside the sandbox
- host-side orchestration and policy layer around the sandbox

The closest jackin implementation would therefore be:

- keep current agent image build flow at first
- run the agent inside a microVM
- provide the Docker-compatible engine inside that VM, not as a host-side sidecar
- track sandbox instances independently of host Docker container naming

That still differs from Docker Sandboxes in one important way during the first phase:

- initial jackin `microvm` mode will likely still build images on the host, while Docker Sandboxes keeps the main sandbox execution model fully inside the VM boundary

That is acceptable as an incremental architecture, but it should be described honestly.

## Provider Implementation Matrix

| Topic | Linux `microvm` | macOS `microvm` |
|---|---|---|
| Recommended provider | Kata Containers | Apple Containerization |
| Best first integration style | shell out to `ctr` / `nerdctl` / configured provider CLI | shell out to `container` CLI |
| Longer-term integration style | dedicated Rust backend using structured runtime integration | Swift helper around `containerization` APIs |
| Agent image source | host-built OCI image | host-built OCI image |
| Inner engine recommendation | VM-local `dockerd` with guest-local storage | VM-local `dockerd` or equivalent |
| Workspace model | shared guest path, not raw host bind-mount assumptions | same-path if possible, otherwise explicit guest path model |
| Main blocker | Docker-in-Kata storage semantics | provider integration and lifecycle control |
| Best first milestone | experimental Linux `microvm` mode | experimental macOS `microvm` mode via `container` |

## Key Architecture Changes Required

### 1. Backend Abstraction

The current `load_agent` flow needs a backend seam.

Suggested responsibilities:

- repo resolution and validation
- image build
- persisted agent-state preparation
- backend launch/attach/list/eject

Suggested internal shape:

- `src/backend/mod.rs`
- `src/backend/dind.rs`
- `src/backend/microvm.rs`

### 2. Instance Registry

The project should stop treating Docker names as the source of truth.

Persist per-instance metadata such as:

- stable instance ID
- agent selector
- backend kind
- provider kind
- workspace label
- display name
- backend-specific handle

This is required for mixed backends because VM instances will not naturally map to current Docker naming conventions.

### 3. Workspace Materialization Model

Today workspaces assume direct bind mounts. A VM backend may need a different transport.

The design should introduce a backend-neutral concept such as:

- direct bind mount
- shared filesystem passthrough
- synchronized directory

This does not require changing the user-facing workspace model immediately, but it does require changing the internal representation.

### 4. Runtime Capability Contract

The product should define what an agent runtime is expected to provide.

At minimum:

- shell execution
- Claude entrypoint support
- Git identity configuration
- plugin bootstrap
- Docker-compatible engine access inside the sandbox, if the backend promises Docker workflows

This prevents future backends from being "supported" in name but missing core behavior.

## Platform Support Matrix

### DinD

- macOS: supported now through existing Docker environments
- Linux: supported now
- Windows/WSL2: possible but still secondary

### MicroVM

- Linux + KVM + containerd: target `kata`
- macOS Apple Silicon + supported macOS: target Apple Containerization
- Linux without KVM: fallback or fail
- older or unsupported macOS: fallback or fail

## Suggested Operator Scenarios

### Scenario 1: Current Behavior, Explicitly Named

The operator uses:

```sh
jackin load agent-smith --sandbox-mode dind
```

Outcome:

- current behavior preserved
- runtime is clearly labeled as container-based

### Scenario 2: Stronger Isolation on Linux Workstation

The operator uses:

```sh
jackin load the-architect --sandbox-mode microvm
```

On a Linux host with KVM, the product selects Kata.

Outcome:

- agent runs inside a VM-backed sandbox
- private engine remains inside the VM boundary

### Scenario 3: Stronger Isolation on Mac

The operator uses:

```sh
jackin load the-architect --sandbox-mode microvm
```

On a supported Apple Silicon Mac, the product selects Apple Containerization.

Outcome:

- operator gets a local microVM-style sandbox instead of container-only isolation

### Scenario 4: Auto Fallback

The operator uses:

```sh
jackin load agent-smith --sandbox-mode auto
```

Outcome:

- use `microvm` when a supported provider is available
- otherwise fall back to `dind` with a clear message

## Implementation Phases

### Phase 1: Prepare the Codebase

- extract a backend interface from the current runtime flow
- add backend-neutral instance persistence
- fix current launch metadata persistence bugs
- harden DinD transport and cleanup behavior

### Phase 2: Ship Explicit DinD Mode

- introduce CLI/config support for `--sandbox-mode`
- map `dind` to current behavior
- keep `microvm` hidden or experimental until at least one provider works end to end

### Phase 3: Linux MicroVM Prototype

- add an experimental Kata-backed `microvm` provider
- validate current agent images under VM-backed execution
- validate Docker workflows with the required `/var/lib/docker` workaround
- measure startup time, build performance, and workspace behavior

### Phase 4: macOS MicroVM Provider

- add Apple Containerization provider for `microvm`
- ensure workspace semantics and state persistence remain operator-friendly

### Phase 5: Docs and Product Positioning

- update docs to describe `dind` vs `microvm`
- keep the security model blunt and accurate
- explain host support and fallback behavior clearly

## Risks and Design Traps

- pretending Kata is cross-platform when it is really a Linux/KVM solution
- assuming current DinD sidecar semantics can simply be moved under Kata without storage changes
- tying instance identity to Docker names when multiple backends exist
- supporting a backend that cannot actually satisfy Docker-based agent workflows
- adding a provider-specific UX before the product-level mode abstraction is stable

## Alternatives Worth Mentioning in the Future Design

### Sysbox

Good Linux-only hardening path for `dind` mode.

- better than privileged DinD
- not a microVM
- useful if the product wants a stronger container boundary without VM orchestration

### gVisor

Good defense-in-depth option, but not the best primary answer for this feature.

- stronger than plain containers
- weaker than microVMs
- not the clearest fit when nested Docker workflows are a core requirement

## What the Next Agent Should Produce

The next design pass should turn this TODO into a full implementation design with:

- exact CLI and config schema changes
- exact backend trait/module structure
- exact instance registry format
- workspace transport model for VM providers
- persistence policy for user state vs engine state
- provider selection rules for Linux/macOS
- rollout plan for experimental vs stable support

## Related Files

- `src/runtime.rs` - current launch, attach, cleanup, and runtime coupling
- `src/docker.rs` - Docker CLI execution model
- `src/instance.rs` - persisted state and Docker-style naming assumptions
- `src/workspace.rs` - workspace mount resolution
- `src/config.rs` - future config surface for sandbox mode
- `src/derived_image.rs` - derived runtime layer generation
- `docker/construct/Dockerfile` - current Docker-oriented construct contract
- `docker/runtime/entrypoint.sh` - current runtime entrypoint behavior
- `docs/src/content/docs/reference/architecture.mdx` - current architecture story
- `docs/src/content/docs/guides/security-model.mdx` - current security boundary statement
- `docs/src/content/docs/guides/comparison.mdx` - comparison with Docker Sandboxes
