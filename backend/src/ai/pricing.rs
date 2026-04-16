//! Hard-coded LLM pricing table for cost estimation.
//!
//! Prices are in USD per 1 million tokens (input/output separately).
//! This module is intentionally kept simple — no DB, no config — so cost
//! estimates are always available synchronously.

/// Per-model pricing entry (USD per 1M tokens).
pub struct ModelPrice {
    pub input_per_million: f64,
    pub output_per_million: f64,
}

/// Return the pricing entry for the given `(provider, model)` pair.
///
/// Matching uses `str::contains` on the model name so that full version
/// strings like `"gemini-2.5-flash-preview-05-20"` still match `"flash"`.
/// Returns `None` for unknown combinations so the caller can log a warning.
pub fn price_for(provider: &str, model: &str) -> Option<ModelPrice> {
    match (provider, model) {
        ("gemini", m) if m.contains("flash") => Some(ModelPrice {
            input_per_million: 0.15,
            output_per_million: 0.60,
        }),
        ("gemini", m) if m.contains("pro") => Some(ModelPrice {
            input_per_million: 1.25,
            output_per_million: 5.00,
        }),
        ("claude", m) if m.contains("sonnet") => Some(ModelPrice {
            input_per_million: 3.00,
            output_per_million: 15.00,
        }),
        ("claude", m) if m.contains("haiku") => Some(ModelPrice {
            input_per_million: 0.25,
            output_per_million: 1.25,
        }),
        ("claude", m) if m.contains("opus") => Some(ModelPrice {
            input_per_million: 15.00,
            output_per_million: 75.00,
        }),
        _ => None,
    }
}

/// Estimate the cost in USD for a single LLM call.
///
/// Falls back to `0.0` and logs a warning for unknown `(provider, model)`
/// combinations so that the calling code can continue without crashing.
pub fn estimate_cost_usd(
    provider: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
) -> f64 {
    match price_for(provider, model) {
        Some(p) => {
            (input_tokens as f64 * p.input_per_million / 1_000_000.0)
                + (output_tokens as f64 * p.output_per_million / 1_000_000.0)
        }
        None => {
            tracing::warn!(
                provider,
                model,
                "unknown model for cost estimation — returning 0.0"
            );
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── price_for ─────────────────────────────────────────────────────────────

    #[test]
    fn price_for_gemini_flash() {
        let p = price_for("gemini", "gemini-2.5-flash-preview-05-20").unwrap();
        assert_eq!(p.input_per_million, 0.15);
        assert_eq!(p.output_per_million, 0.60);
    }

    #[test]
    fn price_for_gemini_pro() {
        let p = price_for("gemini", "gemini-1.5-pro-latest").unwrap();
        assert_eq!(p.input_per_million, 1.25);
        assert_eq!(p.output_per_million, 5.00);
    }

    #[test]
    fn price_for_claude_sonnet() {
        let p = price_for("claude", "claude-sonnet-4-20250514").unwrap();
        assert_eq!(p.input_per_million, 3.00);
        assert_eq!(p.output_per_million, 15.00);
    }

    #[test]
    fn price_for_claude_haiku() {
        let p = price_for("claude", "claude-3-haiku-20240307").unwrap();
        assert_eq!(p.input_per_million, 0.25);
        assert_eq!(p.output_per_million, 1.25);
    }

    #[test]
    fn price_for_claude_opus() {
        let p = price_for("claude", "claude-opus-4-5").unwrap();
        assert_eq!(p.input_per_million, 15.00);
        assert_eq!(p.output_per_million, 75.00);
    }

    #[test]
    fn price_for_unknown_provider_returns_none() {
        assert!(price_for("openai", "gpt-4o").is_none());
    }

    #[test]
    fn price_for_unknown_model_returns_none() {
        assert!(price_for("gemini", "gemini-unknown-xyz").is_none());
    }

    // ── estimate_cost_usd ─────────────────────────────────────────────────────

    #[test]
    fn estimate_cost_gemini_flash_known_values() {
        // 1_000_000 input tokens at $0.15/M = $0.15
        // 500_000 output tokens at $0.60/M = $0.30
        // Total = $0.45
        let cost = estimate_cost_usd(
            "gemini",
            "gemini-2.5-flash-preview-05-20",
            1_000_000,
            500_000,
        );
        let diff = (cost - 0.45_f64).abs();
        assert!(diff < 1e-9, "expected ~0.45, got {cost}");
    }

    #[test]
    fn estimate_cost_claude_sonnet() {
        // 100 input tokens at $3/M = $0.0003
        // 50 output tokens at $15/M = $0.00075
        // Total = $0.00105
        let cost = estimate_cost_usd("claude", "claude-sonnet-4-20250514", 100, 50);
        let diff = (cost - 0.00105_f64).abs();
        assert!(diff < 1e-9, "expected ~0.00105, got {cost}");
    }

    #[test]
    fn estimate_cost_zero_tokens_is_zero() {
        let cost = estimate_cost_usd("gemini", "gemini-2.5-flash-preview-05-20", 0, 0);
        assert_eq!(cost, 0.0);
    }

    #[test]
    fn estimate_cost_unknown_model_returns_zero() {
        let cost = estimate_cost_usd("unknown_provider", "unknown_model", 1000, 500);
        assert_eq!(cost, 0.0, "unknown model must return 0.0");
    }

    #[test]
    fn estimate_cost_unknown_gemini_model_returns_zero() {
        let cost = estimate_cost_usd("gemini", "gemini-future-xyz", 1000, 500);
        assert_eq!(cost, 0.0);
    }
}
