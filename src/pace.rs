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

/// The clock every figure worked out *from the time* reads: the start of the
/// current minute.
///
/// The pill is redrawn four times a second, but a pace, a day count or a
/// forecast read off the exact second each changes whenever its own rounding
/// happens to cross — a pace in time form half a minute out of step with the
/// countdown beside it, a colour sliding on every tick (a repaint each time,
/// G23). Read off the minute, they all step together, once, as the system
/// clock turns over (A30). Ages and the next-poll countdown keep the second:
/// a ticking second is how a live reading is told from a stuck one.
pub fn on_the_minute(now: i64) -> i64 {
   now.div_euclid(60) * 60
}

fn round_to_minute(epoch: i64) -> i64 {
   epoch.div_euclid(60) * 60 + if epoch.rem_euclid(60) >= 30 { 60 } else { 0 }
}

/// How the days left in the weekly window are counted.
///
/// By the calendar, neither end is obvious. Today is usually half spent by the
/// time anyone looks, and the day the window resets is worth only the hours
/// before the reset — with a reset at 07:00 it is worth almost nothing. Both
/// ends are therefore a setting rather than a decision made here.
///
/// By time there is nothing to decide: the days are the exact time from now to
/// the reset, `4д7ч`, and the allowance divides by that (A29).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DayCount {
   Calendar { today: bool, reset_day: bool },
   Time,
}

impl Default for DayCount {
   fn default() -> Self {
      DayCount::Calendar {
         today: false,
         reset_day: true,
      }
   }
}

/// How many days the rest of the weekly quota has to be spread over.
///
/// By the calendar, counted on local dates rather than by dividing seconds:
/// the count is the length of a run of days — from today or tomorrow, up to
/// the reset day or the one before it — so Thursday with a reset on Tuesday
/// gives 5 by default (Fri…Tue), 6 with today, 4 without the reset day. Always
/// a whole number.
///
/// By time, the seconds to the reset over a day's worth of them, fraction and
/// all.
pub fn days_until(resets_at: i64, now: i64, count: DayCount) -> f64 {
   let DayCount::Calendar { today, reset_day } = count else {
      return (resets_at - on_the_minute(now)).max(0) as f64 / 86400.0;
   };
   let (Some(reset), Some(start)) = (
      Local.timestamp_opt(resets_at, 0).single(),
      Local.timestamp_opt(now, 0).single(),
   ) else {
      return 0.0;
   };
   let span = (reset.date_naive() - start.date_naive()).num_days();
   let drop_first = i64::from(!today);
   let drop_last = i64::from(!reset_day);
   (span + 1 - drop_first - drop_last).max(0) as f64
}

/// `5д` / `5d` by the calendar, `4д7ч` / `12ч` by time — the day count as it
/// appears on the pill. By time it is written the way a weekly pace gap is,
/// days and whole hours, so the two read alike side by side.
pub fn fmt_days_until(resets_at: i64, now: i64, lang: &str, count: DayCount) -> String {
   if count == DayCount::Time {
      // Rounded up, as the five-hour countdown is: with 4 d 6 h 20 min to go
      // there is more than 4д6ч left, and it reads 0ч only once it is over.
      let secs = (resets_at - on_the_minute(now)).max(0);
      return fmt_days_hours((secs + 3599) / 3600, lang);
   }
   let d = days_until(resets_at, now, count) as i64;
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
   if lang.eq_ignore_ascii_case("ru") {
      RU[i]
   } else {
      EN[i]
   }
}

/// The run of calendar days the count covers, as the names of its first and
/// last day — `(Ср, Вт)` — or `None` when no day is left in it. Only for the
/// calendar count; by time there is no run of days to name.
pub fn day_run(resets_at: i64, now: i64, count: DayCount, lang: &str) -> Option<(String, String)> {
   let DayCount::Calendar { today, reset_day } = count else {
      return None;
   };
   let reset = Local.timestamp_opt(resets_at, 0).single()?.date_naive();
   let start = Local.timestamp_opt(now, 0).single()?.date_naive();
   let first = if today { start } else { start.succ_opt()? };
   let last = if reset_day { reset } else { reset.pred_opt()? };
   (first <= last).then(|| {
      (
         weekday_name(first.weekday(), lang).to_string(),
         weekday_name(last.weekday(), lang).to_string(),
      )
   })
}

/// `вторник, 30 сентября, 10:00` long, `Вт 30 сент., 10:00` short — a reset a
/// day or more away, where the weekday alone leaves the date to be worked out.
pub fn fmt_reset_date(epoch: i64, lang: &str, long: bool) -> String {
   const RU_DAYS: [&str; 7] = [
      "понедельник",
      "вторник",
      "среда",
      "четверг",
      "пятница",
      "суббота",
      "воскресенье",
   ];
   const EN_DAYS: [&str; 7] = [
      "Monday",
      "Tuesday",
      "Wednesday",
      "Thursday",
      "Friday",
      "Saturday",
      "Sunday",
   ];
   const RU_MONTHS: [&str; 12] = [
      "января",
      "февраля",
      "марта",
      "апреля",
      "мая",
      "июня",
      "июля",
      "августа",
      "сентября",
      "октября",
      "ноября",
      "декабря",
   ];
   const RU_MON: [&str; 12] = [
      "янв.",
      "февр.",
      "марта",
      "апр.",
      "мая",
      "июня",
      "июля",
      "авг.",
      "сент.",
      "окт.",
      "нояб.",
      "дек.",
   ];
   const EN_MONTHS: [&str; 12] = [
      "January",
      "February",
      "March",
      "April",
      "May",
      "June",
      "July",
      "August",
      "September",
      "October",
      "November",
      "December",
   ];
   let Some(dt) = Local.timestamp_opt(round_to_minute(epoch), 0).single() else {
      return "--".to_string();
   };
   let (d, m) = (dt.day(), dt.month0() as usize);
   let wd = dt.weekday().num_days_from_monday() as usize;
   let clock = dt.format("%H:%M");
   let ru = lang.eq_ignore_ascii_case("ru");
   match (ru, long) {
      (true, true) => format!("{}, {d} {}, {clock}", RU_DAYS[wd], RU_MONTHS[m]),
      (true, false) => format!(
         "{} {d} {}, {clock}",
         weekday_name(dt.weekday(), lang),
         RU_MON[m]
      ),
      (false, true) => format!("{}, {d} {}, {clock}", EN_DAYS[wd], EN_MONTHS[m]),
      (false, false) => format!(
         "{} {d} {}, {clock}",
         weekday_name(dt.weekday(), lang),
         &EN_MONTHS[m][..3]
      ),
   }
}

/// `4 д 7 ч` / `3 ч 37 мин` / `37 мин` — a span in words, for a hint rather
/// than a cell. Rounded up like the countdown: the minute under way counts,
/// and past a day the hour under way does.
pub fn fmt_span_words(secs: i64, lang: &str) -> String {
   let s = secs.max(0);
   let ru = lang.eq_ignore_ascii_case("ru");
   let (du, hu, mu) = if ru {
      ("д", "ч", "мин")
   } else {
      ("d", "h", "min")
   };
   if s >= 86400 {
      let hours = (s + 3599) / 3600;
      let (d, h) = (hours / 24, hours % 24);
      return if h > 0 {
         format!("{d} {du} {h} {hu}")
      } else {
         format!("{d} {du}")
      };
   }
   let mins = (s + 59) / 60;
   let (h, m) = (mins / 60, mins % 60);
   match (h, m) {
      (0, m) => format!("{m} {mu}"),
      (h, 0) => format!("{h} {hu}"),
      (h, m) => format!("{h} {hu} {m} {mu}"),
   }
}

/// How much of the weekly limit may be spent per day to finish exactly at 100%.
/// Returns percent-per-day.
///
/// `days` is the figure shown beside it (`days_until`), not something worked
/// out afresh: the two numbers sit next to each other on the pill and must
/// agree. By time that holds under a day too — half a day and 40% left is
/// `80%` a day, which is what makes `12ч 80%` read right. Capped at 100 so the
/// cell keeps its width in the last minutes; with nothing left to divide by,
/// everything left is for now.
pub fn daily_allowance(used_pct: f64, days: f64) -> f64 {
   let left_pct = (100.0 - used_pct).max(0.0);
   if days <= 0.0 {
      return left_pct.min(100.0);
   }
   (left_pct / days).min(100.0)
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

/// How much of the window a gap of `delta_pct` points is worth, in seconds.
///
/// The gap is a share of the window, so it converts at the window's own rate:
/// 1% of five hours is three minutes, 1% of a week is 100.8. Capped at the whole
/// window, which is as far ahead of the clock as spending can meaningfully get.
pub fn pace_secs(delta_pct: f64, span: i64) -> i64 {
   (delta_pct.abs().min(100.0) * span as f64 / 100.0).round() as i64
}

/// `0:27` / `3д2ч` — a pace gap in time, in the grain its window is read in.
///
/// A five-hour window is watched in minutes, so it gets `h:mm` like the
/// countdown beside it. A week is not: `73:55` is a number to do arithmetic on,
/// and minutes are noise at that scale, so it gets days and whole hours.
pub fn fmt_pace_time(secs: i64, span: i64, lang: &str) -> String {
   let s = secs.max(0);
   if span <= 86400 {
      let mins = (s + 30) / 60;
      return format!("{}:{:02}", mins / 60, mins % 60);
   }
   fmt_days_hours(pace_hours(s), lang)
}

/// `3д2ч` / `5ч` — whole hours, with a day count once there is one.
fn fmt_days_hours(hours: i64, lang: &str) -> String {
   let (d, h) = (hours / 24, hours % 24);
   let (du, hu) = if lang.eq_ignore_ascii_case("ru") {
      ("д", "ч")
   } else {
      ("d", "h")
   };
   if d > 0 {
      format!("{d}{du}{h}{hu}")
   } else {
      format!("{h}{hu}")
   }
}

/// A weekly gap in whole hours, rounded to the nearest — the grain `3д2ч` is
/// written in, and what decides whether it has a day count at all.
pub fn pace_hours(secs: i64) -> i64 {
   (secs.max(0) + 1800) / 3600
}

/// The direction of a gap, as the one arrow a figure carries.
///
/// Triangles rather than arrows: at pill size a thin up arrow in front of a
/// digit reads as a 1, and a 2.5x overspend printing as "12.5" is the one
/// misreading this cell must not allow. Segoe UI has no smaller triangle of
/// its own (`▴`/`▾` draw as tofu), so shrinking it on screen is `view.rs`'s
/// job — a cell of its own at the pill's smaller role — not a smaller glyph
/// here; this stays the one pair of characters everything else measures and
/// searches for.
fn pace_arrow(delta: f64) -> &'static str {
   if delta > 0.5 {
      "▲"
   } else if delta < -0.5 {
      "▼"
   } else {
      ""
   }
}

/// The three renderings of one gap, unarrowed: points, ratio, time.
fn pace_units(used_pct: f64, target_pct: f64, span: i64, lang: &str) -> [String; 3] {
   let delta = used_pct - target_pct;
   let points = format!("{:.0}%", delta.abs());
   let ratio = {
      let r = pace_ratio(used_pct, target_pct);
      let shown = if r >= 1.0 { r } else { 1.0 / r };
      format!("{shown:.1}×")
   };
   let time = fmt_pace_time(pace_secs(delta, span), span, lang);
   [points, ratio, time]
}

/// The pace figure in pieces, in the order they are shown.
///
/// One string, or two when the form asks for both numbers; empty when it asks
/// for none. Only the first piece carries the arrow: the two numbers are the
/// same comparison, so a second arrow would say nothing and crowd the pill.
///
/// The direction always comes from the difference in points, in any unit, so
/// that the arrow, the word in the panel and the colour can never disagree. The
/// ratio is folded to be at least 1 — "1.4 times faster than you have to" reads;
/// "0.7 times slower" does not.
pub fn pace_parts(
   view: PaceView,
   used_pct: f64,
   target_pct: f64,
   span: i64,
   lang: &str,
) -> Vec<String> {
   if view == PaceView::Off {
      return Vec::new();
   }
   let arrow = pace_arrow(used_pct - target_pct);
   let [points, ratio, time] = pace_units(used_pct, target_pct, span, lang);
   if view == PaceView::Time {
      return vec![format!("{arrow}{time}")];
   }

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

/// The same pieces as one string, joined the way the form asks — what the pill
/// shows when the form is a single cell. Only the tests need it whole: the pill
/// and the terminal line each join the pieces themselves.
#[cfg(test)]
pub fn pace_text(view: PaceView, used_pct: f64, target_pct: f64, span: i64, lang: &str) -> String {
   pace_parts(view, used_pct, target_pct, span, lang).join(view.joiner())
}

/// Every unit of the figure, for the panel and the JSON: the pill is where the
/// choice is honoured, the panel is where everything is spelled out. The time
/// form leads with time; every other form says both of its numbers in its own
/// order and puts time last. One arrow either way, on whatever comes first.
pub fn pace_detail(
   view: PaceView,
   used_pct: f64,
   target_pct: f64,
   span: i64,
   lang: &str,
) -> String {
   if view == PaceView::Time {
      let arrow = pace_arrow(used_pct - target_pct);
      let [points, ratio, time] = pace_units(used_pct, target_pct, span, lang);
      return format!("{arrow}{time} {points} {ratio}");
   }
   let mut parts = pace_parts(view.both(), used_pct, target_pct, span, lang);
   parts.push(fmt_pace_time(
      pace_secs(used_pct - target_pct, span),
      span,
      lang,
   ));
   parts.join(" ")
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

/// Where usage *should* be if the window were consumed at an even rate — as of
/// the current minute (`on_the_minute`), so every rendering of the pace, and
/// its colour, moves once a minute and in step with the clock.
pub fn pace_target(resets_at: i64, window_secs: i64, now: i64) -> f64 {
   let elapsed = window_secs - (resets_at - on_the_minute(now));
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
      Some(dt) => format!(
         "{} {}",
         weekday_name(dt.weekday(), lang),
         dt.format("%H:%M")
      ),
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
      let got = daily_allowance(54.0, 3.0);
      assert!((got - 46.0 / 3.0).abs() < 1e-9, "got {got}");
   }

   #[test]
   fn allowance_is_finite_when_no_days_are_counted() {
      // Both ends switched off on the reset day: everything left is for now.
      assert_eq!(daily_allowance(40.0, 0.0), 60.0);
      assert!(daily_allowance(0.0, 0.0).is_finite());
   }

   #[test]
   fn allowance_is_zero_when_the_window_is_spent() {
      assert_eq!(daily_allowance(100.0, 5.0), 0.0);
      assert_eq!(daily_allowance(140.0, 5.0), 0.0);
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
      let p = |view| pace_text(view, 80.0, 60.0, FIVE_HOUR_SECS, "en");
      assert_eq!(p(PaceView::Ratio), "▲2.0×");
      assert_eq!(p(PaceView::Points), "▲20%");
      assert_eq!(p(PaceView::Time), "▲1:00");
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
      let parts = |view| pace_parts(view, 80.0, 60.0, FIVE_HOUR_SECS, "en");
      let two = parts(PaceView::RatioPoints);
      assert_eq!(two.len(), 2);
      assert!(two[0].starts_with('▲'));
      assert!(!two[1].contains('▲') && !two[1].contains('▼'));
      assert!(parts(PaceView::Off).is_empty());
      assert_eq!(parts(PaceView::Ratio).len(), 1);
      assert_eq!(parts(PaceView::Time).len(), 1);
   }

   #[test]
   fn under_pace_folds_the_ratio_to_read_forwards() {
      assert_eq!(
         pace_text(PaceView::RatioPoints, 40.0, 60.0, FIVE_HOUR_SECS, "en"),
         "▼1.5× 20%"
      );
   }

   #[test]
   fn an_even_burn_has_no_arrow() {
      let p = |view| pace_text(view, 50.0, 50.0, FIVE_HOUR_SECS, "en");
      assert_eq!(p(PaceView::Ratio), "1.0×");
      assert_eq!(p(PaceView::Points), "0%");
      assert_eq!(p(PaceView::Time), "0:00");
   }

   #[test]
   fn a_five_hour_gap_is_three_minutes_a_point() {
      // 100% of five hours is 300 minutes, so each point is 3.
      let t = |used, target| pace_text(PaceView::Time, used, target, FIVE_HOUR_SECS, "ru");
      assert_eq!(t(50.0, 41.0), "▲0:27");
      assert_eq!(t(20.0, 64.0), "▼2:12");
      // Capped at the whole window rather than running past it.
      assert_eq!(t(140.0, 10.0), "▲5:00");
   }

   #[test]
   fn a_weekly_gap_is_days_and_whole_hours() {
      let t =
         |delta: f64, lang| fmt_pace_time(pace_secs(delta, SEVEN_DAY_SECS), SEVEN_DAY_SECS, lang);
      // 44 points of a week is 73 h 55 min: three days and two hours.
      assert_eq!(t(44.0, "ru"), "3д2ч");
      assert_eq!(t(-44.0, "en"), "3d2h");
      // Under a day there is no day count to show.
      assert_eq!(t(3.0, "ru"), "5ч");
      assert_eq!(t(0.0, "ru"), "0ч");
      assert_eq!(t(100.0, "en"), "7d0h");
   }

   #[test]
   fn the_panel_spells_out_every_unit_with_one_arrow() {
      let d = |view| pace_detail(view, 80.0, 60.0, FIVE_HOUR_SECS, "en");
      assert_eq!(d(PaceView::Time), "▲1:00 20% 2.0×");
      assert_eq!(d(PaceView::Piped), "▲20% 2.0× 1:00");
      assert_eq!(d(PaceView::Ratio), "▲2.0× 20% 1:00");
      assert_eq!(
         pace_detail(PaceView::Time, 40.0, 60.0, SEVEN_DAY_SECS, "ru"),
         "▼1д10ч 20% 1.5×"
      );
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
      let now = 1_700_000_040;
      // Half of a five-hour window gone.
      let t = pace_target(now + FIVE_HOUR_SECS / 2, FIVE_HOUR_SECS, now);
      assert!((t - 50.0).abs() < 1e-9, "got {t}");
   }

   #[test]
   fn the_pace_steps_once_a_minute_with_the_clock() {
      // A30: anywhere inside one minute the target is the same number, so
      // the figure and its colour hold still; it moves as the minute turns.
      let minute = 1_700_000_040;
      let reset = minute + FIVE_HOUR_SECS / 2 + 17;
      let at = |s| pace_target(reset, FIVE_HOUR_SECS, minute + s);
      assert_eq!(at(0), at(59));
      assert!(at(60) > at(59));
      // In time form the gap now turns over with the countdown beside it,
      // not half a minute after it.
      let gap = |s| pace_text(PaceView::Time, 60.0, at(s), FIVE_HOUR_SECS, "en");
      assert_eq!(gap(0), gap(59));
      assert_ne!(gap(59), gap(60));
   }

   #[test]
   fn by_time_the_day_count_rounds_up_like_a_countdown() {
      let now = 1_770_000_000;
      // 4 d 6 h 20 min is more than 4д6ч, as 2:19:40 is more than 2:19.
      let reset = now + 4 * 86400 + 6 * 3600 + 20 * 60;
      assert_eq!(fmt_days_until(reset, now, "ru", DayCount::Time), "4д7ч");
      // The last minutes of the window are still an hour on the pill.
      assert_eq!(fmt_days_until(now + 120, now, "ru", DayCount::Time), "1ч");
      assert_eq!(fmt_days_until(now, now, "ru", DayCount::Time), "0ч");
      // Held for the minute, like the pace.
      assert_eq!(
         days_until(reset, now + 59, DayCount::Time),
         days_until(reset, now, DayCount::Time)
      );
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
      assert_eq!(days_until(local_days_ahead(now, 5, 3), now, d), 5.0);
      assert_eq!(days_until(local_days_ahead(now, 5, 22), now, d), 5.0);
      assert_eq!(days_until(local_days_ahead(now, 1, 4), now, d), 1.0);
   }

   #[test]
   fn both_ends_of_the_run_are_settings() {
      let now = 1_770_000_000;
      let reset = local_days_ahead(now, 5, 10);
      let count =
         |today, reset_day| days_until(reset, now, DayCount::Calendar { today, reset_day });
      assert_eq!(count(false, true), 5.0, "tomorrow through the reset day");
      assert_eq!(count(true, true), 6.0, "today as well");
      assert_eq!(count(false, false), 4.0, "neither end");
      assert_eq!(count(true, false), 5.0, "today, but not the reset day");
   }

   #[test]
   fn a_reset_today_or_already_past_is_zero_days() {
      let now = 1_770_000_000;
      let d = DayCount::default();
      assert_eq!(days_until(now, now, d), 0.0);
      assert_eq!(days_until(now - 3 * 86400, now, d), 0.0);
      // Today *is* the reset day: it counts only when both ends do.
      let both = DayCount::Calendar {
         today: true,
         reset_day: true,
      };
      assert_eq!(days_until(now, now, both), 1.0);
      assert_eq!(
         days_until(
            now,
            now,
            DayCount::Calendar {
               today: true,
               reset_day: false
            }
         ),
         0.0
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
   fn by_time_the_days_are_the_clock_to_the_reset() {
      let now = 1_770_000_000;
      let reset = now + 4 * 86400 + 7 * 3600;
      assert!((days_until(reset, now, DayCount::Time) - (4.0 + 7.0 / 24.0)).abs() < 1e-9);
      assert_eq!(fmt_days_until(reset, now, "ru", DayCount::Time), "4д7ч");
      assert_eq!(fmt_days_until(reset, now, "en", DayCount::Time), "4d7h");
      // Under a day there is no day count, as with a pace gap.
      assert_eq!(
         fmt_days_until(now + 12 * 3600, now, "ru", DayCount::Time),
         "12ч"
      );
      // A reset already past is nothing left, not a negative span.
      assert_eq!(days_until(now - 600, now, DayCount::Time), 0.0);
   }

   #[test]
   fn by_time_the_allowance_is_the_rest_over_the_time_left() {
      // Half a day and 40% left: 80% a day, so `12ч 80%` reads right.
      assert!((daily_allowance(60.0, 0.5) - 80.0).abs() < 1e-9);
      // The last minutes stay within the cell.
      assert_eq!(daily_allowance(60.0, 0.01), 100.0);
      let now = 1_770_000_000;
      let d = days_until(now + 2 * 86400, now, DayCount::Time);
      assert!((daily_allowance(50.0, d) - 25.0).abs() < 1e-9);
   }

   #[test]
   fn a_reset_date_is_spelled_out_long_and_short() {
      let at = Local
         .with_ymd_and_hms(2026, 9, 29, 10, 0, 0)
         .single()
         .unwrap()
         .timestamp();
      assert_eq!(
         fmt_reset_date(at, "ru", true),
         "вторник, 29 сентября, 10:00"
      );
      assert_eq!(fmt_reset_date(at, "ru", false), "Вт 29 сент., 10:00");
      assert_eq!(
         fmt_reset_date(at, "en", true),
         "Tuesday, 29 September, 10:00"
      );
      assert_eq!(fmt_reset_date(at, "en", false), "Tue 29 Sep, 10:00");
   }

   #[test]
   fn a_span_in_words_rounds_up_like_the_countdown() {
      assert_eq!(fmt_span_words(3 * 3600 + 37 * 60, "ru"), "3 ч 37 мин");
      assert_eq!(fmt_span_words(3 * 3600 + 36 * 60 + 20, "ru"), "3 ч 37 мин");
      assert_eq!(fmt_span_words(37 * 60, "en"), "37 min");
      assert_eq!(fmt_span_words(2 * 3600, "en"), "2 h");
      assert_eq!(
         fmt_span_words(4 * 86400 + 6 * 3600 + 20 * 60, "ru"),
         "4 д 7 ч"
      );
      assert_eq!(fmt_span_words(2 * 86400, "en"), "2 d");
   }

   #[test]
   fn the_run_of_days_names_its_ends() {
      let now = 1_770_000_000;
      let reset = local_days_ahead(now, 5, 10);
      let named =
         |today, reset_day| day_run(reset, now, DayCount::Calendar { today, reset_day }, "en");
      let wd = |days| {
         weekday_name(
            Local
               .timestamp_opt(local_days_ahead(now, days, 12), 0)
               .unwrap()
               .weekday(),
            "en",
         )
         .to_string()
      };
      assert_eq!(named(false, true), Some((wd(1), wd(5))));
      assert_eq!(named(true, false), Some((wd(0), wd(4))));
      assert_eq!(day_run(reset, now, DayCount::Time, "en"), None);
      // Nothing left of the run: the reset is tomorrow and neither end counts.
      let soon = local_days_ahead(now, 1, 10);
      assert_eq!(
         day_run(
            soon,
            now,
            DayCount::Calendar {
               today: false,
               reset_day: false
            },
            "en"
         ),
         None
      );
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
