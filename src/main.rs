//! ccd-usage-monitor — a floating one-line readout of Claude Code quota usage.
//!
//! Modes:
//!   ccd-usage-monitor             the widget
//!   ccd-usage-monitor --demo      the widget on synthetic data
//!   ccd-usage-monitor --bridge    Claude Code status-line command: stdin JSON -> state file
//!   ccd-usage-monitor --print     print the current line and exit
//!   ccd-usage-monitor --probe-api one usage-endpoint request, printed raw
//!   ccd-usage-monitor --sources  what each source reports right now
//!   ccd-usage-monitor --sessions  every conversation on disk and what it reports
//!   ccd-usage-monitor --serve     the same snapshot as JSON on loopback (A21),
//!                                 for a widget in a panel that already runs a
//!                                 webview. `--port N`, `--no-api`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod bridge;
mod config;
mod desktop;
mod geometry;
mod history;
mod model;
mod pace;
mod runtime;
mod schema;
mod serve;
mod session;
#[cfg(windows)]
mod settings_ui;
#[cfg(windows)]
mod slint_ui;
mod state;
mod tray;
mod view;
#[cfg(windows)]
mod win;

fn main() {
   let args: Vec<String> = std::env::args().skip(1).collect();
   let has = |flag: &str| args.iter().any(|a| a == flag);
   // `--flag value` and `--flag=value` both, because a panel's config file
   // spells these either way and being wrong about it is a silent default.
   let value = |flag: &str| -> Option<String> {
      args
         .iter()
         .position(|a| a == flag)
         .and_then(|i| args.get(i + 1).cloned())
         .or_else(|| {
            args
               .iter()
               .find_map(|a| a.strip_prefix(&format!("{flag}=")).map(str::to_string))
         })
   };

   if has("--serve") {
      let port = match value("--port") {
         Some(p) => match p.parse() {
            Ok(p) => p,
            Err(_) => {
               eprintln!("ccd-usage-monitor: --port wants a number, got `{p}`");
               std::process::exit(2);
            }
         },
         None => serve::DEFAULT_PORT,
      };
      std::process::exit(serve::run(port, has("--no-api")));
   }
   if has("--bridge") {
      std::process::exit(bridge::run());
   }
   if has("--probe-api") {
      std::process::exit(probe_api());
   }
   if has("--sources") {
      std::process::exit(probe_sources());
   }
   if has("--sessions") {
      std::process::exit(probe_sessions());
   }
   if has("--print") {
      let cfg = config::Config::load();
      let now = state::now();
      // Both local sources, because both are a file read and neither costs a
      // request: the state file for the quota windows, a transcript for the
      // context. The endpoint is deliberately left out — a command meant for
      // a shell prompt must not go to the network.
      let file = state::read();
      let session = session::choose(&session::list(&session::live_sessions()), &cfg)
         .and_then(|e| session::snapshot(&cfg, &file.context_limits, &e))
         .unwrap_or_default();
      let snap = model::Snapshot::merge(file.to_snapshot(), session);
      println!("{}", bridge::render(&snap, &cfg, now));
      return;
   }

   if let Err(e) = slint_ui::run(has("--demo")) {
      eprintln!("ccd-usage-monitor: {e}");
      std::process::exit(1);
   }
}

fn probe_api() -> i32 {
   let now = state::now();
   let Some(token) = api::read_token() else {
      eprintln!("no credentials at ~/.claude/.credentials.json");
      return 1;
   };
   match token.expires_at_ms {
      Some(ms) => println!(
         "token expires at {} (epoch {}), now {}",
         pace::fmt_reset_full(ms / 1000, "en"),
         ms / 1000,
         now
      ),
      None => println!("token carries no expiry"),
   }
   if token.is_expired(now) {
      eprintln!("token expired; open Claude Code once to refresh it");
      return 1;
   }
   match api::fetch_raw(&token.access_token) {
      Ok(api::Fetched::RateLimited { retry_after_secs }) => {
         eprintln!(
            "rate limited (HTTP 429); retry in {} s",
            retry_after_secs.unwrap_or(api::PENALTY_MIN_SECS)
         );
         1
      }
      Ok(api::Fetched::Body(body)) => {
         println!("{body}");
         match serde_json::from_str::<serde_json::Value>(&body) {
            Ok(v) => {
               let snap = api::parse_usage(&v, now);
               println!("\nparsed:");
               println!("  five_hour: {:?}", snap.five_hour);
               println!("  seven_day: {:?}", snap.seven_day);
               0
            }
            Err(e) => {
               eprintln!("not JSON: {e}");
               1
            }
         }
      }
      Err(e) => {
         eprintln!("request failed: {e}");
         1
      }
   }
}

/// Print what every source says right now, side by side. The fastest way to
/// answer "why is the pill showing that number".
fn probe_sources() -> i32 {
   let now = state::now();
   let cfg = config::Config::load();

   let bridge = state::read().to_snapshot();
   let desktop = desktop::read().unwrap_or_default();
   let api = match api::read_token() {
      None => {
         println!("api      : no credentials file");
         model::Snapshot::default()
      }
      Some(t) if t.is_expired(now) => {
         println!(
            "api      : token expired at {}",
            pace::fmt_reset_full(t.expires_at_ms.unwrap_or(0) / 1000, "en")
         );
         model::Snapshot::default()
      }
      Some(t) => match api::fetch(&t.access_token, now) {
         Ok(s) => s,
         Err(e) => {
            println!("api      : {e}");
            model::Snapshot::default()
         }
      },
   };

   let sessions = {
      let live = session::live_sessions();
      let list = session::list(&live);
      match session::choose(&list, &cfg) {
         Some(e) => session::snapshot(&cfg, &state::read().context_limits, &e).unwrap_or_default(),
         None => {
            println!("session  : no session picked ({:?})", cfg.session_pick);
            model::Snapshot::default()
         }
      }
   };

   let show = |name: &str, snap: &model::Snapshot| {
      let one = |w: Option<model::Window>| match w {
         None => "-".to_string(),
         Some(w) => format!(
            "{:>3.0}%  resets {:<9}  seen {}",
            w.used_pct,
            w.resets_at
               .map(|r| pace::fmt_reset_full(r, "en"))
               .unwrap_or_else(|| "unknown".into()),
            pace::fmt_duration(now - w.observed_at)
         ),
      };
      let mut rows: Vec<String> = Vec::new();
      if snap.five_hour.is_some() || snap.seven_day.is_some() {
         rows.push(format!("5h  {}", one(snap.five_hour)));
         rows.push(format!("7d  {}", one(snap.seven_day)));
      }
      if let Some(c) = &snap.context {
         // The whole division, not only its answer. Two sources reporting
         // different percentages for one session is the diagnosis: it says
         // they disagree about the size of the window, not about the tokens.
         rows.push(format!(
            "ctx {:>3.0}%  {} of {} ({})  {}  session {}  seen {}",
            c.used_pct,
            c.used_tokens
               .map(pace::fmt_tokens)
               .unwrap_or_else(|| "?".into()),
            c.limit_tokens
               .map(pace::fmt_tokens)
               .unwrap_or_else(|| "?".into()),
            if c.limit_stated { "stated" } else { "table" },
            c.model.clone().unwrap_or_else(|| "?".into()),
            &c.session_id[..c.session_id.len().min(8)],
            pace::fmt_duration(now - c.observed_at)
         ));
      }
      // A source with nothing to say still says so: silence in this table
      // would look like a source that was not consulted.
      if rows.is_empty() {
         rows.push("-".to_string());
      }
      for (i, row) in rows.iter().enumerate() {
         match i {
            0 => println!("{name:<9}: {row}"),
            _ => println!("{:<9}  {row}", ""),
         }
      }
   };

   show("bridge", &bridge);
   show("desktop", &desktop);
   show("api", &api);
   show("session", &sessions);

   // The merged line answers "what would the pill show", so it obeys the
   // switches; the three above always report, which is the point of the
   // command.
   let pick = |on: bool, snap: model::Snapshot| {
      if on { snap } else { model::Snapshot::default() }
   };
   let merged = model::Snapshot::merge(
      model::Snapshot::merge(
         model::Snapshot::merge(pick(cfg.bridge_enabled, bridge), pick(cfg.api_enabled, api)),
         pick(cfg.desktop_enabled, desktop),
      ),
      pick(cfg.session_enabled, sessions),
   );
   show("merged", &merged);
   let on: Vec<&str> = config::SourceKind::ALL
      .iter()
      .filter(|k| cfg.enabled(**k))
      .map(|k| k.short())
      .collect();
   println!(
      "enabled  : {}{}",
      if on.is_empty() {
         "none".to_string()
      } else {
         on.join(" + ")
      },
      if cfg.demo { "  (demo figures on)" } else { "" }
   );
   0
}

/// Every conversation the picker would offer, with what each one reports.
///
/// This is how a session id gets into `config.toml` by hand, and the only place
/// the whole tree is read at once.
fn probe_sessions() -> i32 {
   let now = state::now();
   let cfg = config::Config::load();
   let learned = state::read().context_limits;
   let live = session::live_sessions();
   let mut list = session::list(&live);
   session::describe(&mut list);
   let chosen = session::choose(&list, &cfg).map(|e| e.id);

   if list.is_empty() {
      println!("no transcripts under ~/.claude/projects");
      return 1;
   }
   println!(
      "{} transcripts, {} running; picking the {:?} one",
      list.len(),
      live.len(),
      cfg.session_pick
   );
   for e in &list {
      let mark = if Some(&e.id) == chosen.as_ref() {
         "*"
      } else {
         " "
      };
      let live = if e.live { "live" } else { "    " };
      let r = session::reading(&e.path, Some(&e.id), e.modified);
      let figure = match &r {
         Some(r) => {
            let (limit, stated) =
               session::limit_for(r.model.as_deref(), r.used_tokens, &cfg, &learned);
            format!(
               "{:>8}  {:>3.0}% of {:<5} {:<6} {}",
               r.used_tokens,
               model::pct_of(r.used_tokens, limit),
               pace::fmt_tokens(limit),
               if stated { "stated" } else { "table" },
               r.model.clone().unwrap_or_else(|| "?".into())
            )
         }
         None => "       -".to_string(),
      };
      println!(
         "{mark} {} {live} {:<16} {:<24} {:>6}  {figure}",
         e.id,
         e.short_name(),
         e.project().unwrap_or_default(),
         pace::fmt_duration(now - e.modified),
      );
   }
   0
}
