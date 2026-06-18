//! Keybinding registry — single source of truth coupling key dispatch and hint advertisement.
//!
//! A [`Keymap<A>`] binds each action to one or more key chords and a hint label. The
//! dispatcher matches incoming keys against the table; the hint renderer produces
//! [`HintSpan`] sequences from the same table. Divergence between handled keys and
//! advertised keys is therefore structurally impossible for [`Visibility::Shown`] and
//! [`Visibility::HiddenAlias`] bindings.

use crate::geometry::HintSpan;
use crate::scroll::ScrollAxes;

// ── Neutral logical key ──────────────────────────────────────────────────────

/// Platform-neutral key identity. Both the crossterm surfaces (host, launch) and
/// the capsule's raw-byte parser produce and match this type, so a single
/// [`Keymap`] covers all surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogicalKey {
    Char(char),
    Enter,
    Esc,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
}

/// Modifier flags packed into a `u8`. Bit 0 = Ctrl, bit 1 = Alt, bit 2 = Shift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Mods(u8);

impl Mods {
    pub const NONE: Self = Self(0);
    pub const CTRL: Self = Self(1);
    pub const ALT: Self = Self(2);
    pub const SHIFT: Self = Self(4);

    /// Return a copy of `self` with the Ctrl bit set.
    #[must_use]
    pub const fn with_ctrl(self) -> Self {
        Self(self.0 | Self::CTRL.0)
    }

    /// Return a copy of `self` with the Alt bit set.
    #[must_use]
    pub const fn with_alt(self) -> Self {
        Self(self.0 | Self::ALT.0)
    }

    /// Return a copy of `self` with the Shift bit set.
    #[must_use]
    pub const fn with_shift(self) -> Self {
        Self(self.0 | Self::SHIFT.0)
    }

    /// True if every bit in `other` is also set in `self`.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// True if no modifier bits are set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// A key chord: a logical key plus zero or more modifier bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyChord {
    pub key: LogicalKey,
    pub mods: Mods,
}

impl KeyChord {
    /// Chord with no modifiers.
    #[must_use]
    pub const fn plain(key: LogicalKey) -> Self {
        Self { key, mods: Mods::NONE }
    }

    /// Chord with Ctrl held.
    #[must_use]
    pub const fn ctrl(key: LogicalKey) -> Self {
        Self { key, mods: Mods::CTRL }
    }

    /// Chord with Alt held.
    #[must_use]
    pub const fn alt(key: LogicalKey) -> Self {
        Self { key, mods: Mods::ALT }
    }

    /// Chord with Shift held (typically only meaningful for non-Char keys).
    #[must_use]
    pub const fn shift(key: LogicalKey) -> Self {
        Self { key, mods: Mods::SHIFT }
    }
}

/// Convert a crossterm `KeyEvent` into a platform-neutral [`KeyChord`].
///
/// Shift is only tracked for non-`Char` keys because for `Char` keys the
/// shifted character is already encoded in the `char` value (`'Q'` vs `'q'`).
/// Unknown key codes (function keys, media keys, …) map to
/// `LogicalKey::Char('\0')` which will never match a real binding.
impl From<crossterm::event::KeyEvent> for KeyChord {
    fn from(ev: crossterm::event::KeyEvent) -> Self {
        use crossterm::event::{KeyCode, KeyModifiers};
        let is_char = matches!(ev.code, KeyCode::Char(_));
        let key = match ev.code {
            KeyCode::Char(c) => LogicalKey::Char(c),
            KeyCode::Enter => LogicalKey::Enter,
            KeyCode::Esc => LogicalKey::Esc,
            KeyCode::Tab => LogicalKey::Tab,
            KeyCode::BackTab => LogicalKey::BackTab,
            KeyCode::Up => LogicalKey::Up,
            KeyCode::Down => LogicalKey::Down,
            KeyCode::Left => LogicalKey::Left,
            KeyCode::Right => LogicalKey::Right,
            KeyCode::Home => LogicalKey::Home,
            KeyCode::End => LogicalKey::End,
            KeyCode::PageUp => LogicalKey::PageUp,
            KeyCode::PageDown => LogicalKey::PageDown,
            KeyCode::Backspace => LogicalKey::Backspace,
            KeyCode::Delete => LogicalKey::Delete,
            _ => LogicalKey::Char('\0'),
        };
        let mut mods = Mods::NONE;
        if ev.modifiers.contains(KeyModifiers::CONTROL) {
            mods = mods.with_ctrl();
        }
        if ev.modifiers.contains(KeyModifiers::ALT) {
            mods = mods.with_alt();
        }
        // Shift is intrinsic to Char casing; only track it for non-Char keys.
        if !is_char && ev.modifiers.contains(KeyModifiers::SHIFT) {
            mods = mods.with_shift();
        }
        Self { key, mods }
    }
}

// ── Binding model ────────────────────────────────────────────────────────────

/// Whether a binding is visible in the hint bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// Key is advertised in the hint bar.
    Shown,
    /// Key works but is not advertised (convenience alias, e.g. vim `h`/`j`/`k`/`l`).
    HiddenAlias,
    /// Key is consumed internally by the widget (e.g. editing keys in a text input).
    Internal,
}

/// One entry in a [`Keymap`]: a set of chords all mapping to the same action.
///
/// The first chord in `chords` drives the glyph in the hint bar; remaining
/// chords are aliases. Set `glyph` to a `Some` string for grouped glyphs like
/// `"↑↓"` that span multiple bindings.
#[derive(Debug)]
pub struct KeyBinding<A> {
    /// All chords that fire this action. First chord drives the hint glyph.
    pub chords: &'static [KeyChord],
    /// The action value returned by [`Keymap::dispatch`].
    pub action: A,
    /// Label displayed after the key glyph in the hint bar (e.g. `"confirm"`).
    /// `None` silences the label; set to `None` for `Internal` bindings.
    pub hint: Option<&'static str>,
    /// Whether this binding appears in the hint bar.
    pub visibility: Visibility,
    /// Override the auto-derived glyph from [`chord_glyph`]. Use for grouped
    /// glyphs like `"↑↓"` or combined glyphs like `"N/Esc"`.
    pub glyph: Option<&'static str>,
}

/// A static keymap binding all chords to actions for a single widget context.
#[derive(Debug)]
pub struct Keymap<A: 'static> {
    bindings: &'static [KeyBinding<A>],
}

impl<A: Copy + 'static> Keymap<A> {
    /// Construct a keymap from a static binding slice.
    #[must_use]
    pub const fn new(bindings: &'static [KeyBinding<A>]) -> Self {
        Self { bindings }
    }

    /// Return the action for the first binding whose chord set contains `chord`,
    /// or `None` if no binding matches.
    #[must_use]
    pub fn dispatch(&self, chord: KeyChord) -> Option<A> {
        self.bindings
            .iter()
            .find(|b| b.chords.contains(&chord))
            .map(|b| b.action)
    }

    /// Produce [`HintSpan`] sequences for all [`Visibility::Shown`] bindings.
    /// Adjacent `Shown` bindings are separated by [`HintSpan::Sep`].
    #[must_use]
    pub fn hint_spans(&self) -> Vec<HintSpan<'static>> {
        let mut spans: Vec<HintSpan<'static>> = Vec::new();
        for binding in self.bindings.iter().filter(|b| b.visibility == Visibility::Shown) {
            if !spans.is_empty() {
                spans.push(HintSpan::Sep);
            }
            let glyph: &'static str = binding
                .glyph
                .unwrap_or_else(|| chord_glyph(binding.chords.first().copied()));
            spans.push(HintSpan::Key(glyph));
            if let Some(label) = binding.hint {
                spans.push(HintSpan::Text(label));
            }
        }
        spans
    }

    /// Like [`hint_spans`] but omits scroll-axis arrow bindings when the
    /// corresponding scroll axis is unavailable (matching the behaviour of
    /// [`crate::components::scroll_hint_spans`]).
    #[must_use]
    pub fn hint_spans_for_axes(&self, axes: ScrollAxes) -> Vec<HintSpan<'static>> {
        let mut spans: Vec<HintSpan<'static>> = Vec::new();
        for binding in self.bindings.iter().filter(|b| b.visibility == Visibility::Shown) {
            if !self.axis_gate_passes(binding, axes) {
                continue;
            }
            if !spans.is_empty() {
                spans.push(HintSpan::Sep);
            }
            let glyph: &'static str = binding
                .glyph
                .unwrap_or_else(|| chord_glyph(binding.chords.first().copied()));
            spans.push(HintSpan::Key(glyph));
            if let Some(label) = binding.hint {
                spans.push(HintSpan::Text(label));
            }
        }
        spans
    }

    fn axis_gate_passes(&self, binding: &KeyBinding<A>, axes: ScrollAxes) -> bool {
        let all_vertical = !binding.chords.is_empty()
            && binding.chords.iter().all(|c| {
                matches!(c.key, LogicalKey::Up | LogicalKey::Down) && c.mods.is_empty()
            });
        let all_horizontal = !binding.chords.is_empty()
            && binding.chords.iter().all(|c| {
                matches!(c.key, LogicalKey::Left | LogicalKey::Right) && c.mods.is_empty()
            });
        if all_vertical && !axes.vertical {
            return false;
        }
        if all_horizontal && !axes.horizontal {
            return false;
        }
        true
    }
}

// ── Glyph derivation ─────────────────────────────────────────────────────────

/// Derive the hint-bar key glyph from a chord.
///
/// Reproduces the exact glyphs already in use across the codebase so output is
/// byte-identical to hand-written hints. Callers that need a *grouped* glyph
/// (e.g. `"↑↓"` for a pair of bindings) should set [`KeyBinding::glyph`]
/// instead of relying on this function.
///
/// Returns `""` when `chord` is `None`. Returns `"?"` for Char values not in
/// the common-shortcut set — callers must supply an explicit `glyph` for those.
#[must_use]
pub fn chord_glyph(chord: Option<KeyChord>) -> &'static str {
    let Some(chord) = chord else { return "" };
    match chord.key {
        LogicalKey::Char(c) if chord.mods.contains(Mods::CTRL) => match c.to_ascii_lowercase() {
            'q' => "Ctrl+Q",
            'c' => "Ctrl-C",
            'l' => "Ctrl+L",
            'h' => "Ctrl+H",
            _ => "Ctrl+?",
        },
        LogicalKey::Char(c) if chord.mods.is_empty() || chord.mods == Mods::SHIFT => {
            match c.to_ascii_uppercase() {
                'A' => "A",
                'B' => "B",
                'C' => "C",
                'D' => "D",
                'E' => "E",
                'F' => "F",
                'G' => "G",
                'H' => "H",
                'I' => "I",
                'J' => "J",
                'K' => "K",
                'L' => "L",
                'M' => "M",
                'N' => "N",
                'O' => "O",
                'P' => "P",
                'Q' => "Q",
                'R' => "R",
                'S' => "S",
                'T' => "T",
                'U' => "U",
                'V' => "V",
                'W' => "W",
                'X' => "X",
                'Y' => "Y",
                'Z' => "Z",
                '*' => "*",
                '1' => "1",
                '2' => "2",
                '3' => "3",
                '4' => "4",
                _ => "?",
            }
        }
        LogicalKey::Enter => "\u{21b5}", // ↵
        LogicalKey::Esc => "Esc",
        LogicalKey::Tab => "\u{21e5}",  // ⇥
        LogicalKey::BackTab => "\u{21e4}", // ⇤
        LogicalKey::Up => "\u{2191}",   // ↑
        LogicalKey::Down => "\u{2193}", // ↓
        LogicalKey::Left => "\u{2190}", // ←
        LogicalKey::Right => "\u{2192}", // →
        LogicalKey::Home => "Home",
        LogicalKey::End => "End",
        LogicalKey::PageUp => "PgUp",
        LogicalKey::PageDown => "PgDn",
        LogicalKey::Backspace => "⌫",
        LogicalKey::Delete => "Del",
        _ => "?",
    }
}

#[cfg(test)]
mod tests;
