# jackin-capsule

In-container control plane for jackin❯ role containers.

`jackin-capsule` is copied into derived role images and runs as PID 1 under `/jackin/runtime/jackin-capsule`. It owns the terminal sessions, PTYs, pane layout, status bar, attach socket, runtime setup, and the in-container git trailer hook. The host `jackin` binary starts containers detached and attaches through the Capsule client so the operator sees the multiplexer instead of raw container logs.

The crate is split out from the host CLI because it runs inside Linux role containers and carries dependencies the host should not inherit directly, including Tokio socket handling, PTY control, VT parsing, and raw ANSI rendering.

