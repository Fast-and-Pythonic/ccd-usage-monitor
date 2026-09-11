//! Shared data model: what we know about the quota windows, and from where.


/// Where a piece of data came from. Shown in the tooltip so a stale number is
/// always attributable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Claude Code's own `statusLine` payload, relayed by our bridge.
    Bridge,
    /// The `oauth/usage` endpoint, polled directly.
    Api,
    /// The usage history the Claude desktop app writes for itself.
    Desktop,
    /// A session transcript under `~/.claude/projects`, read directly.
    Session,
    /// Synthetic data from `--demo`.
    Demo,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Bridge => "statusLine",
            Source::Api => "usage API",
            Source::Desktop => "desktop app",
            Source::Session => "transcript",
            Source::Demo => "demo",
        }
    }

    /// The same thing said for a machine rather than for a person.
    ///
    /// `label` is prose and is allowed to be reworded; this is what `--serve`
    /// publishes, so somebody else's widget can branch on it. Two names for one
    /// thing is the price of being able to improve one of them.
    pub fn key(self) -> &'static str {
        match self {
            Source::Bridge => "bridge",
            Source::Api => "api",
            Source::Desktop => "desktop",
            Source::Session => "session",
            Source::Demo => "demo",
        }
    }
}

/// One rate-limit window (5-hour or 7-day).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    /// Percentage consumed, 0..100 (can exceed 100 for spend limits).
    pub used_pct: f64,
    /// Unix epoch seconds when the window resets. Absent from sources that
    /// only report a percentage, such as the desktop app's history.
    pub resets_at: Option<i64>,
    /// Unix epoch seconds when we learned this value.
    pub observed_at: i64,
    pub source: Source,
}

impl Window {
    /// Merge two observations of the same window field by field.
    ///
    /// The percentage comes from whichever source saw it last. The reset time
    /// is different in kind: it is an absolute moment, so an older reading of
    /// it stays true until it passes, and a source that reports no reset time
    /// at all must not erase one another source already knows.
    pub fn merge(a: Option<Window>, b: Option<Window>) -> Option<Window> {
        match (a, b) {
            (Some(a), Some(b)) => {
                let (newer, older) = if b.observed_at >= a.observed_at {
                    (b, a)
                } else {
                    (a, b)
                };
                Some(Window {
                    resets_at: newer.resets_at.or(older.resets_at),
                    ..newer
                })
            }
            (Some(a), None) => Some(a),
            (None, b) => b,
        }
    }
}

/// Context-window fill of one Claude Code session.
///
/// The percentage is a division, and only one source knows both halves of it.
/// The tokens are what was actually counted; the limit is the window they were
/// counted against, which the status line states outright and the transcript
/// reader can only infer from the model. Both halves are carried so the panel
/// can show the division rather than only its answer — a denominator wrong by
/// five times is otherwise invisible, and five times is the real distance
/// between a 200K window and a 1M one.
#[derive(Clone, Debug)]
pub struct ContextInfo {
    /// `input_tokens + cache_creation_input_tokens + cache_read_input_tokens`
    /// of the last assistant turn — the sum Claude Code itself divides.
    /// `output_tokens` is deliberately not in it: the shipped formula reports
    /// that separately and does not count it against the window.
    pub used_tokens: Option<u64>,
    /// The denominator. Absent from a source that reports a percentage only.
    pub limit_tokens: Option<u64>,
    /// Already rounded and clamped the way Claude Code rounds it, so the pill
    /// and the app's own footer cannot differ by a point.
    pub used_pct: f64,
    /// Whether `limit_tokens` was stated by Claude Code or guessed by us.
    pub limit_stated: bool,
    /// The model id, because the window size is an attribute of the model.
    pub model: Option<String>,
    /// Something a person recognises: the live registry's name, else the
    /// transcript's own slug.
    pub label: Option<String>,
    pub session_id: String,
    pub observed_at: i64,
    pub source: Source,
}

impl ContextInfo {
    /// Merge two readings field by field, on `Window::merge`'s rule.
    ///
    /// The fill comes from whichever source saw it last. The window's *size* is
    /// different in kind — it is an attribute of the model, not of the moment —
    /// so a size stated once survives a newer reading that only guessed one,
    /// exactly as an absolute reset time survives a source that carries none.
    ///
    /// A size does not travel between models. A reading of opus-5 must not lend
    /// its million tokens to a reading of sonnet-4-6, and switching model
    /// mid-conversation is ordinary, not contrived.
    pub fn merge(a: Option<ContextInfo>, b: Option<ContextInfo>) -> Option<ContextInfo> {
        match (a, b) {
            (Some(a), Some(b)) => {
                let (newer, older) = if b.observed_at >= a.observed_at {
                    (b, a)
                } else {
                    (a, b)
                };
                let same_model = newer.model.is_none() || newer.model == older.model;
                if newer.limit_stated || !same_model || older.limit_tokens.is_none() {
                    return Some(newer);
                }
                // Only the denominator is borrowed, so the percentage has to be
                // recomputed rather than carried over from either reading.
                let used_pct = match (newer.used_tokens, older.limit_tokens) {
                    (Some(t), Some(l)) => pct_of(t, l),
                    _ => newer.used_pct,
                };
                Some(ContextInfo {
                    limit_tokens: older.limit_tokens,
                    limit_stated: older.limit_stated,
                    used_pct,
                    ..newer
                })
            }
            (Some(a), None) => Some(a),
            (None, b) => b,
        }
    }
}

/// The percentage Claude Code shows: rounded to a whole point and clamped, the
/// way `h4t` does it in the shipped binary, so the widget and the app's own
/// footer never differ by one.
pub fn pct_of(used: u64, limit: u64) -> f64 {
    if limit == 0 {
        return 0.0;
    }
    ((used as f64 / limit as f64) * 100.0).round().clamp(0.0, 100.0)
}

/// Everything the UI draws.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub context: Option<ContextInfo>,
}

impl Snapshot {
    pub fn merge(a: Snapshot, b: Snapshot) -> Snapshot {
        Snapshot {
            five_hour: Window::merge(a.five_hour, b.five_hour),
            seven_day: Window::merge(a.seven_day, b.seven_day),
            context: ContextInfo::merge(a.context, b.context),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(used: f64, resets: Option<i64>, at: i64, source: Source) -> Option<Window> {
        Some(Window {
            used_pct: used,
            resets_at: resets,
            observed_at: at,
            source,
        })
    }

    #[test]
    fn the_percentage_comes_from_the_newer_reading() {
        let old = win(50.0, Some(900), 100, Source::Api);
        let new = win(60.0, None, 200, Source::Desktop);
        let merged = Window::merge(old, new).unwrap();
        assert_eq!(merged.used_pct, 60.0);
        assert_eq!(merged.source, Source::Desktop);
    }

    #[test]
    fn a_source_without_a_reset_time_does_not_erase_one() {
        let with_reset = win(50.0, Some(900), 100, Source::Api);
        let without = win(60.0, None, 200, Source::Desktop);
        assert_eq!(Window::merge(with_reset, without).unwrap().resets_at, Some(900));
        // Order must not matter.
        assert_eq!(Window::merge(without, with_reset).unwrap().resets_at, Some(900));
    }

    #[test]
    fn a_newer_reset_time_wins_over_an_older_one() {
        let old = win(50.0, Some(900), 100, Source::Api);
        let new = win(60.0, Some(1800), 200, Source::Api);
        assert_eq!(Window::merge(old, new).unwrap().resets_at, Some(1800));
    }

    fn ctx(pct: f64, tokens: u64, limit: Option<u64>, stated: bool, model: &str, at: i64) -> Option<ContextInfo> {
        Some(ContextInfo {
            used_tokens: Some(tokens),
            limit_tokens: limit,
            used_pct: pct,
            limit_stated: stated,
            model: Some(model.to_string()),
            label: None,
            session_id: "s".into(),
            observed_at: at,
            source: Source::Session,
        })
    }

    #[test]
    fn the_percentage_is_rounded_the_way_claude_code_rounds_it() {
        // The same 88 958 tokens, in the two windows that are actually in use.
        // Getting the denominator wrong is not a rounding error, it is five
        // times the answer.
        assert_eq!(pct_of(88_958, 1_000_000), 9.0);
        assert_eq!(pct_of(88_958, 200_000), 44.0);
        assert_eq!(pct_of(400_000, 200_000), 100.0, "clamped, never above a full window");
        assert_eq!(pct_of(1, 0), 0.0, "a missing window is not a division by zero");
    }

    #[test]
    fn a_guessed_window_does_not_overwrite_a_stated_one() {
        let stated = ctx(44.0, 88_958, Some(200_000), true, "m", 100);
        let guessed = ctx(9.0, 90_000, Some(1_000_000), false, "m", 200);
        let merged = ContextInfo::merge(stated, guessed).unwrap();
        assert_eq!(merged.used_tokens, Some(90_000), "the fill is the newer reading's");
        assert_eq!(merged.limit_tokens, Some(200_000), "the size we were told beats the one we guessed");
        assert!(merged.limit_stated);
        assert_eq!(merged.used_pct, 45.0, "the borrowed denominator is applied, not the old percentage");
    }

    #[test]
    fn a_window_size_does_not_travel_between_models() {
        let stated = ctx(44.0, 88_958, Some(200_000), true, "sonnet", 100);
        let other = ctx(9.0, 90_000, Some(1_000_000), false, "opus", 200);
        let merged = ContextInfo::merge(stated, other).unwrap();
        assert_eq!(
            merged.limit_tokens,
            Some(1_000_000),
            "a size learned for one model says nothing about another"
        );
    }
}
