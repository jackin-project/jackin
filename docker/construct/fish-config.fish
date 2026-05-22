# Fish shell config for jackin containers.
#
# zsh remains the default login shell for the `agent` user (set in the
# construct Dockerfile via `useradd -s /bin/zsh`); fish is available
# as an alternative the operator can opt into either by running `fish`
# from the default zsh pane or by changing the login shell with
# `sudo chsh -s /usr/bin/fish agent`.

# PATH parity with the zsh setup so `mise`-managed binaries and user-
# local bin entries resolve identically across both shells.
set -gx PATH "$HOME/.local/share/mise/shims" "$HOME/.local/bin" $PATH

# Starship prompt — same prompt surface as zsh so the operator's visual
# context does not change when switching shells.
if type -q starship
    starship init fish | source
end

# OSC 0/2 window title (`user@host:cwd`) on every prompt. The
# jackin-capsule multiplexer reads OSC 0/2 and renders the pane
# border title from it (matches zellij convention). fish ships a
# `fish_title` hook that fires automatically each prompt; the body
# returns the title text and fish wraps it in the correct escape
# sequence for the active terminal.
function fish_title
    echo (whoami)'@'(prompt_hostname)':'(prompt_pwd)
end

# OSC 7 cwd hint (`file://host/path`) on every prompt. fish does not
# emit this natively; the jackin multiplexer uses it as a secondary
# title source and as the "open new pane here" cwd seed. `string
# escape --style=url` performs the same percent-encoding the OSC 7
# spec requires for paths with spaces or non-ASCII bytes.
function _jackin_emit_osc7 --on-event fish_prompt
    printf '\e]7;file://%s%s\e\\' (hostname) (string escape --style=url -- "$PWD")
end

# Security tools (disable with JACKIN_DISABLE_TIRITH=1 / JACKIN_DISABLE_SHELLFIRM=1)
if test "$JACKIN_DISABLE_TIRITH" != 1
    if type -q tirith
        tirith init --shell fish | source
    end
else
    echo "[fish] tirith shell hook disabled (JACKIN_DISABLE_TIRITH=1)"
end
if test "$JACKIN_DISABLE_SHELLFIRM" != 1
    if type -q shellfirm
        shellfirm init fish | source
    end
else
    echo "[fish] shellfirm shell hook disabled (JACKIN_DISABLE_SHELLFIRM=1)"
end
