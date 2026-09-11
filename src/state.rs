//! The file the bridge writes and the widget reads.
//!
//! Claude Code may have several sessions running at once, each invoking the
//! bridge independently, so writes go through a lock file and land atomically
//! via a temporary file plus rename.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::{ContextInfo, Snapshot, Source, Window};

/// Sessions untouched for this long are dropped from the state file.
const SESSION_TTL_SECS: i64 = 24 * 3600;
/// A lock older than this is presumed abandoned by a killed process.
const LOCK_STALE_SECS: u64 = 3;

pub fn state_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ccd-usage-monitor")
}

pub fn state_path() -> PathBuf {
    state_dir().join("state.json")
}

fn lock_path() -> PathBuf {
    state_dir().join("state.lock")
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------- on disk ---

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredWindow {
    pub used_percentage: f64,
    pub resets_at: i64,
    pub observed_at: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StoredLimits {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub five_hour: Option<StoredWindow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seven_day: Option<StoredWindow>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StoredSession {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_percent: Option<f64>,
    /// The numerator, so the widget can show the division rather than only its
    /// answer — and so the two sources can be compared on the count as well as
    /// on the percentage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub observed_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StateFile {
    pub version: u32,
    #[serde(default)]
    pub rate_limits: StoredLimits,
    #[serde(default)]
    pub sessions: BTreeMap<String, StoredSession>,
    /// Context-window size per model id, as the status line stated it.
    ///
    /// Kept apart from `sessions` because it is a fact about a model and
    /// outlives every session that taught it to us, so `prune` must not touch
    /// it. This is the only path by which a real denominator reaches the
    /// transcript reader, which otherwise has to infer one from the model name.
    #[serde(default)]
    pub context_limits: BTreeMap<String, u64>,
}

impl Default for StateFile {
    fn default() -> Self {
        StateFile {
            // Deliberately not bumped. Serde ignores unknown fields and fills
            // absent ones from `default`, so a new binary reads an old file and
            // an old binary reads a new one; a version bump would only invite a
            // migration that is not needed.
            version: 1,
            rate_limits: StoredLimits::default(),
            sessions: BTreeMap::new(),
            context_limits: BTreeMap::new(),
        }
    }
}

impl StateFile {
    /// Forget sessions nobody has touched in a day, and windows whose reset has
    /// already passed. Claude Code drops those from its payload too, and the
    /// percentage of a window that has rolled over means nothing.
    pub fn prune(&mut self, now: i64) {
        self.sessions
            .retain(|_, s| now - s.observed_at < SESSION_TTL_SECS);
        if let Some(w) = &self.rate_limits.five_hour {
            if w.resets_at <= now {
                self.rate_limits.five_hour = None;
            }
        }
        if let Some(w) = &self.rate_limits.seven_day {
            if w.resets_at <= now {
                self.rate_limits.seven_day = None;
            }
        }
    }

    /// The freshest session wins: with several windows open, the one being
    /// typed into is the one that reported last.
    pub fn to_snapshot(&self) -> Snapshot {
        let to_window = |w: &StoredWindow| Window {
            used_pct: w.used_percentage,
            resets_at: Some(w.resets_at),
            observed_at: w.observed_at,
            source: Source::Bridge,
        };
        let context = self
            .sessions
            .iter()
            .filter_map(|(id, s)| {
                let limit = s.context_limit.or_else(|| {
                    s.model.as_ref().and_then(|m| self.context_limits.get(m).copied())
                });
                s.context_percent.map(|pct| ContextInfo {
                    used_tokens: s.context_tokens,
                    limit_tokens: limit,
                    used_pct: pct,
                    // Claude Code told us this one; nothing here is a guess.
                    limit_stated: limit.is_some(),
                    model: s.model.clone(),
                    label: None,
                    session_id: id.clone(),
                    observed_at: s.observed_at,
                    source: Source::Bridge,
                })
            })
            .max_by_key(|c| c.observed_at);
        Snapshot {
            five_hour: self.rate_limits.five_hour.as_ref().map(to_window),
            seven_day: self.rate_limits.seven_day.as_ref().map(to_window),
            context,
        }
    }
}

// ------------------------------------------------------- statusLine input ---

#[derive(Debug, Deserialize)]
pub struct StatusLinePayload {
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub rate_limits: Option<PayloadLimits>,
    #[serde(default)]
    pub context_window: Option<PayloadContext>,
    #[serde(default)]
    pub model: Option<PayloadModel>,
}

#[derive(Debug, Deserialize)]
pub struct PayloadLimits {
    #[serde(default)]
    pub five_hour: Option<PayloadWindow>,
    #[serde(default)]
    pub seven_day: Option<PayloadWindow>,
}

#[derive(Debug, Deserialize)]
pub struct PayloadWindow {
    #[serde(default)]
    pub used_percentage: Option<f64>,
    #[serde(default)]
    pub resets_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct PayloadContext {
    /// Null early in a session, before the first API response.
    #[serde(default)]
    pub used_percentage: Option<f64>,
    /// The denominator, straight from Claude Code's server-fetched model
    /// catalogue. It is not recoverable from disk any other way: the caches in
    /// `~/.claude.json` that would hold it are null on a machine that has only
    /// ever run the desktop app.
    #[serde(default)]
    pub context_window_size: Option<u64>,
    /// The same sum the transcript reader computes, so the two sources can be
    /// compared on the numerator and not only on the answer.
    #[serde(default)]
    pub total_input_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct PayloadModel {
    #[serde(default)]
    pub id: Option<String>,
}

impl StatusLinePayload {
    /// Fold this tick of the payload into the stored state. Absent fields leave
    /// the previous value alone rather than blanking it: `rate_limits` is
    /// missing for the first tick of every session, and that is not news.
    pub fn apply_to(&self, state: &mut StateFile, now: i64) {
        if let Some(limits) = &self.rate_limits {
            if let Some(w) = limits.five_hour.as_ref().and_then(|w| stored(w, now)) {
                state.rate_limits.five_hour = Some(w);
            }
            if let Some(w) = limits.seven_day.as_ref().and_then(|w| stored(w, now)) {
                state.rate_limits.seven_day = Some(w);
            }
        }
        if let Some(id) = &self.session_id {
            let ctx = self.context_window.as_ref();
            let model = self.model.as_ref().and_then(|m| m.id.clone());
            let entry = state
                .sessions
                .entry(id.clone())
                .or_insert_with(StoredSession::default);
            entry.observed_at = now;
            if let Some(pct) = ctx.and_then(|c| c.used_percentage) {
                entry.context_percent = Some(pct);
            }
            if let Some(t) = ctx.and_then(|c| c.total_input_tokens) {
                entry.context_tokens = Some(t);
            }
            if let Some(size) = ctx.and_then(|c| c.context_window_size) {
                entry.context_limit = Some(size);
                // A fact about the model, remembered where it outlives the
                // session: this is what lets the transcript reader stop
                // guessing once a terminal session has run even once.
                if let Some(m) = &model {
                    state.context_limits.insert(m.clone(), size);
                }
            }
            if model.is_some() {
                entry.model = model;
            }
        }
    }
}

fn stored(w: &PayloadWindow, now: i64) -> Option<StoredWindow> {
    Some(StoredWindow {
        used_percentage: w.used_percentage?,
        resets_at: w.resets_at?,
        observed_at: now,
    })
}

// --------------------------------------------------------------- file i/o ---

pub fn read() -> StateFile {
    match fs::read_to_string(state_path()) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => StateFile::default(),
    }
}

/// Read-modify-write under a lock, then swap the file in atomically so a reader
/// never sees a half-written document.
pub fn update<F: FnOnce(&mut StateFile)>(f: F) -> std::io::Result<()> {
    let dir = state_dir();
    fs::create_dir_all(&dir)?;
    let _guard = Lock::acquire()?;

    let mut state = read();
    let now = now();
    f(&mut state);
    state.prune(now);

    let tmp = dir.join(format!("state.{}.tmp", std::process::id()));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(serde_json::to_string_pretty(&state)?.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, state_path())?;
    Ok(())
}

struct Lock(PathBuf);

impl Lock {
    fn acquire() -> std::io::Result<Lock> {
        let path = lock_path();
        for _ in 0..40 {
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Ok(Lock(path)),
                Err(_) => {
                    let stale = fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|m| m.elapsed().ok())
                        .map(|age| age.as_secs() >= LOCK_STALE_SECS)
                        .unwrap_or(false);
                    if stale {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    std::thread::sleep(Duration::from_millis(15));
                }
            }
        }
        // Better a rare interleaved write than a bridge that stalls the status
        // line: give up on the lock and proceed.
        Ok(Lock(path))
    }
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "session_id": "abc",
      "model": { "id": "claude-opus-5" },
      "context_window": {
        "used_percentage": 38,
        "context_window_size": 1000000,
        "total_input_tokens": 380000
      },
      "rate_limits": {
        "five_hour": { "used_percentage": 23.5, "resets_at": 1738425600 },
        "seven_day": { "used_percentage": 41.2, "resets_at": 1738857600 }
      }
    }"#;

    #[test]
    fn parses_the_documented_payload() {
        let p: StatusLinePayload = serde_json::from_str(SAMPLE).unwrap();
        let mut s = StateFile::default();
        p.apply_to(&mut s, 1_738_400_000);
        assert_eq!(s.rate_limits.five_hour.unwrap().used_percentage, 23.5);
        assert_eq!(s.rate_limits.seven_day.unwrap().resets_at, 1_738_857_600);
        assert_eq!(s.sessions["abc"].context_percent, Some(38.0));
    }

    #[test]
    fn the_payload_states_the_window_size_and_it_is_remembered_per_model() {
        let p: StatusLinePayload = serde_json::from_str(SAMPLE).unwrap();
        let mut s = StateFile::default();
        p.apply_to(&mut s, 1_738_400_000);
        assert_eq!(s.sessions["abc"].context_tokens, Some(380_000));
        assert_eq!(s.sessions["abc"].context_limit, Some(1_000_000));
        assert_eq!(
            s.context_limits.get("claude-opus-5"),
            Some(&1_000_000),
            "the size is a fact about the model, kept where the transcript reader can find it"
        );
        let ctx = s.to_snapshot().context.unwrap();
        assert!(ctx.limit_stated, "a size Claude Code named is not a guess");
    }

    #[test]
    fn a_learned_window_size_survives_pruning() {
        let mut s = StateFile::default();
        let p: StatusLinePayload = serde_json::from_str(SAMPLE).unwrap();
        p.apply_to(&mut s, 100);
        s.prune(100 + SESSION_TTL_SECS + 1);
        assert!(s.sessions.is_empty(), "the session is long gone");
        assert_eq!(
            s.context_limits.get("claude-opus-5"),
            Some(&1_000_000),
            "but what it taught us about the model is not"
        );
    }

    #[test]
    fn a_payload_without_rate_limits_keeps_what_we_had() {
        let mut s = StateFile::default();
        let first: StatusLinePayload = serde_json::from_str(SAMPLE).unwrap();
        first.apply_to(&mut s, 1_738_400_000);

        let bare: StatusLinePayload = serde_json::from_str(
            r#"{"session_id":"abc","context_window":{"used_percentage":null}}"#,
        )
        .unwrap();
        bare.apply_to(&mut s, 1_738_400_100);

        assert!(
            s.rate_limits.five_hour.is_some(),
            "windows must survive a bare tick"
        );
        assert_eq!(s.sessions["abc"].context_percent, Some(38.0));
    }

    #[test]
    fn snapshot_prefers_the_session_that_reported_last() {
        let mut s = StateFile::default();
        s.sessions.insert(
            "old".into(),
            StoredSession {
                context_percent: Some(10.0),
                observed_at: 100,
                ..StoredSession::default()
            },
        );
        s.sessions.insert(
            "new".into(),
            StoredSession {
                context_percent: Some(70.0),
                observed_at: 200,
                ..StoredSession::default()
            },
        );
        let snap = s.to_snapshot();
        assert_eq!(snap.context.unwrap().session_id, "new");
    }

    #[test]
    fn prune_drops_spent_windows_and_dead_sessions() {
        let now = 1_000_000;
        let mut s = StateFile::default();
        s.rate_limits.five_hour = Some(StoredWindow {
            used_percentage: 50.0,
            resets_at: now - 1,
            observed_at: now - 10,
        });
        s.rate_limits.seven_day = Some(StoredWindow {
            used_percentage: 50.0,
            resets_at: now + 86400,
            observed_at: now - 10,
        });
        s.sessions.insert(
            "stale".into(),
            StoredSession {
                context_percent: Some(1.0),
                observed_at: now - SESSION_TTL_SECS - 1,
                ..StoredSession::default()
            },
        );
        s.prune(now);
        assert!(s.rate_limits.five_hour.is_none());
        assert!(s.rate_limits.seven_day.is_some());
        assert!(s.sessions.is_empty());
    }
}
