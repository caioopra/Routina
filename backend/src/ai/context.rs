//! Context-window management for LLM message histories.
//!
//! Provides `truncate_to_budget`, which trims a `Vec<Message>` so the estimated
//! token count stays within `max_tokens`.  The estimate is intentionally rough
//! (char-count / 4) — precise tokenisation would require provider-specific
//! tokenisers that are unavailable at this layer.

use crate::ai::provider::{Message, Role};

/// Default token budget passed to `truncate_to_budget` when no explicit limit
/// is supplied.
pub const DEFAULT_MAX_TOKENS: u32 = 28_000;

/// Estimate the number of tokens in a message using a simple char/4 heuristic.
///
/// This is a rough approximation — real tokenisers vary by model — but it is
/// fast, dependency-free, and conservative enough for context management.
fn estimate_tokens(msg: &Message) -> u32 {
    // Add a small per-message overhead for role/metadata bytes.
    const OVERHEAD: u32 = 4;
    (msg.content.len() as u32 / 4).saturating_add(OVERHEAD)
}

/// Trim `messages` so the total estimated token count fits within `max_tokens`.
///
/// Invariants upheld:
/// - The system message (first message with `role == System`) is always kept.
/// - The last user message is always kept.
/// - When messages must be dropped, oldest non-system, non-last-user messages
///   are removed first (the middle of the conversation is trimmed).
/// - If any message was dropped a truncation notice is prepended as a `System`
///   message immediately after the original system message (or at position 0 if
///   there is no system message).
///
/// If the history is already within budget the slice is returned unchanged.
pub fn truncate_to_budget(messages: Vec<Message>, max_tokens: u32) -> Vec<Message> {
    if messages.is_empty() {
        return messages;
    }

    // Find the last user message index so we can protect it.
    let last_user_idx = messages
        .iter()
        .rposition(|m| m.role == Role::User)
        .unwrap_or(messages.len().saturating_sub(1));

    // Compute per-message estimates once.
    let estimates: Vec<u32> = messages.iter().map(estimate_tokens).collect();
    let total: u32 = estimates.iter().sum();

    if total <= max_tokens {
        return messages;
    }

    // We need to shed (total - max_tokens) tokens.  Walk from oldest to newest,
    // skipping the system message (index 0 if it has Role::System) and the last
    // user message.  Collect indices to drop.
    let system_idx: Option<usize> = if messages.first().is_some_and(|m| m.role == Role::System) {
        Some(0)
    } else {
        None
    };

    let mut budget_remaining = total.saturating_sub(max_tokens);
    let mut drop_indices: Vec<usize> = Vec::new();

    for (i, tokens) in estimates.iter().enumerate() {
        if budget_remaining == 0 {
            break;
        }
        // Never drop the system message or the last user message.
        let is_protected = system_idx.is_some_and(|si| si == i) || i == last_user_idx;
        if is_protected {
            continue;
        }
        drop_indices.push(i);
        budget_remaining = budget_remaining.saturating_sub(*tokens);
    }

    if drop_indices.is_empty() {
        return messages;
    }

    // Build the trimmed list, skipping dropped indices.
    let drop_set: std::collections::HashSet<usize> = drop_indices.into_iter().collect();
    let trimmed: Vec<Message> = messages
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !drop_set.contains(i))
        .map(|(_, m)| m)
        .collect();

    // Prepend a truncation notice.  Insert after the system message if one
    // exists, otherwise at position 0.
    let notice = Message::system("[Note: older messages were omitted to fit the context window.]");

    let insert_pos = if trimmed.first().is_some_and(|m| m.role == Role::System) {
        1
    } else {
        0
    };

    let mut result = trimmed;
    result.insert(insert_pos, notice);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::provider::Role;

    fn sys(content: &str) -> Message {
        Message::system(content)
    }
    fn user(content: &str) -> Message {
        Message::user(content)
    }
    fn asst(content: &str) -> Message {
        Message::assistant(content)
    }

    // ── estimate_tokens ───────────────────────────────────────────────────────

    #[test]
    fn estimate_tokens_empty_content() {
        let msg = user("");
        // overhead = 4, len/4 = 0 → 4
        assert_eq!(estimate_tokens(&msg), 4);
    }

    #[test]
    fn estimate_tokens_known_length() {
        let msg = user("aaaa"); // len=4, 4/4=1, +4 overhead = 5
        assert_eq!(estimate_tokens(&msg), 5);
    }

    // ── truncate_to_budget ────────────────────────────────────────────────────

    #[test]
    fn no_truncation_when_within_budget() {
        let messages = vec![sys("system"), user("user msg"), asst("assistant msg")];
        let result = truncate_to_budget(messages.clone(), DEFAULT_MAX_TOKENS);
        assert_eq!(result.len(), messages.len());
    }

    #[test]
    fn empty_messages_returned_unchanged() {
        let result = truncate_to_budget(vec![], 1000);
        assert!(result.is_empty());
    }

    #[test]
    fn system_message_always_kept() {
        // Build a list where the system message alone would not cause truncation
        // but all others together exceed the budget.
        let system_content = "S".repeat(4); // ~5 tokens
        // Use large middle messages.
        let big = "A".repeat(4000); // ~1004 tokens each
        let messages = vec![sys(&system_content), asst(&big), asst(&big), user("hi")];
        // Budget small enough that middle messages must be dropped.
        let result = truncate_to_budget(messages, 200);

        // System message must still be present.
        assert!(
            result
                .iter()
                .any(|m| m.role == Role::System && m.content == system_content),
            "system message must be preserved; got: {result:?}"
        );
    }

    #[test]
    fn last_user_message_always_kept() {
        let big = "A".repeat(4000);
        let last_user_msg = "last user question";
        let messages = vec![sys("system"), asst(&big), asst(&big), user(last_user_msg)];
        let result = truncate_to_budget(messages, 200);

        let has_last_user = result
            .iter()
            .any(|m| m.role == Role::User && m.content == last_user_msg);
        assert!(has_last_user, "last user message must be preserved");
    }

    #[test]
    fn truncation_notice_prepended_after_system_when_trimmed() {
        let big = "A".repeat(4000);
        let messages = vec![
            sys("system"),
            asst(&big), // will be dropped
            user("hi"),
        ];
        let result = truncate_to_budget(messages, 200);

        // Position 0 must be the original system message.
        assert_eq!(result[0].role, Role::System);
        assert_eq!(result[0].content, "system");

        // Position 1 must be the truncation notice.
        assert_eq!(result[1].role, Role::System);
        assert!(
            result[1].content.contains("older messages were omitted"),
            "notice not found at position 1; got: {}",
            result[1].content
        );
    }

    #[test]
    fn truncation_notice_at_position_zero_when_no_system_message() {
        let big = "A".repeat(4000);
        let messages = vec![
            asst(&big), // dropped
            user("hi"),
        ];
        let result = truncate_to_budget(messages, 100);

        // The first message should be the truncation notice.
        assert_eq!(result[0].role, Role::System);
        assert!(result[0].content.contains("older messages were omitted"));
    }

    #[test]
    fn oldest_non_protected_messages_dropped_first() {
        let big = "A".repeat(4000);
        let messages = vec![
            sys("system"),
            asst("old_assistant"), // should be dropped first — oldest non-protected
            asst(&big),            // newer, also non-protected
            user("final user"),
        ];
        // Budget that forces at least one drop but not necessarily all.
        let result = truncate_to_budget(messages, 300);

        // old_assistant (index 1) must be dropped before the big one.
        let has_old = result.iter().any(|m| m.content == "old_assistant");
        assert!(
            !has_old,
            "oldest non-protected message must be dropped first"
        );
    }

    #[test]
    fn many_messages_truncated_to_budget() {
        // 100 assistant messages each ~500 tokens (2000 chars).
        let msg_content = "X".repeat(2000);
        let mut messages = vec![sys("system")];
        for _ in 0..100 {
            messages.push(asst(&msg_content));
        }
        messages.push(user("final user"));

        let result = truncate_to_budget(messages, 5000);

        // Total should now fit within 5000 tokens (roughly).
        let total: u32 = result.iter().map(estimate_tokens).sum();
        // Allow a small overrun because the last retained messages may push us
        // slightly over, but the drop loop should have gotten us close.
        assert!(
            total <= 5000 + 600, // 600 slack for the last pair
            "total tokens {total} should be near or below 5000"
        );

        // Protected messages still present.
        assert!(
            result
                .iter()
                .any(|m| m.role == Role::System && m.content == "system")
        );
        assert!(
            result
                .iter()
                .any(|m| m.role == Role::User && m.content == "final user")
        );
    }

    #[test]
    fn no_truncation_notice_when_nothing_dropped() {
        let messages = vec![sys("system"), user("hi")];
        let result = truncate_to_budget(messages, DEFAULT_MAX_TOKENS);

        let notice_count = result
            .iter()
            .filter(|m| m.content.contains("older messages were omitted"))
            .count();
        assert_eq!(
            notice_count, 0,
            "no truncation notice when nothing was dropped"
        );
    }
}
