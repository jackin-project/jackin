# Fish shell config for Capsule-managed containers.









set -gx PATH "$HOME/.local/share/mise/shims" "$HOME/.local/bin" $PATH



if type -q starship
    starship init fish | source
end







function fish_title
    echo (whoami)'@'(prompt_hostname)':'(prompt_pwd)
end






function _jackin_emit_osc7 --on-event fish_prompt
    printf '\e]7;file://%s%s\e\\' (hostname) (string escape --style=url -- "$PWD")
end


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
