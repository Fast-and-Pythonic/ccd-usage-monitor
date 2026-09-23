//! Secondary source: the `oauth/usage` endpoint Claude Code itself authenticates
//! against.
//!
//! This endpoint is not documented by Anthropic — it is what the community
//! status-line tools use. Treat every field as optional and every failure as
//! routine: if this stops working the widget keeps running on bridge data.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use serde_json::Value;

use crate::model::{Snapshot, Source, Window};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const OAUTH_BETA: &str = "oauth-2025-04-20";

/// The OAuth token Claude Code maintains for its own requests. We only ever
/// read it; refreshing is Claude Code's job.
pub struct Token {
   pub access_token: String,
   /// Milliseconds since the epoch, as stored by Claude Code.
   pub expires_at_ms: Option<i64>,
}

impl Token {
   pub fn is_expired(&self, now_secs: i64) -> bool {
      match self.expires_at_ms {
         Some(ms) => ms / 1000 <= now_secs,
         None => false,
      }
   }
}

/// Claude Code's own directory — not ours.
///
/// Public because two sources need it now: the credentials this module reads
/// and the session transcripts `session` reads. A module of its own for one
/// function would be worse than the borrow, so it stays here rather than being
/// tidied away later.
pub fn claude_dir() -> Option<std::path::PathBuf> {
   if let Ok(dir) = std::env::var("CLAUDE_CONFIG_DIR")
      && !dir.is_empty()
   {
      return Some(std::path::PathBuf::from(dir));
   }
   Some(dirs::home_dir()?.join(".claude"))
}

pub fn read_token() -> Option<Token> {
   let path = claude_dir()?.join(".credentials.json");
   let text = std::fs::read_to_string(path).ok()?;
   let v: Value = serde_json::from_str(&text).ok()?;
   let oauth = v.get("claudeAiOauth")?;
   Some(Token {
      access_token: oauth.get("accessToken")?.as_str()?.to_string(),
      expires_at_ms: oauth.get("expiresAt").and_then(|e| e.as_i64()),
   })
}

/// The first pause after a 429 that carries no `Retry-After`, and the longest
/// one repeated refusals can grow it to.
pub const PENALTY_MIN_SECS: i64 = 60;
pub const PENALTY_MAX_SECS: i64 = 300;

/// What one request came back with. A rate-limit answer is not an error to be
/// printed and forgotten — it is an instruction to stop asking for a while.
pub enum Fetched {
   Body(String),
   /// `retry_after_secs` is `None` when the answer carried no `Retry-After`
   /// and the length of the pause is ours to choose.
   RateLimited {
      retry_after_secs: Option<i64>,
   },
}

/// How long to stay away after a refusal.
///
/// A 429 with no `Retry-After` says nothing about how long the limit lasts, so
/// guessing five minutes punishes a burst of curiosity far beyond what the
/// endpoint asked for. Start at a minute and double only if the next attempt is
/// refused again; a success resets `previous` to zero.
pub fn next_penalty(previous: i64, retry_after: Option<i64>) -> i64 {
   match retry_after {
      Some(s) if s > 0 => s,
      _ if previous <= 0 => PENALTY_MIN_SECS,
      _ => (previous * 2).min(PENALTY_MAX_SECS),
   }
}

/// When the next request may go out, given the times of the recent ones.
///
/// The budget exists so the endpoint never has to say no: a handful of manual
/// refreshes in a row costs a few seconds of waiting instead of a five-minute
/// penalty. `history` holds request times inside the last minute, oldest first.
pub fn next_slot(history: &[i64], budget_per_min: u32, now: i64) -> i64 {
   let budget = budget_per_min.max(1) as usize;
   if history.len() < budget {
      return now;
   }
   // The oldest request in the window frees its slot a minute after it.
   history[history.len() - budget] + 60
}

/// One request. Returns the raw body so `--probe-api` can print it.
///
/// HTTP statuses are read rather than turned into errors, because the text of
/// `Error::StatusCode(429)` ("http status: 429") is exactly what a user should
/// never have to see in a status widget.
pub fn fetch_raw(token: &str) -> Result<Fetched, String> {
   let mut res = ureq::get(USAGE_URL)
      .header("Authorization", &format!("Bearer {token}"))
      .header("anthropic-beta", OAUTH_BETA)
      .header("Accept", "application/json")
      .config()
      .http_status_as_error(false)
      .build()
      .call()
      .map_err(|e| e.to_string())?;

   let status = res.status().as_u16();
   if status == 429 {
      let retry_after_secs = res
         .headers()
         .get("retry-after")
         .and_then(|v| v.to_str().ok())
         .and_then(|s| s.trim().parse::<i64>().ok())
         .filter(|s| *s > 0);
      return Ok(Fetched::RateLimited { retry_after_secs });
   }

   let body = res.body_mut().read_to_string().map_err(|e| e.to_string())?;
   if !(200..300).contains(&status) {
      // Keep the body out of the message: it may be a whole HTML page.
      return Err(format!("HTTP {status}"));
   }
   Ok(Fetched::Body(body))
}

pub fn fetch(token: &str, now: i64) -> Result<Snapshot, String> {
   match fetch_raw(token)? {
      Fetched::RateLimited { retry_after_secs } => Err(format!(
         "rate limited, retry in {}s",
         retry_after_secs.unwrap_or(PENALTY_MIN_SECS)
      )),
      Fetched::Body(body) => {
         let v: Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
         Ok(parse_usage(&v, now))
      }
   }
}

/// The endpoint has used both `utilization` and `used_percentage`, and both ISO
/// timestamps and epoch seconds, so accept either of each.
pub fn parse_usage(v: &Value, now: i64) -> Snapshot {
   Snapshot {
      five_hour: window_from(v.get("five_hour"), now),
      seven_day: window_from(v.get("seven_day"), now),
      context: None,
   }
}

fn window_from(v: Option<&Value>, now: i64) -> Option<Window> {
   let v = v?;
   let used = v
      .get("utilization")
      .or_else(|| v.get("used_percentage"))
      .and_then(|x| x.as_f64())?;
   let resets_at = v.get("resets_at").and_then(parse_time)?;
   Some(Window {
      used_pct: used,
      resets_at: Some(resets_at),
      observed_at: now,
      source: Source::Api,
   })
}

fn parse_time(v: &Value) -> Option<i64> {
   if let Some(n) = v.as_i64() {
      return Some(n);
   }
   if let Some(f) = v.as_f64() {
      return Some(f as i64);
   }
   let s = v.as_str()?;
   chrono::DateTime::parse_from_rfc3339(s)
      .ok()
      .map(|dt| dt.timestamp())
}

/// Poll in the background and push every successful reading to the UI thread.
/// Failures are reported once as a message rather than retried tightly.
pub enum ApiEvent {
   /// A request is going out now, so the UI can say so.
   Polling,
   Data(Snapshot),
   Unavailable(String),
   /// The endpoint asked us to back off; nothing will be sent until then.
   RateLimited {
      until: i64,
   },
   /// Our own budget is spent. The request has not been dropped — it goes out
   /// at this moment on its own.
   Throttled {
      until: i64,
   },
}

/// What the poller should do at this instant. Pulled out of the loop so the
/// policy can be tested: the loop then has no decisions of its own left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
   /// Go now.
   Request,
   /// Nothing is due; sleep until this moment.
   Wait(i64),
   /// A request is due but our own budget is spent; it goes out at this
   /// moment, without anyone having to ask again.
   Throttled(i64),
}

/// The whole request policy, in order of who overrules whom.
///
/// A rate-limit penalty beats everything, including "refresh now" — asking
/// again is what makes a rate limit worse. Then comes the schedule: nothing is
/// due until `interval` has passed since the last request, and *this is why
/// changing a setting no longer costs a request*. Only then the budget, which
/// delays rather than refuses.
pub fn decide(
   now: i64,
   last_request: i64,
   interval_secs: u64,
   forced: bool,
   penalty_until: i64,
   history: &[i64],
   budget_per_min: u32,
) -> Action {
   if now < penalty_until {
      return Action::Wait(penalty_until);
   }
   // `last_request == 0` is a poller that has not run yet: start straight away.
   let due_at = if last_request == 0 {
      now
   } else {
      last_request + interval_secs as i64
   };
   if !forced && now < due_at {
      return Action::Wait(due_at);
   }
   let slot = next_slot(history, budget_per_min, now);
   if slot > now {
      Action::Throttled(slot)
   } else {
      Action::Request
   }
}

/// Why the poller was woken.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wake {
   /// Someone asked for figures now.
   Now,
   /// A setting changed; recompute the schedule, but do not go asking.
   Recheck,
}

/// The poller's two settings, shared with the UI rather than copied into the
/// thread.
///
/// They used to be arguments, which meant changing one had to drop the thread
/// and start another — and a fresh thread begins by making a request. Editing
/// the request budget therefore spent from that very budget.
#[derive(Debug)]
pub struct PollerSettings {
   interval_secs: AtomicU64,
   budget_per_min: AtomicU32,
}

impl PollerSettings {
   pub fn new(interval_secs: u64, budget_per_min: u32) -> PollerSettings {
      PollerSettings {
         interval_secs: AtomicU64::new(interval_secs),
         budget_per_min: AtomicU32::new(budget_per_min),
      }
   }

   pub fn set(&self, interval_secs: u64, budget_per_min: u32) {
      self.interval_secs.store(interval_secs, Ordering::Relaxed);
      self.budget_per_min.store(budget_per_min, Ordering::Relaxed);
   }

   pub fn get(&self) -> (u64, u32) {
      (
         self.interval_secs.load(Ordering::Relaxed),
         self.budget_per_min.load(Ordering::Relaxed),
      )
   }
}

/// What the poller remembers of itself across a restart: when it last asked,
/// and what it last learned. The rest of its memory — the penalty, the
/// budget history — stays in-memory only: losing that after a restart just
/// means the widget is briefly as cautious as a fresh install, never more.
///
/// These two are different. Forgetting `last_request` on every restart is
/// what made the schedule pointless during active development, when the
/// widget restarts far more often than the interval — each relaunch asked
/// again immediately, as if it had never asked before. And forgetting the
/// reading itself is what left the pill blank until the next poll came in:
/// the other sources (bridge, desktop, session) don't carry the quota
/// windows at all when they're switched off, so the API's own last answer is
/// sometimes the only copy of that data anywhere outside this file.
fn poll_state_path() -> std::path::PathBuf {
   crate::state::state_dir().join("api_poll.json")
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct PollState {
   last_request: i64,
   #[serde(flatten)]
   limits: crate::state::StoredLimits,
}

fn load_poll_state() -> PollState {
   std::fs::read_to_string(poll_state_path())
      .ok()
      .and_then(|text| serde_json::from_str(&text).ok())
      .unwrap_or_default()
}

/// Written whole and swapped in, like `window.json` (`geometry::save`) — one
/// writer, so no lock, and an error here is not worth interrupting anyone
/// over: the next write will just try again.
fn write_poll_state(state: &PollState) {
   let dir = crate::state::state_dir();
   if std::fs::create_dir_all(&dir).is_err() {
      return;
   }
   let tmp = dir.join(format!("api_poll.{}.tmp", std::process::id()));
   let Ok(text) = serde_json::to_string(state) else {
      return;
   };
   if std::fs::write(&tmp, text).is_ok() {
      let _ = std::fs::rename(&tmp, poll_state_path());
   } else {
      let _ = std::fs::remove_file(&tmp);
   }
}

fn load_last_request() -> i64 {
   load_poll_state().last_request
}

fn save_last_request(last_request: i64) {
   let mut state = load_poll_state();
   state.last_request = last_request;
   write_poll_state(&state);
}

fn to_stored(w: Window) -> crate::state::StoredWindow {
   crate::state::StoredWindow {
      used_percentage: w.used_pct,
      // `parse_usage` already drops a window that arrived without one.
      resets_at: w.resets_at.unwrap_or(0),
      observed_at: w.observed_at,
   }
}

fn from_stored(w: &crate::state::StoredWindow) -> Window {
   Window {
      used_pct: w.used_percentage,
      resets_at: Some(w.resets_at),
      observed_at: w.observed_at,
      source: Source::Api,
   }
}

/// Folds a fresh reading into what was already saved, field by field —
/// absent fields never overwrite present ones, the same rule `Window::merge`
/// (A03) follows: a response with only one window due for renewal must not
/// blank out the other one, which may just not have been due yet.
fn merge_reading(mut state: PollState, snap: &Snapshot) -> PollState {
   if let Some(w) = snap.five_hour {
      state.limits.five_hour = Some(to_stored(w));
   }
   if let Some(w) = snap.seven_day {
      state.limits.seven_day = Some(to_stored(w));
   }
   state
}

fn save_last_reading(snap: &Snapshot) {
   write_poll_state(&merge_reading(load_poll_state(), snap));
}

/// What to show while waiting for the first poll of this run: the answer
/// from the last one, aged honestly. Its `observed_at` is untouched, so the
/// normal staleness rules (A07, A14) grey it out and then drop it exactly as
/// they would a live reading nobody has refreshed in a while — nothing here
/// pretends the data is newer than it is.
fn reading_from_state(state: &PollState) -> (Snapshot, Option<i64>) {
   let five_hour = state.limits.five_hour.as_ref().map(from_stored);
   let seven_day = state.limits.seven_day.as_ref().map(from_stored);
   let last_ok = five_hour.or(seven_day).map(|w| w.observed_at);
   (
      Snapshot {
         five_hour,
         seven_day,
         context: None,
      },
      last_ok,
   )
}

pub fn load_last_reading() -> (Snapshot, Option<i64>) {
   reading_from_state(&load_poll_state())
}

/// Poll in the background, and again immediately whenever someone sends
/// `Wake::Now`. Waiting on the channel rather than sleeping is what makes
/// "refresh now" instant instead of up to one interval away.
///
/// The whole request policy lives here, next to the requests themselves: the
/// budget that keeps the endpoint from ever having to refuse us, and the pause
/// that follows if it does anyway. Both are read from `settings` on every lap,
/// so the thread outlives any change to them.
pub fn spawn_poller(settings: Arc<PollerSettings>, tx: Sender<ApiEvent>) -> Sender<Wake> {
   let (wake_tx, wake_rx): (Sender<Wake>, Receiver<Wake>) = std::sync::mpsc::channel();
   std::thread::spawn(move || {
      let mut penalty_until: i64 = 0;
      let mut penalty_secs: i64 = 0;
      let mut last_request: i64 = load_last_request();
      // Times of the requests made in the last minute, oldest first.
      let mut history: Vec<i64> = Vec::new();
      // A "refresh now" that has not been satisfied yet; it survives a wait
      // for the budget or for a penalty to expire.
      let mut forced = false;

      loop {
         let now = crate::state::now();
         let (interval, budget) = settings.get();
         history.retain(|t| now - *t < 60);

         let wait = match decide(
            now,
            last_request,
            interval,
            forced,
            penalty_until,
            &history,
            budget,
         ) {
            Action::Wait(until) => Duration::from_secs((until - now).max(1) as u64),
            Action::Throttled(until) => {
               // Not a refusal: the request is due in a few seconds, and
               // this loop will make it then without being asked again.
               let _ = tx.send(ApiEvent::Throttled { until });
               Duration::from_secs((until - now).max(1) as u64)
            }
            Action::Request => {
               forced = false;
               last_request = now;
               save_last_request(now);
               history.push(now);
               let _ = tx.send(ApiEvent::Polling);
               let mut wait = Duration::from_secs(interval.max(1));
               match read_token() {
                  None => {
                     let _ = tx.send(ApiEvent::Unavailable("no credentials file".into()));
                  }
                  Some(t) if t.is_expired(now) => {
                     let _ = tx.send(ApiEvent::Unavailable("token expired".into()));
                  }
                  Some(t) => match fetch_raw(&t.access_token) {
                     Ok(Fetched::RateLimited { retry_after_secs }) => {
                        penalty_secs = next_penalty(penalty_secs, retry_after_secs);
                        penalty_until = now + penalty_secs;
                        wait = Duration::from_secs(penalty_secs.max(1) as u64);
                        let _ = tx.send(ApiEvent::RateLimited {
                           until: penalty_until,
                        });
                     }
                     Ok(Fetched::Body(body)) => match serde_json::from_str::<Value>(&body) {
                        Ok(v) => {
                           penalty_secs = 0;
                           // Stamped when the answer arrived, not when
                           // the request left: before the request there
                           // were no figures to be the age of.
                           // Everything that shows an age reads this
                           // one number.
                           let seen = crate::state::now();
                           let snap = parse_usage(&v, seen);
                           save_last_reading(&snap);
                           let _ = tx.send(ApiEvent::Data(snap));
                        }
                        Err(e) => {
                           let _ = tx.send(ApiEvent::Unavailable(e.to_string()));
                        }
                     },
                     Err(e) => {
                        let _ = tx.send(ApiEvent::Unavailable(e));
                     }
                  },
               }
               wait
            }
         };

         // Collapse a burst of wake-ups into the single lap that follows; a
         // `Now` among them survives, a `Recheck` only ends the wait early.
         match wake_rx.recv_timeout(wait) {
            Ok(w) => {
               forced |= w == Wake::Now;
               while let Ok(w) = wake_rx.try_recv() {
                  forced |= w == Wake::Now;
               }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
         }
      }
   });
   wake_tx
}

#[cfg(test)]
mod tests {
   use super::*;

   #[test]
   fn parses_iso_timestamps_and_utilization() {
      let v: Value = serde_json::from_str(
         r#"{
              "five_hour": { "utilization": 37.0, "resets_at": "2026-02-08T04:59:59.000000+00:00" },
              "seven_day": { "utilization": 26.0, "resets_at": "2026-02-12T14:59:59.771647+00:00" }
            }"#,
      )
      .unwrap();
      let snap = parse_usage(&v, 1_000);
      let five = snap.five_hour.unwrap();
      assert_eq!(five.used_pct, 37.0);
      assert_eq!(five.resets_at, Some(1_770_526_799));
      assert_eq!(five.source, Source::Api);
      assert!(snap.seven_day.is_some());
   }

   #[test]
   fn parses_epoch_timestamps_and_used_percentage() {
      let v: Value = serde_json::from_str(
         r#"{ "five_hour": { "used_percentage": 12.5, "resets_at": 1738425600 } }"#,
      )
      .unwrap();
      let snap = parse_usage(&v, 1_000);
      assert_eq!(snap.five_hour.unwrap().resets_at, Some(1_738_425_600));
      assert!(snap.seven_day.is_none());
   }

   #[test]
   fn a_window_without_a_reset_time_is_dropped() {
      let v: Value = serde_json::from_str(r#"{ "five_hour": { "utilization": 5 } }"#).unwrap();
      assert!(parse_usage(&v, 1_000).five_hour.is_none());
   }

   #[test]
   fn the_penalty_starts_at_a_minute_and_doubles() {
      assert_eq!(next_penalty(0, None), 60);
      assert_eq!(next_penalty(60, None), 120);
      assert_eq!(next_penalty(120, None), 240);
      // ...and stops growing.
      assert_eq!(next_penalty(240, None), 300);
      assert_eq!(next_penalty(300, None), 300);
      // A success resets the ladder, so the next refusal is a minute again.
      assert_eq!(next_penalty(0, None), 60);
   }

   #[test]
   fn retry_after_wins_over_our_guess() {
      assert_eq!(next_penalty(240, Some(15)), 15);
      assert_eq!(next_penalty(0, Some(0)), 60, "a zero header is no header");
   }

   #[test]
   fn the_budget_delays_rather_than_refuses() {
      let now = 1_000_000;
      // Three requests with a budget of four: go now.
      let history = vec![now - 50, now - 40, now - 30];
      assert_eq!(next_slot(&history, 4, now), now);
      // A fourth fills it; the next one waits for the oldest to age out.
      let history = vec![now - 50, now - 40, now - 30, now - 20];
      assert_eq!(next_slot(&history, 4, now), now + 10);
   }

   #[test]
   fn a_budget_of_one_still_works() {
      let now = 1_000_000;
      assert_eq!(next_slot(&[], 1, now), now);
      assert_eq!(next_slot(&[now - 5], 1, now), now + 55);
      assert_eq!(next_slot(&[now - 5], 0, now), now + 55, "zero means one");
   }

   const NOW: i64 = 1_000_000;

   #[test]
   fn a_poller_that_has_never_run_goes_at_once() {
      assert_eq!(decide(NOW, 0, 180, false, 0, &[], 4), Action::Request);
   }

   #[test]
   fn nothing_is_due_until_the_interval_has_passed() {
      // The regression this was written for: editing a setting used to drop
      // the thread and start another, and a fresh thread began with a
      // request. Ten seconds into a three-minute interval, nothing is due.
      assert_eq!(
         decide(NOW, NOW - 10, 180, false, 0, &[NOW - 10], 4),
         Action::Wait(NOW + 170)
      );
   }

   #[test]
   fn refresh_now_overrules_the_schedule() {
      assert_eq!(
         decide(NOW, NOW - 10, 180, true, 0, &[NOW - 10], 4),
         Action::Request
      );
   }

   #[test]
   fn a_spent_budget_delays_a_forced_request_rather_than_dropping_it() {
      let history = [NOW - 50, NOW - 40, NOW - 30, NOW - 20];
      assert_eq!(
         decide(NOW, NOW - 20, 180, true, 0, &history, 4),
         Action::Throttled(NOW + 10)
      );
   }

   #[test]
   fn a_penalty_outranks_even_refresh_now() {
      assert_eq!(
         decide(NOW, NOW - 500, 180, true, NOW + 90, &[], 4),
         Action::Wait(NOW + 90)
      );
   }

   #[test]
   fn a_shorter_interval_can_make_a_poll_due_immediately() {
      // Not a bug: two minutes have passed and the interval was just set to
      // one, so the reading really is overdue.
      assert_eq!(
         decide(NOW, NOW - 120, 60, false, 0, &[NOW - 120], 4),
         Action::Request
      );
   }

   #[test]
   fn expiry_is_read_in_milliseconds() {
      let t = Token {
         access_token: "x".into(),
         expires_at_ms: Some(2_000_000),
      };
      assert!(t.is_expired(2_001));
      assert!(!t.is_expired(1_999));
   }

   // Only the data shape is tested here, not `load_last_request` /
   // `save_last_request` themselves — those touch the real state directory,
   // and geometry.rs's equivalent functions aren't unit-tested for the same
   // reason.
   #[test]
   fn an_absent_or_empty_file_is_a_poller_that_has_never_run() {
      // What `load_last_request` falls back to when the file is missing or
      // unreadable — the same `0` `decide` already treats as "go at once".
      let state: PollState = serde_json::from_str("{}").unwrap();
      assert_eq!(state.last_request, 0);
   }

   #[test]
   fn a_saved_request_time_round_trips() {
      let state = PollState {
         last_request: 1_700_000_000,
         ..PollState::default()
      };
      let text = serde_json::to_string(&state).unwrap();
      let back: PollState = serde_json::from_str(&text).unwrap();
      assert_eq!(back.last_request, 1_700_000_000);
   }

   fn window(used_pct: f64, observed_at: i64) -> Window {
      Window {
         used_pct,
         resets_at: Some(observed_at + 3600),
         observed_at,
         source: Source::Api,
      }
   }

   #[test]
   fn a_reading_with_both_windows_is_saved_whole() {
      let snap = Snapshot {
         five_hour: Some(window(10.0, 1_000)),
         seven_day: Some(window(20.0, 1_000)),
         context: None,
      };
      let state = merge_reading(PollState::default(), &snap);
      assert_eq!(state.limits.five_hour.unwrap().used_percentage, 10.0);
      assert_eq!(state.limits.seven_day.unwrap().used_percentage, 20.0);
   }

   #[test]
   fn a_window_missing_from_the_answer_does_not_blank_out_the_saved_one() {
      // The same rule `Window::merge` follows (A03): a response that is only
      // due to renew one window must not erase the other.
      let mut saved = PollState::default();
      saved.limits.seven_day = Some(crate::state::StoredWindow {
         used_percentage: 20.0,
         resets_at: 4_600,
         observed_at: 1_000,
      });
      let snap = Snapshot {
         five_hour: Some(window(15.0, 2_000)),
         seven_day: None,
         context: None,
      };
      let state = merge_reading(saved, &snap);
      assert_eq!(state.limits.five_hour.unwrap().used_percentage, 15.0);
      assert_eq!(
         state.limits.seven_day.unwrap().used_percentage,
         20.0,
         "unchanged"
      );
   }

   #[test]
   fn an_empty_saved_state_reads_back_as_no_reading_at_all() {
      let (snap, last_ok) = reading_from_state(&PollState::default());
      assert!(snap.five_hour.is_none());
      assert!(snap.seven_day.is_none());
      assert_eq!(
         last_ok, None,
         "nothing was ever polled, so nothing was ever ok"
      );
   }

   #[test]
   fn a_saved_reading_comes_back_tagged_as_the_api_source() {
      let state = merge_reading(
         PollState::default(),
         &Snapshot {
            five_hour: Some(window(30.0, 5_000)),
            seven_day: None,
            context: None,
         },
      );
      let (snap, last_ok) = reading_from_state(&state);
      let five = snap.five_hour.unwrap();
      assert_eq!(five.source, Source::Api);
      assert_eq!(five.used_pct, 30.0);
      assert_eq!(last_ok, Some(5_000));
   }
}
