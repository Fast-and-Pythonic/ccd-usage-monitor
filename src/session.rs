//! Fourth source: the conversation transcripts Claude Code writes for itself.
//!
//! `~/.claude/projects/<encoded-cwd>/<session-uuid>.jsonl` is a line of JSON per
//! event, appended as the conversation happens. Assistant lines carry the token
//! counters, and the sum of the three input counters is what sits in the context
//! window:
//!
//! ```json
//! {"type":"assistant","sessionId":"…","isSidechain":false,"timestamp":"…",
//!  "slug":"…","cwd":"…","message":{"model":"claude-opus-5","usage":{
//!    "input_tokens":2,"cache_creation_input_tokens":817,
//!    "cache_read_input_tokens":88139,"output_tokens":569}}}
//! ```
//!
//! That formula is not inferred. It is what Claude Code's own status-line code
//! does with the same numbers, so the widget divides exactly what the app
//! divides — and `output_tokens` is deliberately outside the sum, because the
//! shipped code reports it separately and does not count it against the window.
//!
//! **Why this source exists at all.** The status line is a terminal feature and
//! the desktop app never runs it (G06), which left the context figure missing
//! for anyone working in the app. These files are written whatever the client
//! is, so this is the source that works everywhere — at the cost of the one
//! thing the status line states outright and a file on disk does not: the size
//! of the window the tokens are counted against.
//!
//! The format is undocumented, so every field is optional and anything
//! unexpected means this source stays quiet rather than guessing.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::config::{Config, CtxLimit, SessionPick};
use crate::model::{ContextInfo, Snapshot, Source, pct_of};

/// The smallest tail worth reading, and the largest worth reading before giving
/// up on a file for this tick.
///
/// A transcript reaches thirty megabytes and is appended to while Claude Code
/// holds it open, so the whole file is never read. A hundred and twenty-eight
/// kilobytes usually spans several turns — but a single line in a real
/// transcript here runs to 1 259 844 bytes, so a fixed window is a source that
/// reports nothing on exactly the days it matters. The window doubles until a
/// qualifying line turns up or the file runs out.
const TAIL_MIN: u64 = 128 * 1024;
const TAIL_MAX: u64 = 4 * 1024 * 1024;

/// How much of the head is read to recover a session's name and directory. The
/// encoded directory name is not reversible — `C--Home-0-My-Programming-------`
/// stands for a path with Cyrillic in it — so the real one costs a file open.
const HEAD_BYTES: u64 = 32 * 1024;

/// How long a session's name may be before it is cut. Derived slugs run to
/// fifty characters and would push everything else off a settings row.
const NAME_MAX: usize = 22;

/// Window sizes we know without being told, from the changelog Claude Code
/// caches: the 5-series is natively a million tokens, everything before it two
/// hundred thousand. Matched by prefix, because ids carry dated suffixes.
///
/// This table is the weakest link in the feature and is meant to be read as
/// one: `CLAUDE_CODE_DISABLE_1M_CONTEXT` holds a 5-series model to 200K with
/// nothing on disk to say so, and a model shipped after this build is unknown.
/// Both cases read five times wrong, in opposite directions. That is why the
/// token count can stand on the pill beside the percentage, and why a size the
/// status line stated always wins.
const MODEL_WINDOWS: &[(&str, u64)] = &[
    ("claude-opus-5", 1_000_000),
    ("claude-sonnet-5", 1_000_000),
    ("claude-fable-5", 1_000_000),
];
const DEFAULT_WINDOW: u64 = 200_000;
/// The range the shipped binary clamps its own fetched size to.
const WINDOW_MIN: u64 = 8_192;
const WINDOW_MAX: u64 = 1_000_000;

pub fn projects_dir() -> Option<PathBuf> {
    Some(crate::api::claude_dir()?.join("projects"))
}

pub fn sessions_dir() -> Option<PathBuf> {
    Some(crate::api::claude_dir()?.join("sessions"))
}

// ------------------------------------------------------------- the registry ---

/// One entry of Claude Code's live-session registry, `~/.claude/sessions`.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSession {
    pub session_id: String,
    #[serde(default)]
    pub cwd: Option<String>,
    /// A short derived name — `txts-d8`. The one string here a person reads.
    #[serde(default)]
    pub name: Option<String>,
}

/// The registry, minus its litter.
///
/// Beside each `<pid>.json` sits a `<pid>.<hash>.key` that is not a session, so
/// the extension is the whole filter. And a file here means "a session
/// registered and did not clean up", which is not quite "a session is running":
/// a killed process leaves its entry behind, and the file's own `updatedAt` is
/// stamped near the start of the session rather than repeatedly, so it is no
/// heartbeat either. Everything downstream says "running or recently running"
/// and means it.
pub fn live_sessions() -> Vec<LiveSession> {
    let Some(dir) = sessions_dir() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(s) = serde_json::from_str::<LiveSession>(&text) {
                out.push(s);
            }
        }
    }
    out
}

// ------------------------------------------------------------ the transcripts ---

/// A transcript we could read a context figure out of.
#[derive(Clone, Debug)]
pub struct SessionEntry {
    /// The session uuid, which is also the file stem.
    pub id: String,
    pub path: PathBuf,
    /// Unix seconds of the last write, near enough the last assistant turn.
    pub modified: i64,
    /// The registry lists this session as running.
    pub live: bool,
    /// From the registry when live; filled in from the transcript itself only
    /// when something asks to see a list.
    pub name: Option<String>,
    pub cwd: Option<String>,
}

impl SessionEntry {
    /// The shortest string that still identifies this session to a person: the
    /// registry's name, else the transcript's slug, else the head of the uuid.
    pub fn short_name(&self) -> String {
        let full = match &self.name {
            Some(n) if !n.is_empty() => n.clone(),
            _ => return self.id.chars().take(8).collect(),
        };
        // Slugs run to fifty characters when they are derived from a path, and
        // a settings row is not that wide.
        match full.char_indices().nth(NAME_MAX) {
            Some((at, _)) => format!("{}…", &full[..at]),
            None => full,
        }
    }

    /// The last component of the working directory — the project, as a person
    /// thinks of it.
    pub fn project(&self) -> Option<String> {
        let cwd = self.cwd.as_ref()?;
        let leaf = cwd
            .rsplit(|c| c == '\\' || c == '/')
            .find(|s| !s.is_empty())?;
        Some(leaf.to_string())
    }
}

/// Every session transcript, newest first.
///
/// One level deep on purpose. Under `<encoded-cwd>/` there is also a `memory/`
/// directory and, for sessions that spawned subagents, a `<uuid>/subagents/`
/// tree of sidechain transcripts carrying the *same* `sessionId`. Walking the
/// tree would list each of those as a session of its own and let a subagent's
/// context stand in for the conversation's.
///
/// A directory that cannot be read is skipped rather than emptying the list:
/// the encoded names are long and deeply nested, and one unreadable path is not
/// an answer of "there are no sessions".
pub fn list(live: &[LiveSession]) -> Vec<SessionEntry> {
    let Some(root) = projects_dir() else {
        return Vec::new();
    };
    let Ok(dirs) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<SessionEntry> = Vec::new();
    for dir in dirs.flatten() {
        if !dir.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|s| s.to_str()).map(str::to_owned) else {
                continue;
            };
            // On Windows this metadata is served out of the directory
            // enumeration itself rather than by opening the file, which is what
            // makes a full rescan cheap enough to put on a timer.
            let modified = file
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let known = live.iter().find(|l| l.session_id == id);
            out.push(SessionEntry {
                id,
                path,
                modified,
                live: known.is_some(),
                name: known.and_then(|l| l.name.clone()),
                cwd: known.and_then(|l| l.cwd.clone()),
            });
        }
    }
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out
}

/// Which of them the widget is reading.
///
/// `Live` does not fall back to the freshest transcript when the registry is
/// empty. A07's rule applies: `--%` is worth more than a true figure about a
/// conversation nobody is in.
pub fn choose(list: &[SessionEntry], cfg: &Config) -> Option<SessionEntry> {
    match cfg.session_pick {
        SessionPick::Pinned => list.iter().find(|e| e.id == cfg.session_id).cloned(),
        SessionPick::Live => list.iter().find(|e| e.live).cloned(),
        SessionPick::Freshest => list.first().cloned(),
    }
}

/// Fill in the name and directory of entries that the registry did not name, by
/// reading the head of each transcript.
///
/// Only for showing a list. Never on the polling path: it opens every file.
pub fn describe(entries: &mut [SessionEntry]) {
    for e in entries.iter_mut() {
        if e.name.is_some() && e.cwd.is_some() {
            continue;
        }
        let Some((bytes, _)) = read_head(&e.path, HEAD_BYTES) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        for line in text.split('\n') {
            let Ok(l) = serde_json::from_str::<Line>(line) else {
                continue;
            };
            if e.name.is_none() {
                e.name = l.slug.clone();
            }
            if e.cwd.is_none() {
                e.cwd = l.cwd.clone();
            }
            if e.name.is_some() && e.cwd.is_some() {
                break;
            }
        }
    }
}

// ------------------------------------------------------------------ reading ---

/// What one transcript's last real assistant turn reports.
#[derive(Clone, Debug)]
pub struct Reading {
    pub used_tokens: u64,
    pub model: Option<String>,
    /// The line's own timestamp, not the moment we read it. A conversation
    /// abandoned an hour ago must look an hour old.
    pub observed_at: i64,
    pub slug: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Line {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    is_sidechain: Option<bool>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    message: Option<Message>,
}

#[derive(Debug, Deserialize)]
struct Message {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<Usage>,
}

/// Missing counters are zero, which is what Claude Code's own code does with an
/// absent usage object.
#[derive(Debug, Default, Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
}

impl Usage {
    /// What sits in the context window: the three input counters, and not the
    /// output. Adding the output would overstate every reading.
    fn in_window(&self) -> u64 {
        self.input_tokens + self.cache_creation_input_tokens + self.cache_read_input_tokens
    }
}

/// The last `want` bytes of a file, and whether the first of them is a
/// fragment.
///
/// Opened for reading only, which on Windows shares read, write and delete — so
/// this never blocks Claude Code's own append, and its hold never blocks this.
/// The length comes from one call and the read from another, so the file can
/// change in between: grown, we read short and miss one turn for one tick;
/// truncated by a session replay, the seek lands past the end and the read
/// returns nothing, which is silence rather than a confident zero.
fn read_tail(path: &Path, want: u64) -> Option<(Vec<u8>, bool)> {
    let mut file = std::fs::File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len == 0 {
        return None;
    }
    let start = len.saturating_sub(want);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity((len - start) as usize);
    file.read_to_end(&mut buf).ok()?;
    if buf.is_empty() {
        return None;
    }
    Some((buf, start > 0))
}

fn read_head(path: &Path, want: u64) -> Option<(Vec<u8>, bool)> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; want as usize];
    let read = file.read(&mut buf).ok()?;
    buf.truncate(read);
    if buf.is_empty() {
        return None;
    }
    Some((buf, false))
}

/// The complete lines of a tail, newest first.
///
/// Two hazards, both from cutting a file at an arbitrary byte. The cut lands
/// inside a UTF-8 sequence about half the time — these transcripts carry
/// Cyrillic paths on every line — so the bytes are decoded lossily rather than
/// strictly, which would throw a whole tail away over one path. And the first
/// line is then a fragment, discarded outright: a fragment of a JSON object is
/// usually invalid and skipped by the parser anyway, but "usually" is not a
/// rule to build on, and a fragment that happened to parse would be a reading
/// of half a message.
fn lines_back(bytes: &[u8], cut: bool) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    let mut lines: Vec<&str> = text.split('\n').collect();
    if cut && !lines.is_empty() {
        lines.remove(0);
    }
    lines
        .into_iter()
        .rev()
        .map(|l| l.trim_end_matches('\r').to_string())
        .filter(|l| l.len() > 1)
        .collect()
}

/// The context figure a transcript's tail reports, or nothing.
///
/// The last qualifying line by position wins, which is what Claude Code's own
/// footer effectively shows. By position and not by timestamp on purpose:
/// replaying a session writes lines whose timestamps run backwards, and sorting
/// by them would resurrect a figure the conversation has moved past.
///
/// A line qualifies when all of this holds:
///
/// * `type == "assistant"`, so user turns and the `system`, `mode` and
///   `attachment` lines are out;
/// * `isSidechain != true` — a subagent's transcript carries the *parent's*
///   `sessionId`, and its context is not the conversation's;
/// * `message.model != "<synthetic>"` — an error or interrupt line is written
///   with that model and all-zero counters, and taking it turns a busy session
///   into a confident nought per cent;
/// * the counters sum above zero, which catches the same case from the other
///   side for any line shape we have not met.
///
/// Several lines share one `message.id` and repeat one API message's usage
/// verbatim, so taking the last is also taking each message once; summing them
/// would multiply the reading by the number of content blocks.
///
/// Compaction needs nothing here. The reset is already inside
/// `cache_read_input_tokens` of the first turn after the boundary.
pub fn parse_tail(lines: &[String], want_id: Option<&str>, fallback_at: i64) -> Option<Reading> {
    for line in lines {
        let Ok(l) = serde_json::from_str::<Line>(line) else {
            continue;
        };
        if l.kind.as_deref() != Some("assistant") || l.is_sidechain == Some(true) {
            continue;
        }
        if let (Some(want), Some(have)) = (want_id, l.session_id.as_deref()) {
            if want != have {
                continue;
            }
        }
        let Some(msg) = l.message else { continue };
        if msg.model.as_deref() == Some("<synthetic>") {
            continue;
        }
        let used = msg.usage.unwrap_or_default().in_window();
        if used == 0 {
            continue;
        }
        let observed_at = l
            .timestamp
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|d| d.timestamp())
            .unwrap_or(fallback_at);
        return Some(Reading {
            used_tokens: used,
            model: msg.model,
            observed_at,
            slug: l.slug,
        });
    }
    None
}

/// Read one transcript, growing the tail until a reading turns up.
pub fn reading(path: &Path, want_id: Option<&str>, fallback_at: i64) -> Option<Reading> {
    let mut want = TAIL_MIN;
    loop {
        let (bytes, cut) = read_tail(path, want)?;
        let lines = lines_back(&bytes, cut);
        if let Some(r) = parse_tail(&lines, want_id, fallback_at) {
            return Some(r);
        }
        // The whole file has been read, or we have looked far enough back that
        // going further would cost more than the figure is worth this tick.
        if !cut || want >= TAIL_MAX {
            return None;
        }
        want *= 2;
    }
}

// ---------------------------------------------------------- the denominator ---

/// The size of the context window a model is talking through, and whether that
/// size was told to us rather than guessed.
///
/// Precedence, most authoritative first: a size the user fixed by hand, a size
/// the status line stated for this exact model, the table, and two hundred
/// thousand for a model nobody has heard of.
///
/// Then raised if the reading itself exceeds it — a conversation that has
/// counted four hundred thousand tokens is not in a two-hundred-thousand-token
/// window, whatever we thought. Not raised when the user fixed the number: that
/// is their answer, and quietly overruling it would be worse than a percentage
/// pinned at a hundred.
pub fn limit_for(
    model: Option<&str>,
    used: u64,
    cfg: &Config,
    learned: &BTreeMap<String, u64>,
) -> (u64, bool) {
    if cfg.ctx_limit == CtxLimit::Fixed {
        return (cfg.ctx_limit_tokens(), true);
    }
    let stated = if cfg.ctx_limit == CtxLimit::Learned {
        model.and_then(|m| learned.get(m).copied())
    } else {
        None
    };
    let (mut limit, told) = match stated {
        Some(size) => (size, true),
        None => (table_window(model), false),
    };
    if used > limit {
        limit = WINDOW_MAX;
    }
    (limit.clamp(WINDOW_MIN, WINDOW_MAX), told)
}

fn table_window(model: Option<&str>) -> u64 {
    let Some(model) = model else {
        return DEFAULT_WINDOW;
    };
    for (prefix, size) in MODEL_WINDOWS {
        if model.starts_with(prefix) {
            return *size;
        }
    }
    DEFAULT_WINDOW
}

/// The chosen session's context, ready for the widget.
pub fn snapshot(
    cfg: &Config,
    learned: &BTreeMap<String, u64>,
    entry: &SessionEntry,
) -> Option<Snapshot> {
    let r = reading(&entry.path, Some(&entry.id), entry.modified)?;
    let (limit, stated) = limit_for(r.model.as_deref(), r.used_tokens, cfg, learned);
    let label = match &entry.name {
        Some(n) if !n.is_empty() => Some(n.clone()),
        _ => r.slug.clone(),
    };
    Some(Snapshot {
        context: Some(ContextInfo {
            used_tokens: Some(r.used_tokens),
            limit_tokens: Some(limit),
            used_pct: pct_of(r.used_tokens, limit),
            limit_stated: stated,
            model: r.model,
            label,
            session_id: entry.id.clone(),
            observed_at: r.observed_at,
            source: Source::Session,
        }),
        ..Snapshot::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn turn(tokens: u64, model: &str, sidechain: bool) -> String {
        format!(
            r#"{{"type":"assistant","sessionId":"s1","isSidechain":{sidechain},"slug":"brave-moth","cwd":"C:\\p","timestamp":"2026-09-05T16:30:45.926Z","message":{{"model":"{model}","usage":{{"input_tokens":2,"cache_creation_input_tokens":8,"cache_read_input_tokens":{}}}}}}}"#,
            tokens - 10
        )
    }

    fn back(lines: &[&str]) -> Vec<String> {
        lines.iter().rev().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_last_real_assistant_turn_is_the_reading() {
        let lines = back(&[
            &turn(1000, "claude-opus-5", false),
            r#"{"type":"user","message":{"role":"user"}}"#,
            &turn(2000, "claude-opus-5", false),
        ]);
        let r = parse_tail(&lines, Some("s1"), 0).unwrap();
        assert_eq!(r.used_tokens, 2000, "the newest turn, and the three input counters only");
        assert_eq!(r.slug.as_deref(), Some("brave-moth"));
        assert_eq!(r.observed_at, 1_788_625_845, "the line's own timestamp, not the file's");
    }

    #[test]
    fn a_synthetic_error_line_is_not_a_reading() {
        // An interrupted or failed request is written with this model and
        // all-zero counters. Taking it turns a busy session into a confident 0%.
        let zero = r#"{"type":"assistant","sessionId":"s1","isSidechain":false,"message":{"model":"<synthetic>","usage":{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}}}"#;
        let lines = back(&[&turn(2000, "claude-opus-5", false), zero]);
        assert_eq!(parse_tail(&lines, Some("s1"), 0).unwrap().used_tokens, 2000);
    }

    #[test]
    fn a_sidechain_line_belongs_to_a_subagent_not_to_the_session() {
        // A subagent's transcript carries the parent's sessionId, so the id
        // alone cannot tell them apart.
        let lines = back(&[
            &turn(2000, "claude-opus-5", false),
            &turn(90_000, "claude-opus-5", true),
        ]);
        assert_eq!(parse_tail(&lines, Some("s1"), 0).unwrap().used_tokens, 2000);
    }

    #[test]
    fn duplicate_lines_of_one_message_are_not_summed() {
        let one = turn(2000, "claude-opus-5", false);
        let lines = back(&[&one, &one]);
        assert_eq!(
            parse_tail(&lines, Some("s1"), 0).unwrap().used_tokens,
            2000,
            "one API message writes several lines and repeats its usage verbatim"
        );
    }

    #[test]
    fn a_compaction_needs_no_special_handling() {
        // The verified trace: the last turn before the boundary, the boundary
        // itself, and the first turn after it. The reset is already inside
        // cache_read_input_tokens, so nothing here knows what a compaction is.
        let boundary = r#"{"type":"system","subtype":"compact_boundary","sessionId":"s1","compactMetadata":{"trigger":"manual","preTokens":327351,"postTokens":17451}}"#;
        let lines = back(&[
            &turn(326_085, "claude-opus-5", false),
            boundary,
            &turn(79_331, "claude-opus-5", false),
        ]);
        assert_eq!(parse_tail(&lines, Some("s1"), 0).unwrap().used_tokens, 79_331);
    }

    #[test]
    fn only_the_named_session_answers_when_one_is_pinned() {
        let other = turn(5000, "claude-opus-5", false).replace("\"s1\"", "\"s2\"");
        let lines = back(&[&turn(2000, "claude-opus-5", false), &other]);
        assert_eq!(parse_tail(&lines, Some("s1"), 0).unwrap().used_tokens, 2000);
        assert_eq!(parse_tail(&lines, Some("s2"), 0).unwrap().used_tokens, 5000);
    }

    #[test]
    fn the_first_line_of_a_cut_tail_is_thrown_away() {
        // The fragment is valid JSON with a plausible reading of its own, which
        // is exactly the case a parser-only defence would miss.
        let whole = turn(2000, "claude-opus-5", false);
        let fragment = turn(999_999, "claude-opus-5", false);
        let bytes = format!("{fragment}\n{whole}\n").into_bytes();
        let lines = lines_back(&bytes, true);
        assert_eq!(parse_tail(&lines, Some("s1"), 0).unwrap().used_tokens, 2000);
        // Without the cut it is an ordinary line and does count.
        let lines = lines_back(&bytes, false);
        assert_eq!(parse_tail(&lines, Some("s1"), 0).unwrap().used_tokens, 2000);
    }

    #[test]
    fn a_tail_cut_inside_a_cyrillic_path_still_parses() {
        let whole = turn(2000, "claude-opus-5", false);
        let mut bytes = "{\"cwd\":\"C:\\\\Записи".as_bytes().to_vec();
        // Cut between the two bytes of a letter, which is where a byte offset
        // lands about half the time on these files.
        bytes.pop();
        bytes.push(b'\n');
        bytes.extend_from_slice(whole.as_bytes());
        bytes.push(b'\n');
        let lines = lines_back(&bytes, true);
        assert_eq!(
            parse_tail(&lines, Some("s1"), 0).unwrap().used_tokens,
            2000,
            "a lossy decode must not throw the whole tail away over one path"
        );
    }

    #[test]
    fn nothing_usable_means_silence() {
        assert!(parse_tail(&[], Some("s1"), 0).is_none());
        assert!(parse_tail(&["not json".into()], Some("s1"), 0).is_none());
        assert!(
            parse_tail(&[r#"{"type":"user","message":{}}"#.into()], Some("s1"), 0).is_none(),
            "a conversation with no answer in it yet reports nothing, not zero"
        );
    }

    fn cfg_with(limit: CtxLimit, fixed: u64) -> Config {
        let mut cfg = Config::default();
        cfg.ctx_limit = limit;
        cfg.ctx_limit_tokens = fixed;
        cfg
    }

    #[test]
    fn the_window_of_a_known_model_is_a_million() {
        let cfg = cfg_with(CtxLimit::Table, 200_000);
        let empty = BTreeMap::new();
        assert_eq!(
            limit_for(Some("claude-opus-5"), 88_958, &cfg, &empty),
            (1_000_000, false)
        );
        // Ids carry dated suffixes, so the table matches by prefix.
        assert_eq!(
            limit_for(Some("claude-sonnet-5-20260901"), 1, &cfg, &empty).0,
            1_000_000
        );
    }

    #[test]
    fn an_unknown_model_falls_back_to_the_older_size() {
        let cfg = cfg_with(CtxLimit::Table, 200_000);
        let empty = BTreeMap::new();
        assert_eq!(limit_for(Some("claude-sonnet-4-6"), 1, &cfg, &empty).0, 200_000);
        assert_eq!(limit_for(None, 1, &cfg, &empty).0, 200_000);
    }

    #[test]
    fn a_stated_size_beats_the_table() {
        let cfg = cfg_with(CtxLimit::Learned, 200_000);
        let mut learned = BTreeMap::new();
        learned.insert("claude-opus-5".to_string(), 200_000);
        assert_eq!(
            limit_for(Some("claude-opus-5"), 1, &cfg, &learned),
            (200_000, true),
            "what the status line named beats what the table guesses"
        );
    }

    #[test]
    fn a_reading_larger_than_the_window_raises_it() {
        let cfg = cfg_with(CtxLimit::Table, 200_000);
        let empty = BTreeMap::new();
        assert_eq!(
            limit_for(Some("claude-sonnet-4-6"), 400_000, &cfg, &empty).0,
            1_000_000,
            "a session that counted 400k tokens is not in a 200k window"
        );
    }

    #[test]
    fn a_fixed_size_is_not_overruled() {
        let cfg = cfg_with(CtxLimit::Fixed, 200_000);
        let mut learned = BTreeMap::new();
        learned.insert("claude-opus-5".to_string(), 1_000_000);
        assert_eq!(
            limit_for(Some("claude-opus-5"), 400_000, &cfg, &learned),
            (200_000, true),
            "the user's own answer stands, even against a reading that overflows it"
        );
    }

    #[test]
    fn the_size_is_clamped_the_way_claude_code_clamps_it() {
        let cfg = cfg_with(CtxLimit::Fixed, 99);
        assert_eq!(limit_for(None, 1, &cfg, &BTreeMap::new()).0, 8_192);
    }
}
