//! Debug-log helpers for console TUI event traces.

/// Render a key event for debug logs. Redacts literal text input when the
/// focused widget owns character entry.
pub fn key_debug_name_for_input(
    key: crossterm::event::KeyEvent,
    consumes_letter_input: bool,
) -> String {
    use crossterm::event::{KeyCode, KeyModifiers};
    let has_command_modifier = key
        .modifiers
        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT | KeyModifiers::SUPER);
    let code = match key.code {
        KeyCode::Char(_) if consumes_letter_input && !has_command_modifier => {
            "Char(<redacted>)".to_string()
        }
        KeyCode::Char(ch) => format!("Char({})", ch.escape_default()),
        other => format!("{other:?}"),
    };
    if key.modifiers.is_empty() {
        code
    } else {
        format!("{:?}+{code}", key.modifiers)
    }
}

#[cfg(test)]
mod tests {
    use super::key_debug_name_for_input;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn key_debug_name_redacts_text_input() {
        assert_eq!(
            key_debug_name_for_input(key(KeyCode::Char('s')), true),
            "Char(<redacted>)"
        );
    }

    #[test]
    fn key_debug_name_keeps_command_modified_chars() {
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(
            key_debug_name_for_input(key, true),
            "KeyModifiers(CONTROL)+Char(s)"
        );
    }
}
