//! Derived numbers: countdowns, reset labels, and how fast the weekly budget
//! may be spent from here on.

use chrono::{DateTime, Datelike, Local, TimeZone, Weekday};

use crate::config::PaceView;

pub const FIVE_HOUR_SECS: i64 = 5 * 3600;
pub const SEVEN_DAY_SECS: i64 = 7 * 86400;

/// `3:37` — time left until a reset. Clamped at zero; a window whose reset has
/// passed reads `0:00` rather than a negative countdown.
///
/// Rounded **up** to the whole minute, which is both what Claude Code's own
/// readout does and the honest reading: with 2 h 19 min 40 s left there really
/// is more than 2:19 to go, and truncating makes the widget look a minute
/// behind the app it mirrors.
pub fn fmt_remaining(secs: i64) -> String {
    let mins = (secs.max(0) + 59) / 60;
    format!("{}:{:02}", mins / 60, mins % 60)
}

/// `18:00` — local wall-clock time of the reset, to the nearest minute.
///
/// Nearest rather than truncated so it agrees with the countdown beside it: a
/// reset at 00:09:40 reads `00:10`, and 2:20 before it is 21:50.
pub fn fmt_clock(epoch: i64) -> String {
    match Local.timestamp_opt(round_to_minute(epoch), 0).single() {
        Some(dt) => dt.format("%H:%M").to_string(),
        None => "--:--".to_string(),
    }
}

fn round_to_minute(epoch: i64) -> i64 {
    epoch.div_euclid(60) * 60 + if epoch.rem_euclid(60) >= 30 { 60 } else { 0 }
}

/// Which days belong to the weekly count.
///
/// Neither end is obvious. Today is usually half spent by the time anyone
/// looks, and the day the window resets is worth only the hours before the
/// reset — with a reset at 07:00 it is worth almost nothing. Both ends are
/// therefore a setting rather than a decision made here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DayCount {
    pub today: bool,
    pub reset_day: bool,
}

impl Default for DayCount {
    fn default() -> Self {
        DayCount { today: false, reset_day: true }
    }
}

/// How many days the rest of the weekly quota has to be spread over.
///
/// Counted on local dates rather than by dividing seconds: the question is a
/// calendar one. The count is the length of a run of days — from today or
/// tomorrow, up to the reset day or the one before it — so Thursday with a
/// reset on Tuesday gives 5 by default (Fri…Tue), 6 with today, 4 without the
/// reset day.
pub fn days_until(resets_at: i64, now: i64, count: DayCount) -> i64 {
    let (Some(reset), Some(today)) = (
        Local.timestamp_opt(resets_at, 0).single(),
        Local.timestamp_opt(now, 0).single(),
    ) else {
        return 0;
    };
    let span = (reset.date_naive() - today.date_naive()).num_days();
    let drop_first = i64::from(!count.today);
    let drop_last = i64::from(!count.reset_day);
    (span + 1 - drop_first - drop_last).max(0)
}

/// `5д` / `5d` — the day count as it appears on the pill.
pub fn fmt_days_until(resets_at: i64, now: i64, lang: &str, count: DayCount) -> String {
    let d = days_until(resets_at, now, count);
    if lang.eq_ignore_ascii_case("ru") {
        format!("{d}д")
    } else {
        format!("{d}d")
    }
}

fn weekday_name(day: Weekday, lang: &str) -> &'static str {
    const EN: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const RU: [&str; 7] = ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"];
    let i = day.num_days_from_monday() as usize;
    if lang.eq_ignore_ascii_case("ru") { RU[i] } else { EN[i] }
}

/// How much of the weekly limit may be spent per day to finish exactly at 100%.
/// Returns percent-per-day.
///
/// `days` is the figure shown beside it (`days_until`), not a fraction of
/// wall-clock time: the two numbers sit next to each other on the pill and must
/// agree. Floored at one day, so the last day of a window says "all of it"
/// rather than dividing by nothing.
pub fn daily_allowance(used_pct: f64, days: i64) -> f64 {
    let left_pct = (100.0 - used_pct).max(0.0);
    (left_pct / days.max(1) as f64).min(100.0)
}

/// The largest factor `pace_ratio` will report in either direction. Past this
/// the number has stopped meaning anything a person can act on, and an
/// unbounded one would print as `inf` at the end of every window.
const RATIO_MAX: f64 = 9.9;

/// How many times slower than an even burn the rest of the window has to go.
///
/// The quota left is measured against the time left rather than the other way
/// round, so the number is the one worth acting on: 20% of the quota with two
/// hours of a five-hour window left is `2.0` — each remaining percent has to
/// last twice as long as an even burn would give it. Below 1 the reading is
/// reversed by the formatter and shown as headroom.
pub fn pace_ratio(used_pct: f64, target_pct: f64) -> f64 {
    let quota_left = (100.0 - used_pct).max(0.0);
    let time_left = (100.0 - target_pct).max(0.0);
    if quota_left <= 0.0 {
        return RATIO_MAX;
    }
    (time_left / quota_left).clamp(1.0 / RATIO_MAX, RATIO_MAX)
}

/// The pace figure in pieces, in the order they are shown.
///
/// One string, or two when the form asks for both numbers; empty when it asks
/// for none. Only the first piece carries the arrow: the two numbers are the
/// same comparison, so a second arrow would say nothing and crowd the pill.
///
/// The direction always comes from the difference in points, in either unit, so
/// that the arrow, the word in the panel and the colour can never disagree. The
/// ratio is folded to be at least 1 — "1.4 times faster than you have to" reads;
/// "0.7 times slower" does not.
pub fn pace_parts(view: PaceView, used_pct: f64, target_pct: f64) -> Vec<String> {
    if view == PaceView::Off {
        return Vec::new();
    }
    let delta = used_pct - target_pct;
    // Triangles rather than arrows: at pill size a thin up arrow in front of a
    // digit reads as a 1, and a 2.5x overspend printing as "12.5" is the one
    // misreading this cell must not allow.
    let arrow = if delta > 0.5 {
        "▲"
    } else if delta < -0.5 {
        "▼"
    } else {
        ""
    };
    let points = format!("{:.0}%", delta.abs());
    let ratio = {
        let r = pace_ratio(used_pct, target_pct);
        let shown = if r >= 1.0 { r } else { 1.0 / r };
        format!("{shown:.1}×")
    };

    let (first, second) = if view.points_first() {
        (points, ratio)
    } else {
        (ratio, points)
    };
    let mut out = vec![format!("{arrow}{first}")];
    if view.has_points() && view.has_ratio() {
        out.push(second);
    }
    out
}

/// The same pieces as one string, joined the way the form asks. What the pill
/// shows unless the form wants a rule painted between them, and what the
/// terminal line and the panel always show.
pub fn pace_text(view: PaceView, used_pct: f64, target_pct: f64) -> String {
    pace_parts(view, used_pct, target_pct).join(view.joiner())
}

/// `26` / `0.4` — a rate, with a decimal only while it needs one. A weekly
/// figure of half a percent a day must not print as `0`.
/// A token count in the width of a figure: `142k`, `1.0M`.
///
/// Never more than four glyphs, because the cell reserves exactly that much and
/// the pill must not change width as the number grows (A04). Below a thousand
/// the count is spelled out, which only happens in the first seconds of a
/// session.
pub fn fmt_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{}k", n / 1_000)
    } else {
        format!("{n}")
    }
}

pub fn fmt_rate(per_unit: f64) -> String {
    let v = per_unit.max(0.0);
    if v < 10.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.0}")
    }
}

/// Where usage *should* be if the window were consumed at an even rate.
pub fn pace_target(resets_at: i64, window_secs: i64, now: i64) -> f64 {
    let elapsed = window_secs - (resets_at - now);
    ((elapsed as f64 / window_secs as f64) * 100.0).clamp(0.0, 100.0)
}

/// Verdict on consumption speed, for colour and for the tooltip wording.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pace {
    Ahead,
    OnTrack,
    Behind,
}

impl Pace {
    /// The verdict for a machine: stable, unlocalised, safe to branch on.
    /// `describe` is the wording a person reads and may be reworded.
    pub fn key(self) -> &'static str {
        match self {
            Pace::Ahead => "ahead",
            Pace::OnTrack => "on_track",
            Pace::Behind => "behind",
        }
    }

    pub fn describe(self, lang: &str) -> &'static str {
        let ru = lang.eq_ignore_ascii_case("ru");
        match self {
            Pace::Ahead if ru => "с запасом",
            Pace::OnTrack if ru => "по графику",
            Pace::Behind if ru => "перерасход",
            Pace::Ahead => "under pace",
            Pace::OnTrack => "on pace",
            Pace::Behind => "over pace",
        }
    }
}

/// Any usage above the even-rate target is an overspend; the tolerance applies
/// only below the line, separating "on pace" from a real surplus.
///
/// The tolerance used to work both ways, which made a small overspend read as
/// "on pace" while the colour beside it had already turned. Under the line a
/// margin is worth having; over it, over is over.
pub fn pace_of(used_pct: f64, target_pct: f64, tolerance: f64) -> Pace {
    let delta = used_pct - target_pct;
    if delta > 0.0 {
        Pace::Behind
    } else if delta < -tolerance {
        Pace::Ahead
    } else {
        Pace::OnTrack
    }
}

/// Local time formatted for the tooltip, e.g. `Tue 18:00`.
pub fn fmt_reset_full(epoch: i64, lang: &str) -> String {
    match Local.timestamp_opt(round_to_minute(epoch), 0).single() {
        Some(dt) => format!("{} {}", weekday_name(dt.weekday(), lang), dt.format("%H:%M")),
        None => "--".to_string(),
    }
}

/// `0:45` / `3:05` / `2:03:00` — a span of seconds, however it is meant.
///
/// Used both for "how long ago this was read" and for "how often to read it".
/// Seconds are always shown: a ticking second is how a glance tells a live
/// reading from one that quietly stopped being refreshed, and a setting of
/// 2 min 30 s must not read as "2 min".
pub fn fmt_duration(secs: i64) -> String {
    let s = secs.max(0);
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{sec:02}")
    } else {
        format!("{m}:{sec:02}")
    }
}

#[allow(dead_code)]
pub fn to_local(epoch: i64) -> Option<DateTime<Local>> {
    Local.timestamp_opt(epoch, 0).single()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_token_count_never_outgrows_its_cell() {
        assert_eq!(super::fmt_tokens(0), "0");
        assert_eq!(super::fmt_tokens(999), "999");
        assert_eq!(super::fmt_tokens(1_000), "1k");
        assert_eq!(super::fmt_tokens(167_820), "167k");
        assert_eq!(super::fmt_tokens(1_000_000), "1.0M");
        for n in [0u64, 1, 999, 1_000, 167_820, 999_999, 1_000_000] {
            assert!(
                super::fmt_tokens(n).chars().count() <= 4,
                "{n} does not fit the reserved width"
            );
        }
    }

    use super::*;

    #[test]
    fn remaining_formats_as_hours_and_minutes() {
        assert_eq!(fmt_remaining(3 * 3600 + 37 * 60), "3:37");
        assert_eq!(fmt_remaining(0), "0:00");
        assert_eq!(fmt_remaining(-500), "0:00");
    }

    #[test]
    fn remaining_rounds_up_to_the_whole_minute() {
        // 2:19:40 left is nearer to 2:20 than to 2:19, and it is what the app
        // being mirrored shows.
        assert_eq!(fmt_remaining(2 * 3600 + 19 * 60 + 40), "2:20");
        assert_eq!(fmt_remaining(2 * 3600 + 19 * 60), "2:19");
        assert_eq!(fmt_remaining(40), "0:01");
        assert_eq!(fmt_remaining(3600 - 1), "1:00");
    }

    #[test]
    fn the_reset_clock_rounds_to_the_nearest_minute() {
        // Build a local moment, then push it off the minute in both directions.
        let base = Local
            .with_ymd_and_hms(2026, 2, 8, 0, 10, 0)
            .single()
            .unwrap()
            .timestamp();
        assert_eq!(fmt_clock(base), "00:10");
        assert_eq!(fmt_clock(base - 20), "00:10");
        assert_eq!(fmt_clock(base - 40), "00:09");
        assert_eq!(fmt_clock(base + 40), "00:11");
    }

    #[test]
    fn allowance_spreads_the_rest_over_the_days_shown() {
        // 54% used with 3 days on the pill -> 46/3 per day, and nothing else.
        let got = daily_allowance(54.0, 3);
        assert!((got - 46.0 / 3.0).abs() < 1e-9, "got {got}");
    }

    #[test]
    fn allowance_is_finite_when_no_days_are_counted() {
        // Both ends switched off on the reset day: everything left is for now.
        assert_eq!(daily_allowance(40.0, 0), 60.0);
        assert!(daily_allowance(0.0, 0).is_finite());
    }

    #[test]
    fn allowance_is_zero_when_the_window_is_spent() {
        assert_eq!(daily_allowance(100.0, 5), 0.0);
        assert_eq!(daily_allowance(140.0, 5), 0.0);
    }

    #[test]
    fn the_ratio_is_time_left_against_quota_left() {
        // Five-hour window, two hours to go, a fifth of the quota left: each
        // remaining percent has to last twice as long as an even burn allows.
        let target = pace_target(2 * 3600, FIVE_HOUR_SECS, 0);
        assert!((target - 60.0).abs() < 1e-9, "elapsed target {target}");
        assert!((pace_ratio(80.0, target) - 2.0).abs() < 1e-9);
        // The same fifth with one hour to go is exactly even.
        let target = pace_target(3600, FIVE_HOUR_SECS, 0);
        assert!((pace_ratio(80.0, target) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_ratio_stays_finite_at_both_ends() {
        // Nothing left to spend: capped rather than infinite.
        assert_eq!(pace_ratio(100.0, 50.0), RATIO_MAX);
        assert_eq!(pace_ratio(140.0, 50.0), RATIO_MAX);
        // No time left, quota to spare: capped the other way.
        assert!((pace_ratio(10.0, 100.0) - 1.0 / RATIO_MAX).abs() < 1e-9);
        assert!(pace_ratio(100.0, 100.0).is_finite());
    }

    #[test]
    fn every_form_says_the_same_thing_its_own_way() {
        // 80% spent with 60% of the window gone: over pace, whichever form.
        let p = |view| pace_text(view, 80.0, 60.0);
        assert_eq!(p(PaceView::Ratio), "▲2.0×");
        assert_eq!(p(PaceView::Points), "▲20%");
        assert_eq!(p(PaceView::RatioPoints), "▲2.0× 20%");
        assert_eq!(p(PaceView::PointsRatio), "▲20% 2.0×");
        assert_eq!(p(PaceView::Split), "▲20% 2.0×");
        // The pipe is written and the spaces are not, so the pair reads as one
        // word; the split form has nothing to write and falls back to a space.
        assert_eq!(p(PaceView::Piped), "▲20%|2.0×");
        assert_eq!(p(PaceView::Off), "");
    }

    #[test]
    fn only_the_first_part_carries_the_arrow() {
        let parts = pace_parts(PaceView::RatioPoints, 80.0, 60.0);
        assert_eq!(parts.len(), 2);
        assert!(parts[0].starts_with('▲'));
        assert!(!parts[1].contains('▲') && !parts[1].contains('▼'));
        assert!(pace_parts(PaceView::Off, 80.0, 60.0).is_empty());
        assert_eq!(pace_parts(PaceView::Ratio, 80.0, 60.0).len(), 1);
    }

    #[test]
    fn under_pace_folds_the_ratio_to_read_forwards() {
        assert_eq!(pace_text(PaceView::RatioPoints, 40.0, 60.0), "▼1.5× 20%");
    }

    #[test]
    fn an_even_burn_has_no_arrow() {
        assert_eq!(pace_text(PaceView::Ratio, 50.0, 50.0), "1.0×");
        assert_eq!(pace_text(PaceView::Points, 50.0, 50.0), "0%");
    }

    #[test]
    fn a_rate_keeps_its_decimal_while_it_needs_one() {
        assert_eq!(fmt_rate(26.4), "26");
        assert_eq!(fmt_rate(9.94), "9.9");
        assert_eq!(fmt_rate(0.42), "0.4");
        assert_eq!(fmt_rate(-1.0), "0.0");
    }

    #[test]
    fn target_tracks_elapsed_share_of_the_window() {
        let now = 1_700_000_000;
        // Half of a five-hour window gone.
        let t = pace_target(now + FIVE_HOUR_SECS / 2, FIVE_HOUR_SECS, now);
        assert!((t - 50.0).abs() < 1e-9, "got {t}");
    }

    #[test]
    fn pace_verdicts() {
        assert_eq!(pace_of(60.0, 50.0, 5.0), Pace::Behind);
        assert_eq!(pace_of(40.0, 50.0, 5.0), Pace::Ahead);
        // Just over the line is already an overspend, so the word agrees with
        // the colour; just under it is still on pace.
        assert_eq!(pace_of(51.0, 50.0, 5.0), Pace::Behind);
        assert_eq!(pace_of(50.0, 50.0, 5.0), Pace::OnTrack);
        assert_eq!(pace_of(47.0, 50.0, 5.0), Pace::OnTrack);
    }

    /// The epoch of a local wall-clock moment `days` calendar days from `now`.
    /// Built through the calendar rather than by adding seconds so the test
    /// says the same thing in every time zone, including across a DST change.
    fn local_days_ahead(now: i64, days: u64, hour: u32) -> i64 {
        let base = Local.timestamp_opt(now, 0).single().unwrap();
        (base.date_naive() + chrono::Days::new(days))
            .and_hms_opt(hour, 0, 0)
            .unwrap()
            .and_local_timezone(Local)
            .earliest()
            .unwrap()
            .timestamp()
    }

    #[test]
    fn days_until_counts_calendar_days() {
        let now = 1_770_000_000;
        let d = DayCount::default();
        // Thursday to the Tuesday after next week reads as five days no matter
        // what hour the reset lands on.
        assert_eq!(days_until(local_days_ahead(now, 5, 3), now, d), 5);
        assert_eq!(days_until(local_days_ahead(now, 5, 22), now, d), 5);
        assert_eq!(days_until(local_days_ahead(now, 1, 4), now, d), 1);
    }

    #[test]
    fn both_ends_of_the_run_are_settings() {
        let now = 1_770_000_000;
        let reset = local_days_ahead(now, 5, 10);
        let count = |today, reset_day| days_until(reset, now, DayCount { today, reset_day });
        assert_eq!(count(false, true), 5, "tomorrow through the reset day");
        assert_eq!(count(true, true), 6, "today as well");
        assert_eq!(count(false, false), 4, "neither end");
        assert_eq!(count(true, false), 5, "today, but not the reset day");
    }

    #[test]
    fn a_reset_today_or_already_past_is_zero_days() {
        let now = 1_770_000_000;
        let d = DayCount::default();
        assert_eq!(days_until(now, now, d), 0);
        assert_eq!(days_until(now - 3 * 86400, now, d), 0);
        // Today *is* the reset day: it counts only when both ends do.
        let both = DayCount { today: true, reset_day: true };
        assert_eq!(days_until(now, now, both), 1);
        assert_eq!(
            days_until(now, now, DayCount { today: true, reset_day: false }),
            0
        );
    }

    #[test]
    fn days_are_localised() {
        let now = 1_770_000_000;
        let then = local_days_ahead(now, 5, 12);
        let d = DayCount::default();
        assert_eq!(fmt_days_until(then, now, "ru", d), "5д");
        assert_eq!(fmt_days_until(then, now, "en", d), "5d");
    }

    #[test]
    fn durations_always_show_seconds() {
        assert_eq!(fmt_duration(0), "0:00");
        assert_eq!(fmt_duration(45), "0:45");
        assert_eq!(fmt_duration(185), "3:05");
        assert_eq!(fmt_duration(3599), "59:59");
        assert_eq!(fmt_duration(3600), "1:00:00");
        assert_eq!(fmt_duration(7380), "2:03:00");
        assert_eq!(fmt_duration(-5), "0:00");
    }

    #[test]
    fn a_poll_interval_is_not_rounded_down_to_the_minute() {
        // The bug this replaced: 150 s of polling was labelled "2 min".
        assert_eq!(fmt_duration(150), "2:30");
        assert_eq!(fmt_duration(30), "0:30");
        assert_eq!(fmt_duration(900), "15:00");
    }

    #[test]
    fn weekday_is_localised() {
        assert_eq!(weekday_name(Weekday::Tue, "en"), "Tue");
        assert_eq!(weekday_name(Weekday::Tue, "ru"), "Вт");
    }
}
