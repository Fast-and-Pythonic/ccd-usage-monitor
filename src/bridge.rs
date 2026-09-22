//! `ccd-usage-monitor --bridge`: the command Claude Code runs as its status line.
//!
//! It reads one JSON payload from stdin, folds it into the shared state file
//! for the widget, and prints the same figures back so the terminal client gets
//! a useful status line out of the deal.

use std::io::Read;

use crate::config::Config;
use crate::pace;
use crate::state::{self, StatusLinePayload};

const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
const GREEN: &str = "\x1b[38;5;114m";
const AMBER: &str = "\x1b[38;5;179m";
const RED: &str = "\x1b[38;5;174m";
/// Where the pace scale starts once spending is over the line; the amber above
/// sits too close to green for a small overspend to read as one.
const ORANGE: &str = "\x1b[38;5;215m";

pub fn run() -> i32 {
   let mut input = String::new();
   if std::io::stdin().read_to_string(&mut input).is_err() {
      return 0;
   }
   let payload: StatusLinePayload = match serde_json::from_str(&input) {
      Ok(p) => p,
      // A malformed payload is not worth an error line in the user's status
      // bar; stay silent and let the next tick correct it.
      Err(_) => return 0,
   };

   let now = state::now();
   let _ = state::update(|s| payload.apply_to(s, now));

   let cfg = Config::load();
   let snapshot = state::read().to_snapshot();
   println!("{}", render(&snapshot, &cfg, now));
   0
}

/// The pace figure, coloured the way the pill colours it: on the gap between
/// spending and the clock, never on the unit it is said in.
fn pace_cell(w: crate::model::Window, span: i64, now: i64, cfg: &Config) -> String {
   if cfg.pace_view == crate::config::PaceView::Off {
      return String::new();
   }
   let Some(resets_at) = w.resets_at else {
      return String::new();
   };
   let target = pace::pace_target(resets_at, span, now);
   let delta = w.used_pct - target;
   let tint = if delta <= 0.0 {
      GREEN
   } else if delta <= cfg.pace_tolerance.max(1.0) {
      ORANGE
   } else {
      RED
   };
   // The separator is grey here for the same reason it is grey on the pill:
   // it is punctuation, not a reading. For every form but the piped one the
   // joiner is a space, where a colour change shows nothing either way.
   let sep = format!("{RESET}{DIM}{}{RESET}{tint}", cfg.pace_view.joiner());
   format!(
      " {tint}{}{RESET}",
      pace::pace_parts(cfg.pace_view, w.used_pct, target, span, &cfg.lang).join(&sep)
   )
}

fn colour(pct: f64, cfg: &Config) -> &'static str {
   if pct >= cfg.danger_pct {
      RED
   } else if pct >= cfg.warn_pct {
      AMBER
   } else {
      GREEN
   }
}

/// The same line the widget draws, in ANSI.
pub fn render(snap: &crate::model::Snapshot, cfg: &Config, now: i64) -> String {
   let mut parts: Vec<String> = Vec::new();
   // The status line is as narrow as the pill, so it follows the same setting:
   // one number, spent or left. The colour keeps to spending regardless.
   let shown = |v: f64| cfg.pct_mode.shown(v);

   match snap.five_hour {
      Some(w) => parts.push(format!(
         "{}{:.0}%{}{} {} {}{}{}",
         colour(w.used_pct, cfg),
         shown(w.used_pct),
         RESET,
         // Right after the percentage, as on the pill: `pace_cell` brings
         // its own leading space, or nothing at all when it is switched off.
         pace_cell(w, pace::FIVE_HOUR_SECS, now, cfg),
         w.resets_at
            .map(|r| pace::fmt_remaining(r - now))
            .unwrap_or_else(|| "--:--".into()),
         DIM,
         w.resets_at
            .map(pace::fmt_clock)
            .unwrap_or_else(|| "--:--".into()),
         RESET,
      )),
      None => parts.push(format!("{DIM}--%{RESET}")),
   }

   match snap.seven_day {
      Some(w) => parts.push(format!(
         "{}{:.0}%{}{} {}{}{} {}",
         colour(w.used_pct, cfg),
         shown(w.used_pct),
         RESET,
         pace_cell(w, pace::SEVEN_DAY_SECS, now, cfg),
         DIM,
         w.resets_at
            .map(|r| pace::fmt_days_until(r, now, &cfg.lang, cfg.day_count()))
            .unwrap_or_else(|| "--".into()),
         RESET,
         w.resets_at
            .map(|r| {
               format!(
                  "{:.0}%",
                  pace::daily_allowance(w.used_pct, pace::days_until(r, now, cfg.day_count()))
               )
            })
            .unwrap_or_else(|| "--%".into()),
      )),
      None => parts.push(format!("{DIM}--%{RESET}")),
   }

   if let Some(ctx) = &snap.context {
      // The same forms the pill offers, so one number cannot be shown two
      // different ways in the two places it appears. `ctx` reads by its own
      // direction, not `pct_mode`: a context window fills, it does not
      // drain like the two quotas above it.
      let view = cfg.ctx_view;
      let pct = format!("{:.0}%", cfg.ctx_pct_dir.shown(ctx.used_pct));
      let tokens = match (ctx.used_tokens, cfg.ctx_tokens_dir()) {
         (Some(t), crate::config::PctMode::Spent) => Some(pace::fmt_tokens(t)),
         (Some(t), crate::config::PctMode::Left) => ctx
            .limit_tokens
            .map(|l| pace::fmt_tokens(l.saturating_sub(t))),
         (None, _) => None,
      };
      let figures: Vec<String> = match (view.tokens_first(), view.has_pct(), view.has_tokens()) {
         (_, false, false) => Vec::new(),
         (true, true, _) => tokens.into_iter().chain([pct]).collect(),
         (true, false, _) => tokens.into_iter().collect(),
         (false, _, true) => [pct].into_iter().chain(tokens).collect(),
         (false, _, false) => vec![pct],
      };
      if !figures.is_empty() {
         parts.push(format!("{DIM}ctx{RESET} {}", figures.join(" ")));
      }
   }

   parts.join(&format!(" {DIM}|{RESET} "))
}
