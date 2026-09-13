//! A rolling log of readings, and the burn rate read out of it.
//!
//! Every source tells us where usage stands *now*; none of them says how fast
//! it got there. That answer needs a memory, so the widget keeps one: a small
//! file of `(time, five-hour %, seven-day %)` next to the state file.
//!
//! The log is seeded from the Claude desktop app's own `plan-usage-history.json`
//! (see [`crate::desktop`]), which is a month of samples at a quarter-hour
//! cadence. That import is deliberately not tied to the desktop *source*
//! switch: the switch decides whose figures are shown right now, while this is
//! a one-off read of a file that is already on disk, so a fresh install can
//! answer "how fast" on its first minute instead of its first hour.

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;
/// Long enough to cover a weekly window and the lookback behind it.
const KEEP_SECS: i64 = 8 * 86400;
/// A reading that has not moved is still written this often, so a quiet hour
/// reads as "nothing spent" rather than as "nothing known".
const HEARTBEAT_SECS: i64 = 300;
/// Below this the two ends of a span are too close together for their
/// difference to mean anything.
const MIN_SPAN_SECS: i64 = 300;
/// A fall bigger than this inside one window is a reset we did not witness.
const RESET_DROP: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sample {
   /// Unix epoch seconds.
   pub t: i64,
   /// Five-hour window, percent.
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub fh: Option<f64>,
   /// Seven-day window, percent.
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub sd: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Log {
   pub version: u32,
   #[serde(default)]
   pub samples: Vec<Sample>,
}

impl Default for Log {
   fn default() -> Self {
      Log {
         version: VERSION,
         samples: Vec::new(),
      }
   }
}

pub fn path() -> PathBuf {
   crate::state::state_dir().join("samples.json")
}

pub fn load() -> Log {
   match fs::read_to_string(path()) {
      Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
      Err(_) => Log::default(),
   }
}

/// Written whole and swapped in, like the state file: a reader that catches the
/// widget mid-write would otherwise see half a document.
pub fn save(log: &Log) {
   let dir = crate::state::state_dir();
   if fs::create_dir_all(&dir).is_err() {
      return;
   }
   let tmp = dir.join(format!("samples.{}.tmp", std::process::id()));
   let Ok(text) = serde_json::to_string(log) else {
      return;
   };
   let written = fs::File::create(&tmp).and_then(|mut f| {
      f.write_all(text.as_bytes())?;
      f.sync_all()
   });
   if written.is_ok() {
      let _ = fs::rename(&tmp, path());
   } else {
      let _ = fs::remove_file(&tmp);
   }
}

fn same(a: Option<f64>, b: Option<f64>) -> bool {
   match (a, b) {
      (Some(x), Some(y)) => (x - y).abs() < 1e-6,
      (None, None) => true,
      _ => false,
   }
}

impl Log {
   /// Note a reading. Returns whether the log changed and is worth writing.
   ///
   /// A run of identical readings is kept as its two ends rather than as every
   /// point in between: the value is a step function, so the middle of a run
   /// carries nothing, and an idle night would otherwise cost a few hundred
   /// samples.
   pub fn record(&mut self, now: i64, fh: Option<f64>, sd: Option<f64>) -> bool {
      if fh.is_none() && sd.is_none() {
         return false;
      }
      let n = self.samples.len();
      let matches = |s: &Sample| same(s.fh, fh) && same(s.sd, sd);

      if n >= 1 && matches(&self.samples[n - 1]) {
         if now - self.samples[n - 1].t < HEARTBEAT_SECS {
            return false;
         }
         if n >= 2 && matches(&self.samples[n - 2]) {
            // Already a run: move its far end instead of growing it.
            self.samples[n - 1].t = now;
            return true;
         }
      }
      self.samples.push(Sample { t: now, fh, sd });
      true
   }

   /// Fold in samples from elsewhere, keeping what we already have. Returns
   /// whether anything was new.
   pub fn merge(&mut self, incoming: impl IntoIterator<Item = Sample>) -> bool {
      let known: HashSet<i64> = self.samples.iter().map(|s| s.t).collect();
      let before = self.samples.len();
      for s in incoming {
         if !known.contains(&s.t) {
            self.samples.push(s);
         }
      }
      if self.samples.len() == before {
         return false;
      }
      self.samples.sort_by_key(|s| s.t);
      self.samples.dedup_by_key(|s| s.t);
      true
   }

   pub fn prune(&mut self, now: i64) {
      self
         .samples
         .retain(|s| now - s.t <= KEEP_SECS && s.t <= now);
   }

   /// Percent per hour, over the tail of the current window.
   ///
   /// `window_start` bounds the search: percentages fall back to zero at a
   /// reset, and a rate measured across one would be nonsense. Inside that
   /// bound the last `lookback` seconds are used, anchored at their start by
   /// the last reading before them — the value held until something moved it,
   /// which is what makes an idle hour read as `0`/h instead of as silence.
   pub fn rate(
      &self,
      pick: impl Fn(&Sample) -> Option<f64>,
      window_start: i64,
      now: i64,
      lookback: i64,
   ) -> Option<f64> {
      let from = window_start.max(now - lookback);
      let mut pts: Vec<(i64, f64)> = self
         .samples
         .iter()
         .filter(|s| s.t >= from && s.t <= now)
         .filter_map(|s| pick(s).map(|v| (s.t, v)))
         .collect();
      pts.sort_by_key(|p| p.0);

      // Only when the range does not already begin at its own start: an
      // anchor on top of a real point would collapse the span to nothing.
      if pts.first().is_none_or(|p| p.0 > from) {
         if let Some(v) = self
            .samples
            .iter()
            .filter(|s| s.t < from && s.t >= window_start)
            .filter_map(|s| pick(s).map(|v| (s.t, v)))
            .max_by_key(|p| p.0)
            .map(|p| p.1)
         {
            pts.insert(0, (from, v));
         }
      }

      // A fall means a reset landed inside the range after all; everything
      // before it belongs to a window that is over.
      if let Some(i) = (1..pts.len())
         .rev()
         .find(|&i| pts[i].1 < pts[i - 1].1 - RESET_DROP)
      {
         pts.drain(..i);
      }

      let first = *pts.first()?;
      let last = *pts.last()?;
      let span = last.0 - first.0;
      if span < MIN_SPAN_SECS {
         return None;
      }
      Some(((last.1 - first.1) / (span as f64 / 3600.0)).max(0.0))
   }
}

/// When the window hits 100% if the rate holds. `None` while nothing is being
/// spent — an unbounded answer is not a forecast.
pub fn runs_out_at(used_pct: f64, rate_per_hour: f64, now: i64) -> Option<i64> {
   if rate_per_hour <= 0.01 {
      return None;
   }
   let left = (100.0 - used_pct).max(0.0);
   Some(now + (left / rate_per_hour * 3600.0) as i64)
}

#[cfg(test)]
mod tests {
   use super::*;

   fn log(points: &[(i64, f64)]) -> Log {
      Log {
         version: VERSION,
         samples: points
            .iter()
            .map(|(t, v)| Sample {
               t: *t,
               fh: Some(*v),
               sd: None,
            })
            .collect(),
      }
   }

   const FIVE_H: i64 = 5 * 3600;

   #[test]
   fn the_rate_is_the_two_ends_of_the_span() {
      // 20% in half an hour is 40%/h, and the middle point does not change it.
      let l = log(&[(0, 10.0), (900, 22.0), (1800, 30.0)]);
      let got = l.rate(|s| s.fh, 0, 1800, 3600).unwrap();
      assert!((got - 40.0).abs() < 1e-9, "got {got}");
   }

   #[test]
   fn a_reading_that_has_not_moved_reads_as_zero() {
      let l = log(&[(0, 30.0), (3600, 30.0)]);
      assert_eq!(l.rate(|s| s.fh, 0, 3600, 3600), Some(0.0));
   }

   #[test]
   fn a_value_held_from_before_the_lookback_still_anchors_it() {
      // The last change was three hours ago and the run has been ticking
      // along unchanged since: the last hour spent nothing, and that is a
      // fact about the hour, not a hole in the record.
      let l = log(&[(0, 30.0), (10_700, 30.0)]);
      assert_eq!(l.rate(|s| s.fh, 0, 10_800, 3600), Some(0.0));
   }

   #[test]
   fn a_log_that_stops_before_the_range_says_nothing() {
      // Nothing written for the last hour means the widget was not running,
      // not that the hour was idle. One point is not a span.
      let l = log(&[(0, 30.0), (7200, 30.0)]);
      assert_eq!(l.rate(|s| s.fh, 0, 10_800, 3600), None);
   }

   #[test]
   fn a_reset_inside_the_range_cuts_off_what_came_before() {
      // 80% then a reset to 0, then 10% in an hour.
      let l = log(&[(0, 80.0), (600, 0.0), (4200, 10.0)]);
      let got = l.rate(|s| s.fh, 0, 4200, 2 * 3600).unwrap();
      assert!((got - 10.0).abs() < 1e-9, "got {got}");
   }

   #[test]
   fn nothing_to_measure_says_so() {
      assert_eq!(Log::default().rate(|s| s.fh, 0, 3600, 3600), None);
      // One point, or two too close together, is not a span.
      assert_eq!(log(&[(3500, 10.0)]).rate(|s| s.fh, 0, 3600, 3600), None);
      assert_eq!(
         log(&[(3400, 10.0), (3500, 12.0)]).rate(|s| s.fh, 3300, 3600, 3600),
         None
      );
   }

   #[test]
   fn a_previous_window_is_out_of_reach() {
      // The window began at 3600; the reading before it belongs to the last.
      let l = log(&[(0, 90.0), (4000, 5.0), (7200, 20.0)]);
      let got = l.rate(|s| s.fh, 3600, 7200, 8 * 3600).unwrap();
      assert!((got - 15.0 / (3200.0 / 3600.0)).abs() < 1e-9, "got {got}");
   }

   #[test]
   fn the_forecast_answers_only_while_something_is_being_spent() {
      // 40% left at 20%/h runs out in two hours.
      assert_eq!(runs_out_at(60.0, 20.0, 1000), Some(1000 + 2 * 3600));
      assert_eq!(runs_out_at(60.0, 0.0, 1000), None);
      // Already spent: the answer is now, not never.
      assert_eq!(runs_out_at(100.0, 20.0, 1000), Some(1000));
   }

   #[test]
   fn a_run_of_the_same_reading_is_kept_as_its_two_ends() {
      let mut l = Log::default();
      assert!(l.record(0, Some(30.0), None));
      // Inside the heartbeat nothing is written at all.
      assert!(!l.record(60, Some(30.0), None));
      assert!(l.record(HEARTBEAT_SECS, Some(30.0), None));
      assert!(l.record(2 * HEARTBEAT_SECS, Some(30.0), None));
      assert_eq!(l.samples.len(), 2, "{:?}", l.samples);
      assert_eq!(l.samples[0].t, 0, "the run keeps where it began");
      assert_eq!(l.samples[1].t, 2 * HEARTBEAT_SECS);
      // A change always lands, heartbeat or not.
      assert!(l.record(2 * HEARTBEAT_SECS + 5, Some(31.0), None));
      assert_eq!(l.samples.len(), 3);
   }

   #[test]
   fn a_reading_with_nothing_in_it_is_not_a_sample() {
      let mut l = Log::default();
      assert!(!l.record(0, None, None));
      assert!(l.samples.is_empty());
   }

   #[test]
   fn merging_keeps_what_we_had_and_sorts_the_rest() {
      let mut l = log(&[(100, 5.0)]);
      let incoming = vec![
         Sample {
            t: 50,
            fh: Some(1.0),
            sd: Some(2.0),
         },
         Sample {
            t: 100,
            fh: Some(99.0),
            sd: None,
         },
      ];
      assert!(l.merge(incoming.clone()));
      assert_eq!(l.samples.len(), 2);
      assert_eq!(l.samples[0].t, 50);
      assert_eq!(l.samples[1].fh, Some(5.0), "ours wins on a shared moment");
      assert!(!l.merge(incoming), "nothing new the second time");
   }

   #[test]
   fn pruning_drops_the_old_and_the_impossible() {
      let now = 10 * 86400;
      let mut l = log(&[(0, 1.0), (now - 3600, 2.0), (now + 600, 3.0)]);
      l.prune(now);
      assert_eq!(l.samples.len(), 1);
      assert_eq!(l.samples[0].t, now - 3600);
   }

   #[test]
   fn a_span_of_exactly_five_minutes_counts() {
      let l = log(&[(0, 10.0), (MIN_SPAN_SECS, 15.0)]);
      assert!(l.rate(|s| s.fh, 0, MIN_SPAN_SECS, FIVE_H).is_some());
   }
}
