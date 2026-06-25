//! Sequence number tracker for hook authority sources.
//!
//! Each source (identified by `source_id`) must send strictly increasing
//! sequence numbers. The tracker rejects reports whose sequence is ≤ the last
//! accepted value, preventing stale or replayed authority.

use std::collections::HashMap;

/// Tracks the last-accepted sequence number per source ID.
#[derive(Debug, Default)]
pub struct SequenceTracker {
    last: HashMap<String, u64>,
}

impl SequenceTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attempt to accept a report from `source_id` with `seq`.
    ///
    /// Returns `true` when accepted (first report from this source, or
    /// `seq` is strictly greater than the last accepted value).
    /// Returns `false` when rejected (stale or replayed sequence).
    pub fn accept(&mut self, source_id: &str, seq: u64) -> bool {
        match self.last.get(source_id) {
            None => {
                // First report from this source — always accepted.
                self.last.insert(source_id.to_owned(), seq);
                true
            }
            Some(&last) if seq > last => {
                self.last.insert(source_id.to_owned(), seq);
                true
            }
            _ => false,
        }
    }

    /// Whether this tracker has seen any report from `source_id`.
    pub fn has_source(&self, source_id: &str) -> bool {
        self.last.contains_key(source_id)
    }

    /// Remove all sequence state for `source_id`. Called when authority
    /// is cleared so the source can re-register cleanly.
    pub fn clear_source(&mut self, source_id: &str) {
        self.last.remove(source_id);
    }

    /// Remove all source sequence state for a session-wide authority reset.
    pub fn clear_all(&mut self) {
        self.last.clear();
    }
}

#[cfg(test)]
mod tests;
