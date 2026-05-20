/// Input event parsing and routing.
///
/// Raw bytes from the client terminal are parsed here into
/// `InputEvent`s which the daemon/compositor then acts on.

/// Parsed input event from the client terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputEvent {
    /// Ctrl+J — open command palette.
    CommandPalette,
    /// Alt+arrow — move focus between panes.
    AltArrow(ArrowDir),
    /// Mouse button press (0-based col, row, button).
    MousePress { col: u16, row: u16, button: u8 },
    /// Raw bytes to forward to the active session.
    Data(Vec<u8>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowDir {
    Left,
    Right,
    Up,
    Down,
}

/// Parse raw bytes from the client into one or more `InputEvent`s.
/// Incomplete escape sequences are returned as Data.
pub fn parse(bytes: &[u8]) -> Vec<InputEvent> {
    let mut events = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        // Ctrl+J = 0x0A
        if bytes[i] == 0x0A {
            events.push(InputEvent::CommandPalette);
            i += 1;
            continue;
        }

        // Check for ESC sequences.
        if bytes[i] == 0x1B {
            // Alt+Left:  ESC [ 1 ; 3 D
            // Alt+Right: ESC [ 1 ; 3 C
            // Alt+Up:    ESC [ 1 ; 3 A
            // Alt+Down:  ESC [ 1 ; 3 B
            if let Some(dir) = parse_alt_arrow(&bytes[i..]) {
                let seq_len = alt_arrow_len(dir);
                events.push(InputEvent::AltArrow(dir));
                i += seq_len;
                continue;
            }

            // Mouse: ESC [ M <btn+32> <col+32> <row+32>  (X10 mouse)
            if bytes[i..].starts_with(b"\x1b[M") && bytes.len() >= i + 6 {
                let btn = bytes[i + 3].saturating_sub(32);
                let col = u16::from(bytes[i + 4].saturating_sub(32).saturating_sub(1));
                let row = u16::from(bytes[i + 5].saturating_sub(32).saturating_sub(1));
                events.push(InputEvent::MousePress {
                    col,
                    row,
                    button: btn & 0x3,
                });
                i += 6;
                continue;
            }

            // SGR mouse: ESC [ < Pm ; Pm ; Pm M/m
            if bytes[i..].starts_with(b"\x1b[<") {
                if let Some((ev, len)) = parse_sgr_mouse(&bytes[i..]) {
                    events.push(ev);
                    i += len;
                    continue;
                }
            }

            // Unknown escape — pass through as data.
        }

        // Accumulate raw bytes into Data event.
        let start = i;
        while i < bytes.len() && (bytes[i] != 0x1B && bytes[i] != 0x0A) {
            i += 1;
        }
        if i > start {
            events.push(InputEvent::Data(bytes[start..i].to_vec()));
        } else if bytes[i] == 0x1B {
            // Pass through the ESC sequence we couldn't parse.
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != 0x1B {
                i += 1;
            }
            events.push(InputEvent::Data(bytes[start..i].to_vec()));
        }
    }

    events
}

fn parse_alt_arrow(bytes: &[u8]) -> Option<ArrowDir> {
    // ESC [ 1 ; 3 A/B/C/D
    if bytes.len() >= 7 && bytes.starts_with(b"\x1b[1;3") {
        return match bytes[6] {
            b'A' => Some(ArrowDir::Up),
            b'B' => Some(ArrowDir::Down),
            b'C' => Some(ArrowDir::Right),
            b'D' => Some(ArrowDir::Left),
            _ => None,
        };
    }
    // ESC [ 3 A/B/C/D (some terminals)
    if bytes.len() >= 5 && bytes.starts_with(b"\x1b[3") {
        return match bytes[4] {
            b'A' => Some(ArrowDir::Up),
            b'B' => Some(ArrowDir::Down),
            b'C' => Some(ArrowDir::Right),
            b'D' => Some(ArrowDir::Left),
            _ => None,
        };
    }
    None
}

fn alt_arrow_len(dir: ArrowDir) -> usize {
    // ESC [ 1 ; 3 A = 7 bytes
    let _ = dir;
    7
}

fn parse_sgr_mouse(bytes: &[u8]) -> Option<(InputEvent, usize)> {
    // ESC [ < params M or m
    let rest = bytes.strip_prefix(b"\x1b[<")?;
    let end = rest.iter().position(|&b| b == b'M' || b == b'm')?;
    let params: Vec<u32> = rest[..end]
        .split(|&b| b == b';')
        .filter_map(|p| std::str::from_utf8(p).ok().and_then(|s| s.parse().ok()))
        .collect();
    if params.len() < 3 {
        return None;
    }
    let button = params[0] as u8;
    let col = (params[1] as u16).saturating_sub(1);
    let row = (params[2] as u16).saturating_sub(1);
    let is_press = rest[end] == b'M';
    let len = 3 + end + 1; // ESC [ < ... M/m
    if is_press {
        Some((InputEvent::MousePress { col, row, button }, len))
    } else {
        Some((InputEvent::Data(bytes[..len].to_vec()), len))
    }
}
