//! `--serve`: the same snapshot the pill draws, as JSON over loopback.
//!
//! The pill is moving to Slint (A20), whose language has to be learnt before a
//! theme can be written. This is the other door: a panel that already runs a
//! webview — Zebar, Rainmeter, eww —
//! can `fetch` this and draw the figures in real CSS, at no cost in memory,
//! because that webview is running either way.
//!
//! It is a serialisation layer and nothing more. Every number and every phrase
//! here comes out of `view.rs` (A19); anything this module had to work out for
//! itself would be a value still trapped in the drawing layer, and the fix for
//! that is in `view.rs`, not here.
//!
//! **The JSON is a published contract** (A21), which is why it is built from
//! the types below rather than by deriving `Serialize` on the model: renaming
//! a field in `model.rs` must not break somebody's Rainmeter skin.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;

use crate::config::{BarColour, Config, CtxView, DetailMode, PaceView, PctMode};
use crate::model::{ContextInfo, Snapshot, Window};
use crate::pace;
use crate::runtime::Runtime;
use crate::state;
use crate::view::{self, Tone};

/// Bumped when a field changes meaning or leaves. Adding one does not count:
/// a consumer that reads what it knows keeps working.
const SCHEMA: u32 = 1;

/// Arbitrary, high, and out of the way of anything common. Overridable with
/// `--port`.
pub const DEFAULT_PORT: u16 = 47317;

/// How often the sources are pumped. The widget redraws every second; a panel
/// reading this does not need to be quicker, and `Runtime::pump` gates its own
/// file reads at 250 ms regardless.
const TICK: Duration = Duration::from_secs(1);

// ---------------------------------------------------------------------------
// The contract
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct Payload {
   schema: u32,
   generated_at: i64,
   stale: Stale,
   five_hour: Option<Quota>,
   seven_day: Option<Quota>,
   context: Option<Context>,
   /// The pill itself, figure by figure, in the order it reads: the text,
   /// which group it belongs to, and whether its colour means anything.
   ///
   /// `view::Cell` and `view::Bar` travel as they are rather than through a
   /// DTO of their own, which is the exception `Ink` and `Tone` already are:
   /// `view.rs` *is* the layer a second front end consumes (A19), so its
   /// vocabulary is the contract rather than an internal name that happens to
   /// be visible. `model.rs` is the one that stays behind a DTO.
   cells: Vec<view::Cell>,
   /// One hairline per group that has something to show.
   bars: Vec<view::Bar>,
   /// The detail panel, line for line, already worded and localised — the
   /// cheapest way to put something true on screen without reimplementing any
   /// of the reasoning above.
   detail_lines: Vec<Line>,
   /// Polling, a rate limit, a queued request or an error: what the age of a
   /// reading cannot say. `null` when there is nothing to report.
   notice: Option<String>,
   display: Display,
}

#[derive(Serialize)]
struct Stale {
   windows: bool,
   context: bool,
}

#[derive(Serialize)]
struct Line {
   label: String,
   value: String,
}

#[derive(Serialize)]
struct Quota {
   /// The stored reading, always "spent" (A15). Colour and pace are keyed on
   /// this and never on `shown_pct`.
   used_pct: f64,
   /// The same reading under the widget's display convention, for a widget
   /// that wants to agree with the pill.
   shown_pct: f64,
   left_pct: f64,
   resets_at: Option<i64>,
   resets_in_secs: Option<i64>,
   /// `3:37` — the countdown as the pill writes it, rounded the way Claude
   /// Code rounds it (G10).
   resets_in: Option<String>,
   /// `15:37` — local wall-clock time of the reset.
   resets_clock: Option<String>,
   observed_at: i64,
   age_secs: i64,
   source: &'static str,
   tone: Tone,
   pace: Option<Pace>,
   /// `20%/h · runs out 07:19, 0:42 before the reset`, or empty while the log
   /// has too little to say.
   rate_text: String,
   /// Weekly only.
   days_left: Option<i64>,
   /// Weekly only: how much may be spent per day to finish exactly at 100%.
   daily_allowance_pct: Option<f64>,
}

#[derive(Serialize)]
struct Pace {
   /// Where usage would be on an even burn.
   target_pct: f64,
   /// `used_pct - target_pct`. Positive is an overspend; everything else
   /// about pace is a rendering of this one number (A16).
   delta_pct: f64,
   /// The same gap in the window's time, signed like `delta_pct`: 9 points of
   /// a five-hour window is 1620. Capped at the whole window.
   delta_secs: i64,
   /// How many times slower the rest of the window has to go. Clamped at 9.9.
   ratio: f64,
   verdict: &'static str,
   /// Every unit — points, ratio, time — led by the one the widget was asked
   /// for, which carries the arrow.
   text: String,
   tone: Tone,
}

#[derive(Serialize)]
struct Context {
   used_tokens: Option<u64>,
   limit_tokens: Option<u64>,
   /// False when the window size was inferred from a table of model names
   /// rather than stated by the status line (A18). A denominator wrong by five
   /// times is invisible in a percentage, so say which it is.
   limit_stated: bool,
   used_pct: f64,
   shown_pct: f64,
   model: Option<String>,
   label: Option<String>,
   session_id: String,
   observed_at: i64,
   age_secs: i64,
   source: &'static str,
   tone: Tone,
}

/// What the widget was asked to say, so a panel can follow the same
/// conventions instead of inventing its own.
#[derive(Serialize)]
struct Display {
   lang: String,
   pct_mode: PctMode,
   pace_view: PaceView,
   ctx_view: CtxView,
   ctx_pct_dir: PctMode,
   ctx_tokens_dir: PctMode,
   bar_colour: BarColour,
   detail_mode: DetailMode,
   warn_pct: f64,
   danger_pct: f64,
   pace_tolerance: f64,
   demo: bool,
}

// ---------------------------------------------------------------------------
// Building it
// ---------------------------------------------------------------------------

fn quota(cfg: &Config, rt: &Runtime, w: Window, span: i64, now: i64, weekly: bool) -> Quota {
   let pace_of = w.resets_at.map(|r| {
      let target = pace::pace_target(r, span, now);
      let delta = w.used_pct - target;
      Pace {
         target_pct: target,
         delta_pct: delta,
         ratio: pace::pace_ratio(w.used_pct, target),
         verdict: pace::pace_of(w.used_pct, target, cfg.pace_tolerance).key(),
         delta_secs: pace::pace_secs(delta, span) * if delta < 0.0 { -1 } else { 1 },
         text: pace::pace_detail(cfg.pace_view, w.used_pct, target, span, &cfg.lang),
         tone: view::pace_tone(delta, cfg),
      }
   });
   Quota {
      used_pct: w.used_pct,
      shown_pct: cfg.pct_mode.shown(w.used_pct),
      left_pct: PctMode::Left.shown(w.used_pct),
      resets_at: w.resets_at,
      resets_in_secs: w.resets_at.map(|r| (r - now).max(0)),
      resets_in: w.resets_at.map(|r| pace::fmt_remaining(r - now)),
      resets_clock: w.resets_at.map(pace::fmt_clock),
      observed_at: w.observed_at,
      age_secs: now - w.observed_at,
      source: w.source.key(),
      tone: view::usage_tone(w.used_pct, cfg),
      pace: pace_of,
      rate_text: match w.resets_at {
         Some(r) => view::rate_text(cfg, rt, w, span, r, now),
         None => String::new(),
      },
      days_left: match (weekly, w.resets_at) {
         (true, Some(r)) => Some(pace::days_until(r, now, cfg.day_count())),
         _ => None,
      },
      daily_allowance_pct: match (weekly, w.resets_at) {
         (true, Some(r)) => Some(pace::daily_allowance(
            w.used_pct,
            pace::days_until(r, now, cfg.day_count()),
         )),
         _ => None,
      },
   }
}

fn context(cfg: &Config, c: &ContextInfo, now: i64) -> Context {
   Context {
      used_tokens: c.used_tokens,
      limit_tokens: c.limit_tokens,
      limit_stated: c.limit_stated,
      used_pct: c.used_pct,
      // `ctx` reads by its own direction, not `pct_mode`: a window fills
      // where a quota drains (A18).
      shown_pct: cfg.ctx_pct_dir.shown(c.used_pct),
      model: c.model.clone(),
      label: c.label.clone(),
      session_id: c.session_id.clone(),
      observed_at: c.observed_at,
      age_secs: now - c.observed_at,
      source: c.source.key(),
      tone: view::usage_tone(c.used_pct, cfg),
   }
}

/// The whole snapshot as the JSON text served to callers.
fn render(cfg: &Config, rt: &Runtime, snap: &Snapshot, stale: (bool, bool), now: i64) -> String {
   let payload = Payload {
      schema: SCHEMA,
      generated_at: now,
      stale: Stale {
         windows: stale.0,
         context: stale.1,
      },
      five_hour: snap
         .five_hour
         .map(|w| quota(cfg, rt, w, pace::FIVE_HOUR_SECS, now, false)),
      seven_day: snap
         .seven_day
         .map(|w| quota(cfg, rt, w, pace::SEVEN_DAY_SECS, now, true)),
      context: snap.context.as_ref().map(|c| context(cfg, c, now)),
      cells: view::cells(cfg, snap, stale.0, stale.1, now),
      bars: view::bars(cfg, snap, stale.1, now),
      detail_lines: view::detail_lines(cfg, rt, snap, stale.0, now)
         .into_iter()
         .map(|(label, value)| Line { label, value })
         .collect(),
      notice: view::api_notice(cfg, rt, now),
      display: Display {
         lang: cfg.lang.clone(),
         pct_mode: cfg.pct_mode,
         pace_view: cfg.pace_view,
         ctx_view: cfg.ctx_view,
         ctx_pct_dir: cfg.ctx_pct_dir,
         ctx_tokens_dir: cfg.ctx_tokens_dir(),
         bar_colour: cfg.bar_colour,
         detail_mode: cfg.detail_mode,
         warn_pct: cfg.warn_pct,
         danger_pct: cfg.danger_pct,
         pace_tolerance: cfg.pace_tolerance,
         demo: cfg.demo,
      },
   };
   serde_json::to_string_pretty(&payload).unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
}

// ---------------------------------------------------------------------------
// Serving it
// ---------------------------------------------------------------------------

/// Run the server until the process is killed. Returns an exit code.
pub fn run(port: u16, no_api: bool) -> i32 {
   let mut cfg = Config::load();
   if no_api {
      // Two pollers against one undocumented endpoint is how a request
      // budget stops meaning anything (A13): the budget is per process, so a
      // widget and a server together ask twice as often as either was
      // allowed to. This is the switch for running both.
      cfg.api_enabled = false;
   }

   // Bind before announcing anything, so "port already in use" is an exit
   // code and a line on stderr rather than a server that silently is not one.
   let addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
   let listener = match TcpListener::bind(addr) {
      Ok(l) => l,
      Err(e) => {
         eprintln!("ccd-usage-monitor --serve: cannot listen on {addr}: {e}");
         return 1;
      }
   };

   let latest = Arc::new(Mutex::new(String::from("{}")));
   let served = latest.clone();
   std::thread::spawn(move || accept_loop(listener, served));

   println!("ccd-usage-monitor --serve: http://{addr}/");
   if !cfg.api_enabled {
      println!("  the usage endpoint is off; the file sources carry the figures");
   }

   // The pump owns the runtime outright and hands the listener a finished
   // string, so nothing is locked for longer than a clone.
   let mut rt = Runtime::new();
   // Edits to the config reach the server the same way they reach the widget
   // (A23). Without this, the one process a person is most likely to leave
   // running for days would be the one that never noticed.
   let mut watch = crate::config::Watch::new();
   loop {
      let now = state::now();
      if let Some(fresh) = watch.poll() {
         cfg = fresh;
         if no_api {
            cfg.api_enabled = false;
         }
         rt.invalidate_files();
      }
      rt.pump(&cfg, now);
      let snap = rt.snapshot(&cfg, now);
      let (snap, stale, ctx_stale) = rt.age_filter(&cfg, snap, now);
      let text = render(&cfg, &rt, &snap, (stale, ctx_stale), now);
      if let Ok(mut slot) = latest.lock() {
         *slot = text;
      }
      std::thread::sleep(TICK);
   }
}

fn accept_loop(listener: TcpListener, latest: Arc<Mutex<String>>) {
   for stream in listener.incoming() {
      let Ok(mut stream) = stream else { continue };
      // One connection must not be able to hold the loop: there is no
      // keep-alive and no second thread, so a client that opens a socket and
      // says nothing would otherwise be a stop button.
      let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
      let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
      let _ = answer(&mut stream, &latest);
   }
}

fn answer(stream: &mut TcpStream, latest: &Arc<Mutex<String>>) -> std::io::Result<()> {
   let head = read_head(stream)?;
   let mut words = head.split_whitespace();
   let method = words.next().unwrap_or_default();
   let path = words.next().unwrap_or_default();
   // Anything after `?` is not ours to interpret; there is one resource here.
   let path = path.split('?').next().unwrap_or(path);

   match (method, path) {
      // A plain `fetch` of JSON is a simple request and needs no preflight,
      // but a widget that sets a header of its own will send one.
      ("OPTIONS", _) => reply(stream, "204 No Content", None, b""),
      ("GET" | "HEAD", "/" | "/snapshot.json") => {
         let body = latest.lock().map(|s| s.clone()).unwrap_or_default();
         let body = if method == "HEAD" {
            String::new()
         } else {
            body
         };
         reply(
            stream,
            "200 OK",
            Some("application/json; charset=utf-8"),
            body.as_bytes(),
         )
      }
      ("GET" | "HEAD", _) => reply(stream, "404 Not Found", Some("text/plain"), b"no\n"),
      _ => reply(
         stream,
         "405 Method Not Allowed",
         Some("text/plain"),
         b"no\n",
      ),
   }
}

/// Read up to the end of the headers, and no further. We answer one shape of
/// request, so there is no reason to let a caller decide how much to allocate.
fn read_head(stream: &mut TcpStream) -> std::io::Result<String> {
   let mut buf = [0u8; 2048];
   let mut got = 0;
   while got < buf.len() {
      let n = stream.read(&mut buf[got..])?;
      if n == 0 {
         break;
      }
      got += n;
      if buf[..got].windows(4).any(|w| w == b"\r\n\r\n") {
         break;
      }
   }
   Ok(String::from_utf8_lossy(&buf[..got]).into_owned())
}

fn reply(
   stream: &mut TcpStream,
   status: &str,
   content_type: Option<&str>,
   body: &[u8],
) -> std::io::Result<()> {
   let mut head = format!("HTTP/1.1 {status}\r\n");
   if let Some(ct) = content_type {
      head.push_str(&format!("Content-Type: {ct}\r\n"));
   }
   head.push_str(&format!("Content-Length: {}\r\n", body.len()));
   // Loopback only, and the body is percentages and a session id rather than
   // anything secret — but it is readable by any page the browser happens to
   // have open, and that is the trade for a widget in somebody else's webview
   // being able to read it at all.
   head.push_str("Access-Control-Allow-Origin: *\r\n");
   head.push_str("Access-Control-Allow-Methods: GET, HEAD, OPTIONS\r\n");
   head.push_str("Cache-Control: no-store\r\n");
   // No keep-alive: a single-threaded loop that lets a browser hold the
   // connection open is a server that answers one client, once.
   head.push_str("Connection: close\r\n\r\n");

   stream.write_all(head.as_bytes())?;
   stream.write_all(body)?;
   stream.flush()
}

#[cfg(test)]
mod tests {
   use super::*;
   use crate::model::Source;
   use chrono::{Local, TimeZone};

   fn now() -> i64 {
      Local
         .with_ymd_and_hms(2026, 9, 11, 12, 0, 0)
         .single()
         .unwrap()
         .timestamp()
   }

   fn fixture(now: i64) -> Snapshot {
      Snapshot {
         five_hour: Some(Window {
            used_pct: 24.0,
            resets_at: Some(now + 3 * 3600 + 37 * 60),
            observed_at: now - 12,
            source: Source::Api,
         }),
         seven_day: Some(Window {
            used_pct: 54.0,
            resets_at: Some(now + 3 * 86400 + 5 * 3600),
            observed_at: now - 12,
            source: Source::Api,
         }),
         context: Some(ContextInfo {
            used_tokens: Some(382_000),
            limit_tokens: Some(1_000_000),
            used_pct: 38.0,
            limit_stated: false,
            model: Some("claude-opus-5".into()),
            label: None,
            session_id: "abcdef0123456789".into(),
            observed_at: now - 90,
            source: Source::Session,
         }),
      }
   }

   fn parsed(cfg: &Config) -> serde_json::Value {
      let now = now();
      let rt = Runtime::blank();
      let text = render(cfg, &rt, &fixture(now), (false, false), now);
      serde_json::from_str(&text).expect("what we serve is valid JSON")
   }

   #[test]
   fn the_snapshot_carries_both_halves_of_every_figure() {
      let v = parsed(&Config::default());
      assert_eq!(v["schema"], SCHEMA);

      let five = &v["five_hour"];
      assert_eq!(five["used_pct"], 24.0);
      assert_eq!(five["left_pct"], 76.0);
      assert_eq!(five["resets_in"], "3:37");
      assert_eq!(five["age_secs"], 12);
      assert_eq!(five["source"], "api");
      assert_eq!(five["pace"]["verdict"], "on_track");

      // The division, not only its answer (A18) — and the flag that says the
      // divisor was a guess.
      let ctx = &v["context"];
      assert_eq!(ctx["used_tokens"], 382_000);
      assert_eq!(ctx["limit_tokens"], 1_000_000);
      assert_eq!(ctx["limit_stated"], false);
      assert_eq!(ctx["model"], "claude-opus-5");
   }

   #[test]
   fn the_weekly_window_alone_carries_a_day_count() {
      let v = parsed(&Config::default());
      assert_eq!(v["seven_day"]["days_left"], 3);
      assert_eq!(
         v["seven_day"]["daily_allowance_pct"]
            .as_f64()
            .unwrap()
            .round(),
         15.0
      );
      assert!(
         v["five_hour"]["days_left"].is_null(),
         "a five-hour window has no days"
      );
   }

   #[test]
   fn colour_travels_as_a_role_and_never_as_a_value() {
      // A19: the palette belongs to whoever draws. What crosses the wire is
      // which two roles a figure sits between, and how far along.
      // Both fixture figures sit under the warning line, so both are flat.
      let v = parsed(&Config::default());
      assert_eq!(v["five_hour"]["tone"]["from"], "green");
      assert_eq!(v["five_hour"]["tone"]["to"], "green");
      assert!(v["five_hour"]["tone"]["t"].is_number());

      // Past it the figure names the two roles it sits between, and how far
      // along — never a colour.
      let now = now();
      let cfg = Config::default();
      let rt = Runtime::blank();
      let mut snap = fixture(now);
      snap.five_hour.as_mut().unwrap().used_pct = 90.0;
      let v: serde_json::Value =
         serde_json::from_str(&render(&cfg, &rt, &snap, (false, false), now)).unwrap();

      assert_eq!(v["five_hour"]["tone"]["from"], "amber");
      assert_eq!(v["five_hour"]["tone"]["to"], "red");
      // Over pace as well, and pace ramps from orange rather than from green
      // the moment it is over the line (A16).
      assert_eq!(v["five_hour"]["pace"]["verdict"], "behind");
      assert_eq!(v["five_hour"]["pace"]["tone"]["from"], "orange");
   }

   #[test]
   fn the_stored_reading_is_spending_whatever_the_convention_is() {
      // A15: one number stored, two ways to read it. `used_pct` must not
      // move when the display convention does, or a consumer computing a
      // colour from it would invert with the pill.
      let spent = parsed(&Config::default());
      let left = parsed(&Config {
         pct_mode: PctMode::Left,
         ..Config::default()
      });

      assert_eq!(
         spent["five_hour"]["used_pct"],
         left["five_hour"]["used_pct"]
      );
      assert_eq!(spent["five_hour"]["shown_pct"], 24.0);
      assert_eq!(left["five_hour"]["shown_pct"], 76.0);
      assert_eq!(left["display"]["pct_mode"], "left");
   }

   #[test]
   fn the_panel_travels_already_worded() {
      let v = parsed(&Config {
         lang: "en".to_string(),
         ..Config::default()
      });
      let lines = v["detail_lines"].as_array().expect("an array of lines");
      assert_eq!(lines[0]["label"], "5h");
      assert!(lines[0]["value"].as_str().unwrap().starts_with("24% spent"));
      assert_eq!(lines.last().unwrap()["label"], "src");
   }

   #[test]
   fn an_empty_snapshot_serves_nulls_rather_than_zeroes() {
      // A window nobody reported is absent, not nought per cent: the whole
      // point of `--%` on the pill is that a wrong number looks right.
      let now = now();
      let cfg = Config::default();
      let rt = Runtime::blank();
      let text = render(&cfg, &rt, &Snapshot::default(), (false, false), now);
      let v: serde_json::Value = serde_json::from_str(&text).unwrap();

      assert!(v["five_hour"].is_null());
      assert!(v["seven_day"].is_null());
      assert!(v["context"].is_null());
      // The panel still says something, because "no data" is an answer.
      assert!(!v["detail_lines"].as_array().unwrap().is_empty());
   }

   #[test]
   fn a_window_without_a_reset_has_no_pace_and_says_so() {
      let now = now();
      let cfg = Config::default();
      let rt = Runtime::blank();
      let snap = Snapshot {
         five_hour: Some(Window {
            used_pct: 24.0,
            resets_at: None,
            observed_at: now,
            source: Source::Desktop,
         }),
         seven_day: None,
         context: None,
      };
      let v: serde_json::Value =
         serde_json::from_str(&render(&cfg, &rt, &snap, (false, false), now)).unwrap();

      assert!(v["five_hour"]["pace"].is_null(), "no clock to be ahead of");
      assert!(v["five_hour"]["resets_at"].is_null());
      assert_eq!(v["five_hour"]["rate_text"], "");
      assert_eq!(v["five_hour"]["source"], "desktop");
   }
}
