//! JSON reader for Amp thread files.
//!
//! Reads `~/.local/share/amp/threads/*.json`.

use super::{TokenSession, json_u64};

pub(crate) fn poll_session(session: &mut TokenSession) -> bool {
    // Amp keeps thread files flat directly under `threads/` — top level only
    // (max_depth 0), so unrelated nested JSON never inflates the spend total.
    let files = super::find_provider_files(&["/home/agent/.local/share/amp/threads"], "json", 0);
    super::recompute_spend(&files, "amp", |content, acc| {
        let Ok(val) = serde_json::from_str::<serde_json::Value>(content) else {
            return;
        };
        // Thread JSON: array of messages, each may have usage metadata.
        let Some(messages) = val
            .as_array()
            .or_else(|| val.get("messages").and_then(|m| m.as_array()))
        else {
            return;
        };
        for msg in messages {
            if let Some(usage) = msg.get("usage") {
                let input = json_u64(usage, "input_tokens");
                let output = json_u64(usage, "output_tokens");
                let cache_read = json_u64(usage, "cache_read_input_tokens");
                let cache_write = json_u64(usage, "cache_creation_input_tokens");
                acc.input = acc.input.saturating_add(input);
                acc.output = acc.output.saturating_add(output);
                acc.cache_read = acc.cache_read.saturating_add(cache_read);
                acc.cache_write = acc.cache_write.saturating_add(cache_write);
                acc.seen = true;
            }
            if let Some(model) = msg.get("model").and_then(|v| v.as_str()) {
                acc.model = Some(model.to_owned());
            }
        }
    })
    .is_some_and(|acc| acc.commit(&mut session.totals))
}

#[cfg(test)]
mod tests;
