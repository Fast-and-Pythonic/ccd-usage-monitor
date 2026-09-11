//! The four sources, pumped and merged — everything the widget knows, and
//! nothing about how it looks.
//!
//! This module deliberately contains no reference to egui. It is the half of
//! the old `App` that survives a change of GUI: file watching, the poller's
//! channel, the merge across sources (A03), age filtering (A07, A14) and the
//! log of past readings (A17). What draws the result is somebody else's
//! problem.
//!
//! The config is passed in rather than held, because the settings UI owns it
//! and edits it in place; a `Runtime` that kept its own copy would need a way
//! to be told about every change.

use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant, SystemTime};

use crate::api::{self, ApiEvent};
use crate::config::{Config, SessionPick, SourceKind};
use crate::desktop;
use crate::history;
use crate::model::{ContextInfo, Snapshot, Source, Window};
use crate::pace;
use crate::session;
use crate::state;

/// How long a reading from `source` stays current.
///
/// Each source ages at its own rate (A14): the endpoint answers in seconds, so
/// a fifteen-minute-old reading from it means something is wrong, while the
/// desktop history is written every quarter of an hour by design and calling
/// that stale is blaming a source for how it works.
pub fn stale_secs(cfg: &Config, source: Source) -> i64 {
    match source {
        Source::Bridge => cfg.stale_secs(SourceKind::Bridge),
        Source::Api => cfg.stale_secs(SourceKind::Api),
        Source::Desktop => cfg.stale_secs(SourceKind::Desktop),
        Source::Session => cfg.stale_secs(SourceKind::Session),
        // Synthetic figures never go stale; the point of them is to sit still
        // while the layout is worked on.
        Source::Demo => i64::MAX,
    }
}

/// Synthetic figures, for working on the layout.
pub fn demo_snapshot(now: i64) -> Snapshot {
    Snapshot {
        five_hour: Some(Window {
            used_pct: 24.0,
            resets_at: Some(now + 3 * 3600 + 37 * 60),
            observed_at: now,
            source: Source::Demo,
        }),
        seven_day: Some(Window {
            used_pct: 54.0,
            resets_at: Some(now + 3 * 86400 + 5 * 3600),
            observed_at: now,
            source: Source::Demo,
        }),
        context: Some(ContextInfo {
            used_tokens: Some(382_000),
            limit_tokens: Some(1_000_000),
            used_pct: 38.0,
            limit_stated: true,
            model: Some("claude-opus-5".into()),
            label: Some("demo".into()),
            session_id: "demo".into(),
            observed_at: now,
            source: Source::Demo,
        }),
    }
}

pub struct Runtime {
    pub bridge: Snapshot,
    pub api: Snapshot,
    api_rx: Option<Receiver<ApiEvent>>,
    api_wake: Option<std::sync::mpsc::Sender<api::Wake>>,
    pub api_status: Option<String>,
    pub api_last_ok: Option<i64>,
    pub api_polling: bool,
    /// Set while the endpoint has told us to stay away.
    pub api_retry_until: Option<i64>,
    /// Set while our own request budget is spent; the request still happens.
    pub api_wait_until: Option<i64>,
    /// The interval and budget the running poller reads on every lap.
    api_settings: Option<std::sync::Arc<api::PollerSettings>>,
    last_wake_request: Option<Instant>,
    pub desktop: Snapshot,
    pub desktop_mtime: Option<SystemTime>,
    /// The context figure read out of a session transcript.
    pub session: Snapshot,
    /// mtime of the chosen transcript. Between scans this one file is the only
    /// thing looked at, which is what keeps a source that watches eighty-odd
    /// files off the polling path.
    pub session_mtime: Option<SystemTime>,
    pub session_scan: Option<Instant>,
    /// The transcripts as of that scan, newest first; also what the picker
    /// shows.
    pub session_list: Vec<session::SessionEntry>,
    pub session_chosen: Option<session::SessionEntry>,
    /// Window sizes the status line has stated, per model, out of the state
    /// file. The one path by which the transcript reader stops guessing.
    pub context_limits: std::collections::BTreeMap<String, u64>,
    pub state_mtime: Option<SystemTime>,
    pub last_state_check: Instant,
    /// Past readings, for the burn rate. Seeded once at startup.
    pub history: history::Log,
}

impl Runtime {
    pub fn new() -> Runtime {
        // The Claude app's own history makes the rate answerable on the first
        // minute rather than after an hour of watching (see `history`).
        let mut log = history::load();
        let seeded = log.merge(desktop::read_samples());
        log.prune(state::now());
        if seeded {
            history::save(&log);
        }
        Runtime { history: log, ..Runtime::blank() }
    }

    /// A runtime that has read nothing at all.
    ///
    /// `new` is this plus the history log, which is the one part of building a
    /// `Runtime` that touches the disk. Anything testing what the widget
    /// *says* wants this instead: a test built on `new` would quietly depend on
    /// whatever `samples.json` happens to hold on the machine running it, and
    /// would pass or fail by accident.
    pub fn blank() -> Runtime {
        Runtime {
            bridge: Snapshot::default(),
            api: Snapshot::default(),
            api_rx: None,
            api_wake: None,
            api_status: None,
            api_last_ok: None,
            api_polling: false,
            api_retry_until: None,
            api_wait_until: None,
            api_settings: None,
            last_wake_request: None,
            desktop: Snapshot::default(),
            desktop_mtime: None,
            state_mtime: None,
            session: Snapshot::default(),
            session_mtime: None,
            session_scan: None,
            session_list: Vec::new(),
            session_chosen: None,
            context_limits: std::collections::BTreeMap::new(),
            last_state_check: Instant::now() - Duration::from_secs(1),
            history: history::Log::default(),
        }
    }

    /// Forget every remembered mtime, so the next `pump` re-reads all of them.
    ///
    /// What the settings window calls "refresh now": a source that was
    /// switched on, or a setting that changes what a file means, leaves the
    /// remembered mtimes describing answers to a different question.
    pub fn invalidate_files(&mut self) {
        self.state_mtime = None;
        self.desktop_mtime = None;
        self.session_mtime = None;
        self.session_scan = None;
        self.last_state_check = Instant::now() - Duration::from_secs(1);
    }

    pub fn pump(&mut self, cfg: &Config, now: i64) {
        if cfg.demo {
            return;
        }

        if self.last_state_check.elapsed() >= Duration::from_millis(250) {
            self.last_state_check = Instant::now();

            if cfg.bridge_enabled {
                let mtime = std::fs::metadata(state::state_path())
                    .and_then(|m| m.modified())
                    .ok();
                if mtime != self.state_mtime {
                    self.state_mtime = mtime;
                    let file = state::read();
                    // Learned window sizes travel with the state file rather
                    // than with the snapshot: they are facts about models, and
                    // the transcript reader needs them even when the bridge
                    // itself has nothing to say today.
                    self.context_limits = file.context_limits.clone();
                    self.bridge = file.to_snapshot();
                }
            }

            if cfg.desktop_enabled {
                let mtime = desktop::history_path()
                    .and_then(|p| std::fs::metadata(p).ok())
                    .and_then(|m| m.modified().ok());
                if mtime != self.desktop_mtime {
                    self.desktop_mtime = mtime;
                    let previous = self.desktop.five_hour.map(|w| w.used_pct);
                    if let Some(snap) = desktop::read() {
                        // A file whose timestamp moved without its figures
                        // moving has nothing to tell the endpoint. This is not
                        // a rare case: `invalidate_files` forgets every mtime
                        // deliberately — a re-read config does it (A23), and
                        // so does the refresh button — and a reading that came
                        // back identical must not cost a request (G14).
                        let same = snap.five_hour == self.desktop.five_hour
                            && snap.seven_day == self.desktop.seven_day;
                        // A new sample means the figures moved. A percentage
                        // that fell means a window rolled over, so the reset
                        // times the API gave us are now wrong as well.
                        let rolled = match (previous, snap.five_hour) {
                            (Some(before), Some(w)) => w.used_pct < before,
                            _ => false,
                        };
                        self.desktop = snap;
                        if !same {
                            self.request_api_refresh(cfg, rolled || previous.is_none());
                        }
                    }
                }
            }

            if cfg.session_enabled {
                // The tree is enumerated on a timer, not on the tick. Sixteen
                // directories and eighty-odd files is nothing once every half
                // minute and a great deal four times a second; between scans
                // the only syscall this source makes is one `metadata` on the
                // file it has already chosen.
                let due = self
                    .session_scan
                    .is_none_or(|t| t.elapsed() >= Duration::from_secs(cfg.session_scan_secs()));
                if due {
                    self.session_scan = Some(Instant::now());
                    self.session_list = session::list(&session::live_sessions());
                    let chosen = session::choose(&self.session_list, cfg);
                    // A different session means the remembered mtime says
                    // nothing about the new file.
                    let same = match (&chosen, &self.session_chosen) {
                        (Some(a), Some(b)) => a.id == b.id,
                        (None, None) => true,
                        _ => false,
                    };
                    if !same {
                        self.session_mtime = None;
                        self.session = Snapshot::default();
                    }
                    self.session_chosen = chosen;
                }
                match &self.session_chosen {
                    Some(entry) => {
                        let mtime = std::fs::metadata(&entry.path)
                            .and_then(|m| m.modified())
                            .ok();
                        if mtime != self.session_mtime {
                            self.session_mtime = mtime;
                            self.session = session::snapshot(cfg, &self.context_limits, entry)
                                .unwrap_or_default();
                        }
                    }
                    None => self.session = Snapshot::default(),
                }
            }
        }

        // The poller starts the first time the source is switched on, so
        // leaving it off never touches the credentials file. It is never
        // restarted after that: its interval and budget are shared values it
        // re-reads every lap, because a fresh thread would begin with a
        // request and editing a setting would cost one (G14).
        let wanted = (cfg.api_interval(), cfg.api_budget());
        if cfg.api_enabled && self.api_rx.is_none() {
            let settings = std::sync::Arc::new(api::PollerSettings::new(wanted.0, wanted.1));
            let (tx, rx) = channel();
            let wake = api::spawn_poller(settings.clone(), tx);
            self.api_rx = Some(rx);
            self.api_wake = Some(wake);
            self.api_settings = Some(settings);
        }
        if let Some(settings) = &self.api_settings {
            if settings.get() != wanted {
                settings.set(wanted.0, wanted.1);
                // Only "look at the clock again" — the poller works out for
                // itself whether anything is due, and usually nothing is.
                if let Some(wake) = &self.api_wake {
                    let _ = wake.send(api::Wake::Recheck);
                }
            }
        }
        if !cfg.api_enabled && self.api_rx.is_some() {
            self.api_rx = None;
            self.api_wake = None;
            self.api_settings = None;
            self.api_polling = false;
        }
        if let Some(rx) = &self.api_rx {
            while let Ok(event) = rx.try_recv() {
                match event {
                    ApiEvent::Polling => self.api_polling = true,
                    ApiEvent::Data(snap) => {
                        // The age comes from the snapshot rather than from the
                        // moment this event was picked up, so the `src` line and
                        // the window lines below it cannot disagree (G13).
                        self.api_last_ok = snap
                            .five_hour
                            .or(snap.seven_day)
                            .map(|w| w.observed_at)
                            .or(Some(now));
                        self.api = snap;
                        self.api_status = None;
                        self.api_retry_until = None;
                        self.api_wait_until = None;
                        self.api_polling = false;
                    }
                    ApiEvent::Unavailable(msg) => {
                        self.api_status = Some(msg);
                        self.api_polling = false;
                    }
                    ApiEvent::RateLimited { until } => {
                        self.api_retry_until = Some(until);
                        self.api_wait_until = None;
                        self.api_status = None;
                        self.api_polling = false;
                    }
                    ApiEvent::Throttled { until } => {
                        self.api_wait_until = Some(until);
                        self.api_polling = false;
                    }
                }
            }
        }

        // What the pill would show right now, remembered. Demo figures never
        // get here: `pump` returns before any of this when demo is on.
        let snap = self.snapshot(cfg, now);
        let changed = self.history.record(
            now,
            snap.five_hour.map(|w| w.used_pct),
            snap.seven_day.map(|w| w.used_pct),
        );
        if changed {
            self.history.prune(now);
            history::save(&self.history);
        }
    }

    /// Percent per hour for a window, and what it means for the reset.
    ///
    /// The lookback differs by window because the questions do: a five-hour
    /// budget is spent in an afternoon, so the last hour is what matters, while
    /// an hour tells nothing about a week.
    pub fn burn_rate(&self, w: Window, span: i64, now: i64) -> Option<f64> {
        let resets_at = w.resets_at?;
        let lookback = if span == pace::FIVE_HOUR_SECS { 3600 } else { 86400 };
        if span == pace::FIVE_HOUR_SECS {
            self.history.rate(|s| s.fh, resets_at - span, now, lookback)
        } else {
            self.history.rate(|s| s.sd, resets_at - span, now, lookback)
        }
    }

    /// Ask the poller to go now instead of at the end of its interval.
    ///
    /// `force` skips the cool-down, but nothing skips an active rate limit:
    /// the endpoint answers 429 when asked too often, and the way to make that
    /// worse is to keep asking. Unforced wake-ups are also dropped when the
    /// last answer is younger than a minute — there is nothing new to hear.
    pub fn request_api_refresh(&mut self, cfg: &Config, force: bool) {
        if !cfg.api_enabled {
            return;
        }
        let now = state::now();
        if self.api_retry_until.is_some_and(|until| until > now) {
            return;
        }
        if !force {
            if let Some(last) = self.last_wake_request {
                if last.elapsed() < Duration::from_secs(120) {
                    return;
                }
            }
            if self.api_last_ok.is_some_and(|ok| now - ok < 60) {
                return;
            }
        }
        self.last_wake_request = Some(Instant::now());
        if let Some(wake) = &self.api_wake {
            let _ = wake.send(api::Wake::Now);
        }
    }

    /// Drop readings too old to mean anything and report whether what is left
    /// is merely stale. A five-hour percentage from an hour ago is not a
    /// slightly-off number, it is a different window.
    ///
    /// Returns the figures worth showing, whether the windows are stale, and
    /// whether the context figure is — two flags, because the two age on
    /// completely different clocks and one dot for both would lie about one.
    pub fn age_filter(&self, cfg: &Config, snap: Snapshot, now: i64) -> (Snapshot, bool, bool) {
        let drop_after = cfg.drop_after_secs;
        let fresh = |w: Option<Window>| w.filter(|w| now - w.observed_at <= drop_after);
        let five = fresh(snap.five_hour);
        let seven = fresh(snap.seven_day);
        let unspent = |w: Option<Window>| w.filter(|w| w.resets_at.is_none_or(|r| r > now));
        let five = unspent(five);
        let seven = unspent(seven);
        // Each source ages at its own rate, so each carries its own threshold:
        // the desktop history is written every quarter of an hour and calling
        // it stale for that would be blaming it for how it works.
        let stale = [five, seven]
            .into_iter()
            .flatten()
            .any(|w| now - w.observed_at > stale_secs(cfg, w.source));
        // Context is not dropped for age, unlike the windows. A five-hour
        // percentage from an hour ago describes a window that has moved on; a
        // conversation's fill is a fact about that conversation and stays true
        // until someone types in it again. Blanking it after twenty quiet
        // minutes would empty the field exactly when it is being read.
        let ctx_stale = snap
            .context
            .as_ref()
            .is_some_and(|c| now - c.observed_at > stale_secs(cfg, c.source));
        (
            Snapshot {
                five_hour: five,
                seven_day: seven,
                context: snap.context,
            },
            stale,
            ctx_stale,
        )
    }

    /// Whatever the enabled sources have between them, merged field by field
    /// (A03). A source that is switched off contributes nothing, which is the
    /// whole point of being able to switch it off.
    pub fn snapshot(&self, cfg: &Config, now: i64) -> Snapshot {
        if cfg.demo {
            return demo_snapshot(now);
        }
        let pinned = match cfg.session_pick {
            SessionPick::Pinned if !cfg.session_id.is_empty() => Some(&cfg.session_id),
            _ => None,
        };
        let pick = |on: bool, snap: &Snapshot| {
            if !on {
                return Snapshot::default();
            }
            let mut snap = snap.clone();
            // A pinned session is a question about one conversation, so a
            // source reporting on a different one must not answer it — merging
            // by age alone would let two sources take turns.
            if let Some(id) = pinned {
                snap.context = snap.context.filter(|c| &c.session_id == id);
            }
            snap
        };
        Snapshot::merge(
            Snapshot::merge(
                Snapshot::merge(
                    pick(cfg.bridge_enabled, &self.bridge),
                    pick(cfg.api_enabled, &self.api),
                ),
                pick(cfg.desktop_enabled, &self.desktop),
            ),
            pick(cfg.session_enabled, &self.session),
        )
    }
}
