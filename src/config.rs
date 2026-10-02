//! User settings, stored as TOML next to nothing else the app needs.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The one place that never moves: where a config is found when nothing has
/// redirected it yet, and the only place `data_dir` itself is ever read from.
///
/// It sits beside the executable, never under `%APPDATA%`: a program started
/// from a packaged one — the Claude desktop app, a Claude Code session inside
/// it — sees a private copy of `%APPDATA%`, and twice that meant a widget
/// reading settings nobody had edited (A36). Worked out once per process.
fn default_base_dir() -> &'static Path {
   static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
   DIR.get_or_init(|| {
      let fallback = std::env::temp_dir().join("ccd-usage-monitor");
      let dir = std::env::current_exe()
         .map(|exe| default_base_dir_for(&exe))
         .unwrap_or_else(|_| fallback.clone());
      if std::fs::create_dir_all(&dir).is_ok() {
         return dir;
      }
      // Installed somewhere it may not write, such as Program Files: a
      // widget with settings in the temp folder beats one that will not start.
      eprintln!(
         "cannot create {}; keeping settings in {} instead",
         dir.display(),
         fallback.display()
      );
      let _ = std::fs::create_dir_all(&fallback);
      fallback
   })
}

/// `settings` beside the executable — except for a build inside this
/// repository's `target\<profile>\`, which uses the repository's own
/// `settings`. That is what lets the widget from `target\quick` and the
/// status-line bridge from `target\release` find the same files, and keeps
/// them out of reach of `cargo clean`.
fn default_base_dir_for(exe: &Path) -> PathBuf {
   let dir = exe.parent().unwrap_or(Path::new("."));
   let repo = dir
      .parent()
      .filter(|target| target.file_name().is_some_and(|n| n == "target"))
      .and_then(Path::parent)
      .filter(|root| {
         std::fs::read_to_string(root.join("Cargo.toml")).is_ok_and(|t| {
            t.lines()
               .any(|l| l.trim() == r#"name = "ccd-usage-monitor""#)
         })
      });
   repo.unwrap_or(dir).join("settings")
}

/// Where every file this program writes actually lives.
///
/// One hop: read `data_dir` out of whatever sits at the fixed location, and
/// if it names a folder that exists, use that instead. The folder found this
/// way is never itself asked for a `data_dir` of its own — a chain would let
/// two configs point at each other, and one hop is all the problem this
/// solves needs.
///
/// A folder that does not exist is ignored rather than made: a mistyped path
/// must not quietly become a second, empty set of settings — the same spirit
/// as A24's rule for a theme that does not compile.
pub fn base_dir() -> PathBuf {
   redirected(default_base_dir())
}

fn redirected(default: &Path) -> PathBuf {
   let text = std::fs::read_to_string(default.join("config.toml")).unwrap_or_default();
   let (cfg, _) = Config::parse(&text);
   let dir = PathBuf::from(cfg.data_dir.trim());
   if !dir.as_os_str().is_empty() && dir.is_dir() {
      dir
   } else {
      default.to_path_buf()
   }
}

pub fn config_path() -> PathBuf {
   base_dir().join("config.toml")
}

/// Copies every file this program owns into `new_dir`, then points the fixed
/// location at it — called once, the moment `data_dir` changes from empty to
/// set. Copies rather than moves, and never overwrites a file already at the
/// destination: a broken copy, or a folder shared between two installs, must
/// not cost the original. A folder that does not exist yet is refused, which
/// leaves the setting empty (`slint_ui::changed`).
pub fn relocate(new_dir: &Path) -> std::io::Result<()> {
   if !new_dir.is_dir() {
      return Err(std::io::ErrorKind::NotFound.into());
   }
   let from_dir = default_base_dir();
   for name in [
      "config.toml",
      "theme.slint",
      "state.json",
      "window.json",
      "api_poll.json",
      "samples.json",
   ] {
      let (from, to) = (from_dir.join(name), new_dir.join(name));
      if from.exists() && !to.exists() {
         let _ = std::fs::copy(&from, &to);
      }
   }
   // Everything else at the fixed location stops mattering from here on —
   // only `data_dir` is ever read from it (see `base_dir`) — but a stub
   // rather than a stale full copy is what makes that plain to a person who
   // opens it looking for the settings that used to be there.
   #[derive(Serialize)]
   struct Stub {
      data_dir: String,
   }
   let stub = toml::to_string(&Stub {
      data_dir: new_dir.to_string_lossy().into_owned(),
   })
   .unwrap_or_default();
   std::fs::write(from_dir.join("config.toml"), stub)
}

/// One of the four places the figures can come from. Each is switched on or
/// off on its own and carries its own staleness threshold, because they age at
/// very different rates: the endpoint answers in seconds, the desktop app
/// writes its history about every quarter of an hour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceKind {
   /// The file written by `ccd-usage-monitor --bridge`.
   Bridge,
   /// The usage endpoint.
   Api,
   /// The usage history the Claude desktop app keeps.
   Desktop,
   /// The conversation transcripts Claude Code writes for itself. The only
   /// source of a context figure that works outside a terminal session.
   Session,
}

impl SourceKind {
   pub const ALL: [SourceKind; 4] = [
      SourceKind::Bridge,
      SourceKind::Api,
      SourceKind::Desktop,
      SourceKind::Session,
   ];

   /// The tag used where a whole label will not fit.
   pub fn short(self) -> &'static str {
      match self {
         SourceKind::Bridge => "sl",
         SourceKind::Api => "API",
         SourceKind::Desktop => "app",
         SourceKind::Session => "tr",
      }
   }
}

/// The old single-choice setting, kept only long enough to read a config
/// written before the sources became independent switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceMode {
   Auto,
   Bridge,
   Api,
   Desktop,
   Demo,
}

/// When the detail panel under the pill is shown. Hover is cheap to trigger by
/// accident, so it is not the default: most of the time the pill is meant to be
/// glanced at, not interrogated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetailMode {
   /// While the pointer is over the pill.
   Hover,
   /// While the settings window is open.
   WithSettings,
   Never,
}

impl DetailMode {}

/// What the big percentages and the bars say: how much of a window is gone, or
/// how much of it is left. One reading either way — only the direction it is
/// read in changes, so the colour and the pace verdict stay on spending.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PctMode {
   /// `48%` — nearly half the window is gone.
   Spent,
   /// `52%` — nearly half the window is still there.
   Left,
}

impl PctMode {
   pub fn label(self, ru: bool) -> &'static str {
      match (self, ru) {
         (PctMode::Spent, true) => "потрачено",
         (PctMode::Spent, false) => "spent",
         (PctMode::Left, true) => "осталось",
         (PctMode::Left, false) => "left",
      }
   }

   /// The other of the two. Clicking the row toggles to it, and the detail
   /// panel, which says both numbers, needs the one that is not on the pill.
   pub fn other(self) -> PctMode {
      match self {
         PctMode::Spent => PctMode::Left,
         PctMode::Left => PctMode::Spent,
      }
   }

   /// The number shown for a reading. Spending can pass 100% on a spend limit,
   /// so what is left is floored at zero instead of going negative.
   pub fn shown(self, used_pct: f64) -> f64 {
      match self {
         PctMode::Spent => used_pct,
         PctMode::Left => (100.0 - used_pct).max(0.0),
      }
   }
}

/// The old two-way choice of unit, kept only long enough to read a config
/// written before every form of the figure went into one list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaceUnit {
   Ratio,
   Points,
}

/// What the pace figure looks like on the pill.
///
/// Both numbers say the same comparison — usage against the share of the window
/// already elapsed — and answer different questions with it: the points say how
/// far spending has run ahead, the ratio says how much to slow down. Which of
/// them, in which order, is a matter of taste, so all the forms are one list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaceView {
   /// No cell on the pill at all.
   Off,
   Ratio,
   Points,
   /// The gap in points converted to the window's own time: `0:27` on the
   /// five-hour window, `3д2ч` on the weekly one. Alone, never in a pair.
   Time,
   RatioPoints,
   PointsRatio,
   /// Both, with a drawn rule between them like the one between the groups.
   Split,
   /// Both run together with a written separator and no spaces around it:
   /// `9%|2.0x`. The separator keeps the grey of the clock cells while the
   /// numbers carry the pace colour, so the pair reads as one word.
   Piped,
}

impl PaceView {
   pub fn has_ratio(self) -> bool {
      !matches!(self, PaceView::Off | PaceView::Points | PaceView::Time)
   }

   pub fn has_points(self) -> bool {
      !matches!(self, PaceView::Off | PaceView::Ratio | PaceView::Time)
   }

   pub fn points_first(self) -> bool {
      matches!(
         self,
         PaceView::Points | PaceView::PointsRatio | PaceView::Split | PaceView::Piped
      )
   }

   /// What is written between the two numbers when the figure has to be one
   /// string. Only the piped form has a character of its own; the split form
   /// is drawn, not written, so in text it falls back to a space.
   pub fn joiner(self) -> &'static str {
      match self {
         PaceView::Piped => "|",
         _ => " ",
      }
   }

   /// The form that says both numbers, keeping this one's order. The detail
   /// panel uses it: the pill is where the choice is honoured, the panel is
   /// where everything is spelled out.
   pub fn both(self) -> PaceView {
      if self.points_first() {
         PaceView::PointsRatio
      } else {
         PaceView::RatioPoints
      }
   }
}

/// What the colour of a usage bar means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BarColour {
   /// Red when the window is nearly spent — the same rule as the figures.
   Fill,
   /// Red when spending has run ahead of the clock, whatever the fill. A full
   /// bar and a bar about to overflow are not the same thing.
   Pace,
}

impl BarColour {}

/// Which edge of the pill stays where it was put when the pill changes width
/// (A34). Its cells are as wide as their text, so a figure gaining a digit
/// widens the pill; this says which way it grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Anchor {
   /// The left edge holds; the pill grows and shrinks on its right.
   Left,
   /// The right edge holds; the pill grows leftwards — for a pill parked
   /// against the right of the screen.
   Right,
}

/// How the weekly window's days left are counted (A29).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaysBy {
   /// Whole local dates, with `count_today` and `count_reset_day` deciding
   /// the two ends of the run.
   Calendar,
   /// The exact time from now to the reset, nothing to decide.
   Time,
}

/// How a span of days and hours reads on the pill: the day count by time
/// (`days_form`) and the weekly pace gap in time form (`pace_form`) each have
/// their own choice of the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaysForm {
   /// `3д16ч` — says what it is, at the cost of two letters.
   Units,
   /// `3'16` — days and hours around `weekly_sep`, as short as it gets.
   /// Written `colon` before the separator could be chosen, and still read
   /// under that name.
   #[serde(alias = "colon")]
   Short,
}

/// Which session's context the widget reports.
///
/// Not a guess the widget can make well on its own: eighty-odd transcripts sit
/// under `~/.claude/projects` on a working machine, most of them finished
/// conversations that would each report a perfectly true figure about a session
/// nobody is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPick {
   /// The transcript written to most recently, wherever it is.
   Freshest,
   /// The freshest among those Claude Code lists as running. Deliberately no
   /// fallback: with nothing running the field goes quiet rather than
   /// reporting on a conversation nobody is in.
   Live,
   /// One chosen by hand; `session_id` names it.
   Pinned,
}

impl SessionPick {}

/// How the denominator of the context figure is decided.
///
/// The numerator is exact; the denominator is the only guess in the whole
/// feature, and getting it wrong is not a rounding error but five times the
/// answer. So it is a setting rather than a constant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CtxLimit {
   /// By the model's name, from the built-in table.
   Table,
   /// The size the status line stated for this model, when one has ever been
   /// stated; the table underneath.
   Learned,
   /// One number, set by hand. The only answer to a model held to a smaller
   /// window by an environment variable, which leaves no trace on disk.
   Fixed,
}

impl CtxLimit {}

/// What the `ctx` group shows.
///
/// The percentage depends on a denominator that may be a guess; the token count
/// does not. So the count is offered on its own, and beside the percentage, as
/// well as not at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CtxView {
   Off,
   Pct,
   Tokens,
   TokensPct,
   PctTokens,
}

impl CtxView {
   pub fn has_pct(self) -> bool {
      !matches!(self, CtxView::Off | CtxView::Tokens)
   }

   pub fn has_tokens(self) -> bool {
      !matches!(self, CtxView::Off | CtxView::Pct)
   }

   /// Whether the count comes first, so the two cells are built in order.
   pub fn tokens_first(self) -> bool {
      matches!(self, CtxView::Tokens | CtxView::TokensPct)
   }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
   /// Background opacity of the pill, 0.3..1.0.
   pub opacity: f32,
   /// Multiplies every size below, and the paddings with them. The one knob
   /// worth reaching for when the widget moves to another monitor.
   pub scale: f32,
   /// Which edge of the pill stays put as its width changes.
   pub anchor: Anchor,
   /// How the days the weekly allowance is spread over are counted: whole
   /// calendar days, or the exact time to the reset (A29).
   pub days_by: DaysBy,
   /// How the count reads by time: `3д16ч` or `3'16`. Ignored by calendar,
   /// where the count is whole days and has nothing to shorten.
   pub days_form: DaysForm,
   /// What stands between days and hours in the short form of both weekly
   /// figures, the count and the pace gap. Read through `weekly_sep()`.
   pub weekly_sep: String,
   /// Whether the hours in the short form are always two digits, `2'07`, or
   /// as few as they need, `2'7`.
   pub weekly_pad: bool,
   /// Which calendar days count, when `days_by` is the calendar; ignored by
   /// time. Today is usually half spent already; the reset day is worth only
   /// the hours before the reset, which with a morning reset is next to nothing.
   pub count_today: bool,
   pub count_reset_day: bool,
   /// Whether the percentages and the bars count what is spent or what
   /// is left of a window.
   pub pct_mode: PctMode,
   /// The form of the pace figure on the pill, `Off` for none.
   pub pace_view: PaceView,
   /// How the weekly gap reads in the time form: `3д2ч` or `3'2`. The
   /// five-hour gap is `h:mm` either way.
   pub pace_form: DaysForm,
   /// Which cells the pill carries for each window, besides its percentage.
   /// The pill only: the detail panel and the status line keep every figure.
   pub show_5h_pace: bool,
   pub show_5h_left: bool,
   pub show_5h_clock: bool,
   pub show_week_pace: bool,
   pub show_week_days: bool,
   pub show_week_allowance: bool,
   /// Whether a bar is coloured by how full it is or by how fast it filled.
   pub bar_colour: BarColour,
   /// The observed rate and what it means for the reset, in the detail panel.
   pub pace_rate: bool,
   /// When the detail panel under the pill appears.
   pub detail_mode: DetailMode,
   /// Show one line per source in the detail panel, not just the merged one.
   pub detail_all_sources: bool,
   /// `en` or `ru` — affects weekday names and tooltip wording only.
   pub lang: String,
   /// Let clicks fall through to whatever is underneath.
   pub click_through: bool,
   /// Which sources may contribute. All three at once is the normal setting;
   /// turning one off is how "where is this number from" gets answered.
   pub bridge_enabled: bool,
   pub api_enabled: bool,
   pub desktop_enabled: bool,
   pub session_enabled: bool,
   /// Which session the context figure is about, and which one when it is
   /// pinned by hand.
   pub session_pick: SessionPick,
   pub session_id: String,
   /// How often the whole transcript tree is enumerated again. Between scans
   /// the widget stats one file, so this is nearly free either way; it is a
   /// setting because "the list is a minute out of date" is a real complaint
   /// on a machine where sessions are started often.
   pub session_scan_secs: i64,
   /// What the `ctx` group shows, and what its percentage is divided by.
   pub ctx_view: CtxView,
   pub ctx_limit: CtxLimit,
   pub ctx_limit_tokens: u64,
   /// Whether the `ctx` percentage and token count follow one direction or
   /// two. `pct_mode` is tuned for quotas that drain; a context window fills
   /// instead, so `ctx` keeps its own direction rather than borrowing that
   /// one, and can split it in two if the two figures want to disagree.
   pub ctx_dir_linked: bool,
   pub ctx_pct_dir: PctMode,
   pub ctx_tokens_dir: PctMode,
   /// Synthetic figures, for working on the layout.
   pub demo: bool,
   /// How long a reading from each source stays current. The desktop history
   /// is allowed twice as long: it is written every quarter of an hour by
   /// design, and calling that stale is blaming a source for how it works.
   pub bridge_stale_secs: i64,
   pub api_stale_secs: i64,
   pub desktop_stale_secs: i64,
   pub session_stale_secs: i64,
   /// Seconds between usage-endpoint polls.
   pub api_interval_secs: u64,
   /// Requests per rolling minute the widget allows itself. Past this the
   /// poller waits instead of asking; see `api::spawn_poller`.
   pub api_budget_per_min: u32,
   /// Whether an expired token is handed back to Claude Code to renew, by
   /// running `claude -p /usage` out of sight (A35).
   pub api_revive_token: bool,
   /// Read from a config written before the per-source switches; migrated on
   /// load and never written back.
   #[serde(rename = "source", skip_serializing_if = "Option::is_none")]
   pub legacy_source: Option<SourceMode>,
   /// The same, for the two keys that preceded `pace_view`.
   #[serde(rename = "pace_cell", skip_serializing_if = "Option::is_none")]
   pub legacy_pace_cell: Option<bool>,
   #[serde(rename = "pace_unit", skip_serializing_if = "Option::is_none")]
   pub legacy_pace_unit: Option<PaceUnit>,
   /// The three window rectangles, read from a config written before they
   /// moved to `window.json` (see [`crate::geometry`]). Migrated on load and
   /// never written back — they are why the file could not be hand-edited,
   /// since a drag rewrote it and took any comment with it.
   #[serde(rename = "position", skip_serializing_if = "Option::is_none")]
   pub legacy_position: Option<[f32; 2]>,
   #[serde(rename = "settings_pos", skip_serializing_if = "Option::is_none")]
   pub legacy_settings_pos: Option<[f32; 2]>,
   #[serde(rename = "settings_size", skip_serializing_if = "Option::is_none")]
   pub legacy_settings_size: Option<[f32; 2]>,
   /// Percentages at which a figure turns amber, then red.
   pub warn_pct: f64,
   pub danger_pct: f64,
   /// How far usage may drift from the even-rate target before it counts as
   /// running ahead or behind, in percentage points.
   pub pace_tolerance: f64,
   /// After this long a figure is not shown at all. A five-hour window read an
   /// hour ago says nothing useful about the present, and a confident wrong
   /// number is worse than `--%`.
   pub drop_after_secs: i64,
   /// Where every file this program writes actually lives, once set — see
   /// `base_dir`. Empty is the default location, `settings` beside the
   /// executable (A36), and so is a folder that does not exist. Read only
   /// from the copy at the fixed location (`default_base_dir`) — a value saved anywhere else
   /// is stored but never consulted, which is what makes moving to a second
   /// folder later a manual step: edit it there, not in the settings window.
   pub data_dir: String,
}

impl Default for Config {
   fn default() -> Self {
      Config {
         opacity: 0.82,
         scale: 1.0,
         anchor: Anchor::Left,
         days_by: DaysBy::Calendar,
         days_form: DaysForm::Units,
         weekly_sep: "'".to_string(),
         weekly_pad: false,
         count_today: false,
         count_reset_day: true,
         pct_mode: PctMode::Spent,
         pace_view: PaceView::Ratio,
         pace_form: DaysForm::Units,
         show_5h_pace: true,
         show_5h_left: true,
         show_5h_clock: true,
         show_week_pace: true,
         show_week_days: true,
         show_week_allowance: true,
         bar_colour: BarColour::Fill,
         pace_rate: true,
         detail_mode: DetailMode::WithSettings,
         detail_all_sources: false,
         lang: "ru".to_string(),
         click_through: false,
         bridge_enabled: true,
         api_enabled: true,
         desktop_enabled: true,
         session_enabled: true,
         session_pick: SessionPick::Live,
         session_id: String::new(),
         session_scan_secs: 30,
         ctx_view: CtxView::Pct,
         ctx_limit: CtxLimit::Learned,
         ctx_limit_tokens: 200_000,
         ctx_dir_linked: true,
         ctx_pct_dir: PctMode::Spent,
         ctx_tokens_dir: PctMode::Spent,
         demo: false,
         bridge_stale_secs: 900,
         api_stale_secs: 900,
         desktop_stale_secs: 1800,
         session_stale_secs: 900,
         api_interval_secs: 180,
         api_budget_per_min: 4,
         api_revive_token: true,
         legacy_source: None,
         legacy_pace_cell: None,
         legacy_pace_unit: None,
         legacy_position: None,
         legacy_settings_pos: None,
         legacy_settings_size: None,
         warn_pct: 60.0,
         danger_pct: 85.0,
         pace_tolerance: 7.0,
         drop_after_secs: 3600,
         data_dir: String::new(),
      }
   }
}

impl Config {
   pub fn load() -> Config {
      let text = std::fs::read_to_string(config_path()).unwrap_or_default();
      let (mut cfg, sectioned) = Config::parse(&text);
      // Both run, and neither short-circuits the other: `|` rather than
      // `||`, because a config needing one migration usually needs both and
      // skipping the second would leave the keys behind for another release.
      let migrated = cfg.migrate() | cfg.migrate_geometry();
      // A flat file is laid out into sections once, and only if there is a
      // file: a first run should not write settings nobody has chosen yet.
      let flat_on_disk = !sectioned && !text.trim().is_empty();
      if migrated || flat_on_disk {
         cfg.save();
      }
      cfg
   }

   /// Translate a config written before the sources became independent
   /// switches. Returns whether anything changed, so the file is rewritten
   /// once and the old key disappears rather than lingering as a lie.
   /// Move the window rectangles out of the config and into `window.json`,
   /// once. Returns whether the config needs rewriting without them.
   ///
   /// Separate from `migrate` because it has a side effect on another file
   /// and `migrate` is a pure rearrangement of this struct — and because it
   /// must not overwrite a `window.json` that already exists: a config still
   /// carrying the old keys is not evidence that it is the newer of the two.
   fn migrate_geometry(&mut self) -> bool {
      let old = (
         self.legacy_position.take(),
         self.legacy_settings_pos.take(),
         self.legacy_settings_size.take(),
      );
      if old.0.is_none() && old.1.is_none() && old.2.is_none() {
         return false;
      }
      let existing = crate::geometry::load();
      if existing == crate::geometry::Geometry::default() {
         crate::geometry::save(&crate::geometry::Geometry {
            position: old.0,
            settings_pos: old.1,
            settings_size: old.2,
            ..Default::default()
         });
      }
      // Either way the keys leave the config: they are the reason it could
      // not be hand-edited, and a second copy of a window position is a
      // second answer to a question with one.
      true
   }

   fn migrate(&mut self) -> bool {
      let mut changed = false;

      // `pace_cell` + `pace_unit` became one list of forms. A config that
      // chose points must come back showing points, not the default.
      let cell = self.legacy_pace_cell.take();
      let unit = self.legacy_pace_unit.take();
      if cell.is_some() || unit.is_some() {
         self.pace_view = match (cell.unwrap_or(true), unit.unwrap_or(PaceUnit::Ratio)) {
            (false, _) => PaceView::Off,
            (true, PaceUnit::Points) => PaceView::Points,
            (true, PaceUnit::Ratio) => PaceView::Ratio,
         };
         changed = true;
      }

      let Some(mode) = self.legacy_source.take() else {
         return changed;
      };
      let (bridge, api, desktop, demo) = match mode {
         SourceMode::Auto => (true, true, true, false),
         SourceMode::Bridge => (true, false, false, false),
         SourceMode::Api => (false, true, false, false),
         SourceMode::Desktop => (false, false, true, false),
         // Demo was a mode of its own; leave the sources as they were.
         SourceMode::Demo => (
            self.bridge_enabled,
            self.api_enabled,
            self.desktop_enabled,
            true,
         ),
      };
      self.bridge_enabled = bridge;
      self.api_enabled = api;
      self.desktop_enabled = desktop;
      self.demo = demo;
      true
   }

   pub fn enabled(&self, kind: SourceKind) -> bool {
      match kind {
         SourceKind::Bridge => self.bridge_enabled,
         SourceKind::Api => self.api_enabled,
         SourceKind::Desktop => self.desktop_enabled,
         SourceKind::Session => self.session_enabled,
      }
   }

   pub fn stale_secs(&self, kind: SourceKind) -> i64 {
      let raw = match kind {
         SourceKind::Bridge => self.bridge_stale_secs,
         SourceKind::Api => self.api_stale_secs,
         SourceKind::Desktop => self.desktop_stale_secs,
         SourceKind::Session => self.session_stale_secs,
      };
      raw.clamp(30, 24 * 3600)
   }

   pub fn session_scan_secs(&self) -> u64 {
      self.session_scan_secs.clamp(5, 600) as u64
   }

   /// Clamped to the range Claude Code clamps its own fetched size to, so a
   /// hand-edited file cannot make the percentage meaningless.
   pub fn ctx_limit_tokens(&self) -> u64 {
      self.ctx_limit_tokens.clamp(8_192, 1_000_000)
   }

   /// The direction the `ctx` token count reads in. Mirrors `ctx_pct_dir`
   /// while the two are linked; the field only takes over once they are
   /// split.
   pub fn ctx_tokens_dir(&self) -> PctMode {
      if self.ctx_dir_linked {
         self.ctx_pct_dir
      } else {
         self.ctx_tokens_dir
      }
   }

   pub fn api_interval(&self) -> u64 {
      self.api_interval_secs.clamp(30, 3600)
   }

   pub fn api_budget(&self) -> u32 {
      self.api_budget_per_min.clamp(1, 30)
   }

   /// Write the config, keeping whatever the person who owns the file put in
   /// it — comments, key order, blank lines.
   ///
   /// Written whole and swapped in, like every other file this program owns:
   /// a `--bridge` process starting mid-write would otherwise read half a
   /// document and fall back to defaults.
   pub fn save(&self) {
      let path = config_path();
      if let Some(dir) = path.parent() {
         let _ = std::fs::create_dir_all(dir);
      }
      let existing = std::fs::read_to_string(&path).unwrap_or_default();
      let text = self.render(&existing);

      let Some(dir) = path.parent() else { return };
      let tmp = dir.join(format!("config.{}.tmp", std::process::id()));
      let written = std::fs::File::create(&tmp).and_then(|mut f| {
         use std::io::Write;
         f.write_all(text.as_bytes())?;
         f.sync_all()
      });
      if written.is_ok() {
         let _ = std::fs::rename(&tmp, &path);
         // Remembered so the watcher can tell this write from a person's
         // (see `LAST_WRITTEN`).
         if let Ok(mut last) = LAST_WRITTEN.lock() {
            *last = Some(text);
         }
      } else {
         let _ = std::fs::remove_file(&tmp);
      }
   }

   /// Lay this config over the text of the existing file.
   ///
   /// Not `toml::to_string`: that produces a correct file and throws away
   /// everything a person wrote into the old one. The whole point of moving
   /// the window rectangles to `window.json` was to make the config worth
   /// hand-editing, and a hand-edited file that loses its comments on the
   /// next click in the menu is not.
   ///
   /// Each key goes into the table the schema names for it, so the file reads
   /// in groups rather than in the order the struct happens to declare its
   /// fields. The key itself never changes — moving it into a table is a
   /// change of position, not of name, so no value can be mistranslated.
   fn render(&self, existing: &str) -> String {
      use toml_edit::{DocumentMut, Item, Table, Value};

      /// Replace a value but keep its decoration: the comment before it and
      /// the one trailing it on the same line belong to whoever wrote them.
      /// An `f32` widened to `f64` prints as `1.0999999046325684`. The
      /// shortest form that still round-trips the `f32` is `1.0999999`, and
      /// this file is meant to be read.
      fn tidy(key: &str, value: &Value) -> Value {
         let is_f32 = crate::schema::get(key)
            .is_some_and(|s| matches!(s.kind, crate::schema::Kind::Float { .. }));
         match (is_f32, value.as_float()) {
            (true, Some(f)) => match (f as f32).to_string().parse::<f64>() {
               Ok(short) => Value::from(short),
               Err(_) => value.clone(),
            },
            _ => value.clone(),
         }
      }

      fn put(item: &mut Item, new: Value, carried: Option<&toml_edit::Decor>) {
         let decor = item
            .as_value()
            .map(|v| v.decor().clone())
            // Nothing there yet: this key is arriving, and if it arrived
            // from the top level of the same file it brings its comment.
            .or_else(|| carried.cloned());
         let mut new = new;
         if let Some(d) = decor {
            *new.decor_mut() = d;
         }
         *item = Item::Value(new);
      }

      let mut doc: DocumentMut = existing.parse().unwrap_or_default();
      let Ok(flat) = toml_edit::ser::to_document(self) else {
         return existing.to_string();
      };

      // What the keys about to move say for themselves. A comment belongs
      // to the line a person wrote it on, and laying a flat file out into
      // sections moves every one of those lines; without this the one-time
      // migration is also the one moment most likely to throw a comment away.
      // Two decorations per key, and they hold different halves of what a
      // person wrote: the comment trailing a value belongs to the value, the
      // comment standing above the line belongs to the key.
      let carried: std::collections::BTreeMap<String, toml_edit::Decor> = doc
         .iter()
         .filter_map(|(key, item)| {
            let value = item.as_value()?;
            crate::schema::section_of(key)?;
            Some((key.to_string(), value.decor().clone()))
         })
         .collect();
      let carried_keys: std::collections::BTreeMap<String, toml_edit::Decor> = doc
         .iter()
         .filter(|(key, item)| item.is_value() && crate::schema::section_of(key).is_some())
         .filter_map(|(key, _)| {
            let decor = doc.key(key)?.leaf_decor().clone();
            Some((key.to_string(), decor))
         })
         .collect();

      // Top-level keys first and tables after, which is the order TOML
      // requires. Nothing lands at the top level today — every live key has
      // a section — but a field added without a schema entry would, and it
      // should produce a readable file rather than an unparseable one.
      for (key, item) in flat.iter() {
         let Some(value) = item.as_value() else {
            continue;
         };
         if crate::schema::section_of(key).is_none() {
            put(&mut doc[key], tidy(key, value), None);
         }
      }
      for section in crate::schema::Section::ALL {
         let name = section.key();
         let mut any = false;
         for (key, item) in flat.iter() {
            let Some(value) = item.as_value() else {
               continue;
            };
            if crate::schema::section_of(key) != Some(section) {
               continue;
            }
            if !any {
               // `doc[name]` panics when the key is absent, so the table is
               // asked for rather than indexed into.
               if !doc.get(name).is_some_and(|i| i.is_table()) {
                  doc.insert(name, Item::Table(Table::new()));
               }
               any = true;
            }
            let arriving = doc[name].get(key).is_none();
            put(&mut doc[name][key], tidy(key, value), carried.get(key));
            if arriving
               && let Some(decor) = carried_keys.get(key)
               && let Some(table) = doc.get_mut(name).and_then(|i| i.as_table_mut())
               && let Some(mut k) = table.key_mut(key)
            {
               *k.leaf_decor_mut() = decor.clone();
            }
         }
      }

      // Anything we no longer write leaves the file: a key kept only to be
      // read once, like the window rectangles or the pre-switch `source`,
      // would otherwise sit there forever contradicting the file it is in.
      let live: std::collections::BTreeSet<&str> = flat.iter().map(|(k, _)| k).collect();
      let sections: std::collections::BTreeSet<&str> = crate::schema::Section::ALL
         .iter()
         .map(|s| s.key())
         .collect();
      doc.retain(|key, item| {
         // Our own tables, and anything else that is not a bare value —
         // an unknown table belongs to whoever put it there.
         if sections.contains(key) || !item.is_value() {
            return true;
         }
         // A value at the top level survives only if it is one we still
         // write *and* the schema does not give it a section. Without the
         // second half, migrating a flat file leaves every key in it twice:
         // once where it used to be and once where it now belongs, with the
         // stale copy the one a person reads first.
         live.contains(key) && crate::schema::section_of(key).is_none()
      });
      // And inside a section, only what is still a setting. This is what
      // retires a key: the nine sizes that moved into the theme with the
      // move to Slint (A24) are no longer written by `Config`, so the next
      // save takes them out of `[style]` rather than leaving nine lines
      // nobody can make do anything.
      for section in crate::schema::Section::ALL {
         if let Some(table) = doc.get_mut(section.key()).and_then(|i| i.as_table_mut()) {
            table.retain(|key, _| live.contains(key));
         }
      }
      doc.to_string()
   }

   /// Read a config in either shape.
   ///
   /// Returns whether the file was already in sections, which is how `load`
   /// knows to rewrite a flat one. A section is hoisted rather than mapped:
   /// the keys inside it are spelled exactly as `Config` spells them, so
   /// flattening the document and handing it to serde needs no table of
   /// names that could fall out of date.
   fn parse(text: &str) -> (Config, bool) {
      let Ok(value) = toml::from_str::<toml::Value>(text) else {
         return (Config::default(), false);
      };
      let Some(table) = value.as_table() else {
         return (Config::default(), false);
      };
      let mut flat = toml::map::Map::new();
      let mut sectioned = false;
      for (key, item) in table {
         let is_section =
            crate::schema::Section::ALL.iter().any(|s| s.key() == key) && item.is_table();
         match (is_section, item.as_table()) {
            (true, Some(inner)) => {
               sectioned = true;
               for (k, v) in inner {
                  flat.insert(k.clone(), v.clone());
               }
            }
            _ => {
               flat.insert(key.clone(), item.clone());
            }
         }
      }
      let cfg = toml::Value::Table(flat)
         .try_into::<Config>()
         .unwrap_or_default();
      (cfg, sectioned)
   }

   /// The separator of the short weekly form, as it is drawn. Empty falls
   /// back to `'`, since `316` says nothing; a space is kept, `3 16` being a
   /// fair choice. Cut to three characters so a stray paste cannot widen the
   /// pill without bound.
   pub fn weekly_sep(&self) -> &str {
      if self.weekly_sep.is_empty() {
         return "'";
      }
      match self.weekly_sep.char_indices().nth(3) {
         Some((at, _)) => &self.weekly_sep[..at],
         None => &self.weekly_sep,
      }
   }

   /// How the day count by time is written.
   pub fn days_span(&self) -> crate::pace::Span<'_> {
      self.span(self.days_form)
   }

   /// How the weekly pace gap in time form is written.
   pub fn pace_span(&self) -> crate::pace::Span<'_> {
      self.span(self.pace_form)
   }

   fn span(&self, form: DaysForm) -> crate::pace::Span<'_> {
      match form {
         DaysForm::Units => crate::pace::Span::Units,
         DaysForm::Short => crate::pace::Span::Short {
            sep: self.weekly_sep(),
            pad: self.weekly_pad,
         },
      }
   }

   pub fn day_count(&self) -> crate::pace::DayCount {
      match self.days_by {
         DaysBy::Time => crate::pace::DayCount::Time,
         DaysBy::Calendar => crate::pace::DayCount::Calendar {
            today: self.count_today,
            reset_day: self.count_reset_day,
         },
      }
   }

   pub fn is_ru(&self) -> bool {
      self.lang.eq_ignore_ascii_case("ru")
   }

   /// Sizes are clamped where they are read rather than where they are
   /// written, so a hand-edited config.toml cannot produce a pill that is
   /// invisible or larger than the screen.
   pub fn scale(&self) -> f32 {
      clamp(self.scale, 0.6, 3.0, 1.0)
   }
}

/// The exact text this process last wrote to `config.toml`.
///
/// The widget writes the file from some forty places — every switch in the
/// settings window, every tray toggle — and `Watch` must not report those back
/// as somebody's edit. Comparing the text rather than remembering an mtime is
/// what makes that reliable: a rename is not guaranteed to leave a timestamp
/// this process can predict, and two writes inside one filesystem tick would
/// share one anyway.
static LAST_WRITTEN: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Notices when `config.toml` was edited by somebody who is not this program.
///
/// The file is meant to be hand-edited — that was the point of moving the
/// window rectangles out of it — and an editor whose result only appears after
/// a restart is an editor people stop using. So the widget watches the file on
/// the same quarter-second tick that already watches the state file, and
/// re-reads it when it moves.
pub struct Watch {
   path: PathBuf,
   last_check: std::time::Instant,
   mtime: Option<std::time::SystemTime>,
}

impl Watch {
   /// Starts from the file as it is now, so the first poll reports nothing.
   pub fn new() -> Watch {
      Watch::at(config_path())
   }

   fn at(path: PathBuf) -> Watch {
      Watch {
         mtime: mtime_of(&path),
         path,
         last_check: std::time::Instant::now(),
      }
   }

   /// The config as it now stands on disk, if somebody else changed it.
   ///
   /// `None` covers three cases that are all "nothing to do": the tick is not
   /// due, the file has not moved, and the file has moved but holds exactly
   /// what this program put there.
   pub fn poll(&mut self) -> Option<Config> {
      if self.last_check.elapsed() < std::time::Duration::from_millis(250) {
         return None;
      }
      self.last_check = std::time::Instant::now();
      if !self.changed() {
         return None;
      }
      // Through `load` rather than `parse`, so a file edited into an older
      // shape is migrated exactly as it would be at startup. That may write
      // the file again; the next poll sees its own text and stops there.
      Some(Config::load())
   }

   /// The file moved, and what is in it is not what we put there.
   ///
   /// Kept apart from `poll` so the question can be asked about a file of a
   /// test's own choosing, without a real config on the real path.
   fn changed(&mut self) -> bool {
      let mtime = mtime_of(&self.path);
      if mtime == self.mtime {
         return false;
      }
      self.mtime = mtime;
      let text = std::fs::read_to_string(&self.path).unwrap_or_default();
      !LAST_WRITTEN
         .lock()
         .is_ok_and(|w| w.as_deref() == Some(text.as_str()))
   }
}

impl Default for Watch {
   fn default() -> Watch {
      Watch::new()
   }
}

fn mtime_of(path: &std::path::Path) -> Option<std::time::SystemTime> {
   std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// A non-finite value in the file means the number is not usable at all, so it
/// falls back to the default instead of the nearest bound.
fn clamp(v: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
   if v.is_finite() {
      v.clamp(lo, hi)
   } else {
      fallback
   }
}

#[cfg(test)]
mod tests {
   use super::*;

   /// A scratch folder of its own per test, so tests running side by side
   /// never see each other's files.
   fn scratch_dir(name: &str) -> PathBuf {
      let dir = std::env::temp_dir().join(format!("ccd-usage-monitor-test-{name}"));
      let _ = std::fs::remove_dir_all(&dir);
      std::fs::create_dir_all(&dir).unwrap();
      dir
   }

   #[test]
   fn a_build_in_this_repository_uses_its_settings_folder() {
      let root = scratch_dir("repo");
      std::fs::write(
         root.join("Cargo.toml"),
         "[package]\nname = \"ccd-usage-monitor\"\n",
      )
      .unwrap();
      let exe = root
         .join("target")
         .join("quick")
         .join("ccd-usage-monitor.exe");
      assert_eq!(default_base_dir_for(&exe), root.join("settings"));
   }

   #[test]
   fn anywhere_else_the_settings_sit_beside_the_exe() {
      let other = scratch_dir("other-repo");
      std::fs::write(other.join("Cargo.toml"), "[package]\nname = \"other\"\n").unwrap();
      let in_other = other.join("target").join("release").join("x.exe");
      assert_eq!(
         default_base_dir_for(&in_other),
         other.join("target").join("release").join("settings")
      );
      let plain = scratch_dir("plain")
         .join("tools")
         .join("ccd-usage-monitor.exe");
      assert_eq!(
         default_base_dir_for(&plain),
         plain.parent().unwrap().join("settings")
      );
   }

   #[test]
   fn data_dir_is_followed_only_to_a_folder_that_exists() {
      let base = scratch_dir("hop");
      let elsewhere = scratch_dir("hop-target");
      let point = |dir: &Path| {
         let line = toml::to_string(&toml::Table::from_iter([(
            "data_dir".to_string(),
            toml::Value::String(dir.to_string_lossy().into_owned()),
         )]))
         .unwrap();
         std::fs::write(base.join("config.toml"), line).unwrap();
      };
      assert_eq!(redirected(&base), base, "no config: the default");
      point(&elsewhere);
      assert_eq!(redirected(&base), elsewhere);
      let missing = elsewhere.join("not-made");
      point(&missing);
      assert_eq!(redirected(&base), base, "a missing folder is ignored");
      assert!(!missing.exists(), "and not created");
   }

   /// A config with every described setting moved off its default, built from
   /// the schema so that a field added later is covered without anyone
   /// remembering to come back here.
   fn wound_up() -> Config {
      use crate::schema::{self, Kind};
      let value = toml::Value::try_from(Config::default()).unwrap();
      let mut table = value.as_table().unwrap().clone();
      for s in schema::ALL {
         let new = match s.kind {
            Kind::Bool => toml::Value::Boolean(!table[s.key].as_bool().expect("a bool")),
            // The far end of the stated range: in bounds by construction,
            // and never the default.
            Kind::Int { max, .. } => toml::Value::Integer(max),
            Kind::Float { max, .. } => toml::Value::Float(max as f64),
            // The last option, for the same reason.
            Kind::Choice(opts) => toml::Value::String(opts[opts.len() - 1].value.to_string()),
            Kind::Text => toml::Value::String("a-pinned-conversation".into()),
         };
         table.insert(s.key.to_string(), new);
      }
      toml::Value::Table(table).try_into().expect("a config")
   }

   #[test]
   fn nothing_is_lost_on_the_way_into_sections_and_back() {
      // The hole this closes is the expensive one: a setting that quietly
      // stops persisting. The file is the only place it would show, and it
      // would show as "the widget forgot what I chose", weeks later.
      let cfg = wound_up();
      assert_ne!(
         cfg,
         Config::default(),
         "the fixture has to differ to prove anything"
      );

      let text = cfg.render("");
      let (back, sectioned) = Config::parse(&text);
      assert!(sectioned, "a file written now is in sections:\n{text}");
      assert_eq!(cfg, back, "written as:\n{text}");
   }

   #[test]
   fn every_key_is_written_under_the_table_the_schema_names() {
      let text = wound_up().render("");
      let doc: toml::Value = toml::from_str(&text).expect("valid TOML:\n{text}");
      let table = doc.as_table().unwrap();

      for s in crate::schema::ALL {
         let section = table
            .get(s.section.key())
            .unwrap_or_else(|| panic!("no [{}] table in:\n{text}", s.section.key()))
            .as_table()
            .unwrap();
         assert!(
            section.contains_key(s.key),
            "{} is not under [{}]:\n{text}",
            s.key,
            s.section.key()
         );
      }
      // And nothing loose at the top, which is what makes the file readable.
      assert!(
         table.values().all(|v| v.is_table()),
         "a key outside every section:\n{text}"
      );
   }

   #[test]
   fn migrating_a_flat_file_moves_its_keys_rather_than_copying_them() {
      // The case the two tests above miss: they render over an empty
      // document, where there is nothing to leave behind. Over a real flat
      // file the first attempt wrote every key twice — once where it used to
      // be and once where it now belongs — and the stale copy is the one a
      // person reads first.
      let cfg = wound_up();
      let flat = toml::to_string_pretty(&cfg).unwrap();
      let text = cfg.render(&flat);

      let doc: toml::Value = toml::from_str(&text).expect("valid TOML");
      let table = doc.as_table().unwrap();
      let loose: Vec<&String> = table
         .iter()
         .filter(|(_, v)| !v.is_table())
         .map(|(k, _)| k)
         .collect();
      assert!(loose.is_empty(), "left at the top level: {loose:?}\n{text}");
      assert_eq!(Config::parse(&text).0, cfg);
   }

   #[test]
   fn a_comment_travels_with_the_key_it_was_written_against() {
      // Both halves, because they live in different places: the comment
      // above a line belongs to the key, the one trailing the value belongs
      // to the value. Carrying only the second is how the first attempt
      // moved `# deliberate` into the section and left `# why` behind.
      let existing = "\
# why the endpoint is polled so rarely
api_interval_secs = 600  # deliberate
pct_mode = \"left\"
";
      let text = Config {
         api_interval_secs: 600,
         ..Config::default()
      }
      .render(existing);

      let sources = text.split("[sources]").nth(1).unwrap_or("");
      let upto_next = sources.split("\n[").next().unwrap_or("");
      assert!(
         upto_next.contains("# why the endpoint is polled so rarely"),
         "the leading comment did not follow its key:\n{text}"
      );
      assert!(
         upto_next.contains("# deliberate"),
         "the trailing comment did not follow its key:\n{text}"
      );
   }

   #[test]
   fn a_size_keeps_the_shape_a_person_wrote_it_in() {
      // `scale` is an f32; serialised through an f64 it comes out as
      // 1.0999999046325684, which is the same number and an unreadable file.
      let cfg = Config {
         scale: 1.1,
         ..Config::default()
      };
      let text = cfg.render("");
      assert!(text.contains("scale = 1.1"), "{text}");
      assert_eq!(Config::parse(&text).0.scale, 1.1);
   }

   #[test]
   fn a_flat_file_reads_the_same_as_a_sectioned_one() {
      // Upgrading must not change a single setting: whatever the old file
      // said, the widget behaves the same the moment after it is rewritten.
      let cfg = wound_up();
      let flat = toml::to_string_pretty(&cfg).unwrap();
      let sectioned = cfg.render("");

      let (from_flat, was_sectioned) = Config::parse(&flat);
      assert!(!was_sectioned, "a flat file has to be recognised as flat");
      let (from_sections, _) = Config::parse(&sectioned);
      assert_eq!(from_flat, from_sections);
      assert_eq!(from_flat, cfg);
   }

   #[test]
   fn what_a_person_wrote_into_the_file_survives_being_saved_over() {
      // The reason for `toml_edit`. A comment erased by the next click in
      // the settings menu is a file nobody will write a comment in twice.
      let existing = "\
# why the endpoint is polled so rarely here
[sources]
api_interval_secs = 600  # deliberate
";
      let cfg = Config {
         api_interval_secs: 900,
         ..Config::default()
      };
      let text = cfg.render(existing);

      assert!(
         text.contains("# why the endpoint is polled so rarely here"),
         "{text}"
      );
      assert!(text.contains("# deliberate"), "{text}");
      assert!(text.contains("api_interval_secs = 900"), "{text}");
      // And the new value is what comes back, not the one in the comment.
      assert_eq!(Config::parse(&text).0.api_interval_secs, 900);
   }

   #[test]
   fn a_key_that_is_no_longer_written_leaves_the_file() {
      // Otherwise a migrated key sits in the file forever, saying something
      // the widget stopped believing — two answers to one question, and the
      // stale one is the one a person reads.
      let existing = "\
position = [60.0, 60.0]
source = \"api\"

[behaviour]
lang = \"ru\"
";
      let text = Config::default().render(existing);
      assert!(!text.contains("position"), "{text}");
      assert!(!text.contains("source ="), "{text}");
      assert!(text.contains("lang"), "{text}");
   }

   #[test]
   fn the_window_rectangles_are_read_from_an_old_config_and_not_written_back() {
      // They moved to `window.json` because a drag rewrote the config and
      // took any comment in it along. Reading them still has to work — the
      // alternative is a widget that forgets where it was on upgrade — but
      // once read they must leave the file for good, or there would be two
      // answers to one question.
      let old = r#"
            position = [832.8, 820.8]
            settings_pos = [1044.8, 82.4]
            settings_size = [433.6, 640.8]
            opacity = 0.9
        "#;
      let mut cfg: Config = toml::from_str(old).expect("an old config still parses");
      assert_eq!(cfg.legacy_position, Some([832.8, 820.8]));
      assert_eq!(cfg.legacy_settings_pos, Some([1044.8, 82.4]));
      assert_eq!(cfg.legacy_settings_size, Some([433.6, 640.8]));
      // Everything else is read as before.
      assert_eq!(cfg.opacity, 0.9);

      // What `migrate_geometry` does with them, minus the file it writes.
      cfg.legacy_position.take();
      cfg.legacy_settings_pos.take();
      cfg.legacy_settings_size.take();

      let text = toml::to_string_pretty(&cfg).unwrap();
      assert!(!text.contains("position"), "still in the file:\n{text}");
      assert!(!text.contains("settings_"), "still in the file:\n{text}");
   }

   #[test]
   fn an_old_single_mode_becomes_switches() {
      let mut cfg: Config = toml::from_str(r#"source = "api""#).unwrap();
      assert!(cfg.migrate());
      assert!(cfg.api_enabled && !cfg.bridge_enabled && !cfg.desktop_enabled);
      assert!(cfg.legacy_source.is_none());
      // And the key is gone from what gets written back.
      assert!(!toml::to_string_pretty(&cfg).unwrap().contains("source ="));
   }

   #[test]
   fn auto_becomes_all_three_and_migration_happens_once() {
      let mut cfg: Config = toml::from_str(r#"source = "auto""#).unwrap();
      assert!(cfg.migrate());
      assert!(cfg.bridge_enabled && cfg.api_enabled && cfg.desktop_enabled);
      assert!(!cfg.migrate(), "nothing left to migrate the second time");
   }

   #[test]
   fn a_config_without_the_old_key_is_left_alone() {
      let mut cfg: Config = toml::from_str("api_enabled = false").unwrap();
      assert!(!cfg.migrate());
      assert!(!cfg.api_enabled);
      assert!(cfg.bridge_enabled, "untouched keys keep their defaults");
   }

   #[test]
   fn staleness_is_per_source() {
      let cfg = Config::default();
      assert_eq!(cfg.stale_secs(SourceKind::Api), 900);
      assert!(
         cfg.stale_secs(SourceKind::Desktop) > cfg.stale_secs(SourceKind::Api),
         "the desktop history is written every ~15 min and must not be \
             called stale for it"
      );
   }

   #[test]
   fn spending_is_the_reading_itself() {
      assert_eq!(PctMode::Spent.shown(48.0), 48.0);
      assert_eq!(PctMode::Spent.shown(140.0), 140.0);
   }

   #[test]
   fn what_is_left_never_goes_below_zero() {
      assert_eq!(PctMode::Left.shown(40.0), 60.0);
      assert_eq!(PctMode::Left.shown(100.0), 0.0);
      // A spend limit can read past 100%; there is still nothing left.
      assert_eq!(PctMode::Left.shown(140.0), 0.0);
   }

   #[test]
   fn the_mode_toggles_back_to_where_it_started() {
      assert_eq!(PctMode::Spent.other(), PctMode::Left);
      assert_eq!(PctMode::Spent.other().other(), PctMode::Spent);
   }

   #[test]
   fn the_old_pace_keys_become_a_form() {
      let mut cfg: Config = toml::from_str("pace_cell = true\npace_unit = \"points\"").unwrap();
      assert!(cfg.migrate());
      assert_eq!(cfg.pace_view, PaceView::Points);
      let written = toml::to_string_pretty(&cfg).unwrap();
      assert!(!written.contains("pace_cell"), "{written}");
      assert!(!written.contains("pace_unit"), "{written}");
      assert!(written.contains("pace_view = \"points\""), "{written}");
   }

   #[test]
   fn a_pace_cell_that_was_off_stays_off() {
      let mut cfg: Config = toml::from_str("pace_cell = false").unwrap();
      assert!(cfg.migrate());
      assert_eq!(cfg.pace_view, PaceView::Off);
      assert!(!cfg.migrate(), "nothing left to migrate the second time");
   }

   #[test]
   fn a_form_knows_which_parts_it_shows() {
      assert!(PaceView::Split.has_points() && PaceView::Split.has_ratio());
      assert!(PaceView::Split.points_first());
      assert!(PaceView::Piped.has_points() && PaceView::Piped.has_ratio());
      assert!(PaceView::Piped.points_first());
      // The piped form is the only one that writes its own separator.
      assert_eq!(PaceView::Piped.joiner(), "|");
      assert_eq!(PaceView::Split.joiner(), " ");
      assert_eq!(PaceView::Ratio.joiner(), " ");
      assert!(!PaceView::Off.has_points() && !PaceView::Off.has_ratio());
      assert!(PaceView::Ratio.has_ratio() && !PaceView::Ratio.has_points());
      // Time stands alone: neither of the other two rides along on the pill.
      assert!(!PaceView::Time.has_points() && !PaceView::Time.has_ratio());
      assert!(!PaceView::Time.points_first());
      // The panel says both, in whatever order the pill was asked for.
      assert_eq!(PaceView::Points.both(), PaceView::PointsRatio);
      assert_eq!(PaceView::Off.both(), PaceView::RatioPoints);
   }

   #[test]
   fn a_config_written_before_the_setting_shows_spending() {
      let cfg: Config = toml::from_str("").unwrap();
      assert_eq!(cfg.pct_mode, PctMode::Spent);
      // And keeps colouring its bars the way it always did.
      assert_eq!(cfg.bar_colour, BarColour::Fill);
   }

   #[test]
   fn a_setting_that_no_longer_exists_is_taken_out_of_the_file() {
      // The nine sizes moved into the theme. A config still carrying them
      // must come back without them: a line that looks like a setting and
      // does nothing is worse than no line, because it is tried first.
      let before = "[style]
scale = 1.1
corner_radius = 10.0
pill_height = 34.0
gaps_x = 1.0
";
      let after = Config::default().render(before);
      assert!(after.contains("scale"), "{after}");
      for gone in ["corner_radius", "pill_height", "gaps_x"] {
         assert!(
            !after.contains(gone),
            "`{gone}` survived:
{after}"
         );
      }
   }

   #[test]
   fn a_config_written_before_the_transcript_source_gets_it_switched_on() {
      // No migration is written for these keys and none is needed: the struct
      // carries `#[serde(default)]`, so an old file reads back with every new
      // key at its default. Said out loud here because the migration
      // machinery next door invites the opposite assumption.
      let cfg: Config = toml::from_str(r#"api_enabled = true"#).unwrap();
      assert!(cfg.session_enabled);
      assert_eq!(cfg.session_pick, SessionPick::Live);
      assert_eq!(cfg.ctx_view, CtxView::Pct);
      assert_eq!(cfg.ctx_limit, CtxLimit::Learned);
      assert!(cfg.ctx_dir_linked);
      assert_eq!(cfg.ctx_pct_dir, PctMode::Spent);
   }

   #[test]
   fn ctx_direction_defaults_to_spent_regardless_of_pct_mode() {
      // `pct_mode` is tuned for quotas that drain; a window fills instead,
      // so an existing config with `pct_mode = left` must not silently flip
      // ctx to "remaining" the way it once did.
      let cfg = Config {
         pct_mode: PctMode::Left,
         ..Config::default()
      };
      assert_eq!(cfg.ctx_pct_dir, PctMode::Spent);
      assert_eq!(cfg.ctx_tokens_dir(), PctMode::Spent);
   }

   #[test]
   fn linked_ctx_directions_follow_the_percentage_until_split() {
      let mut cfg = Config {
         ctx_pct_dir: PctMode::Left,
         ..Config::default()
      };
      // Still linked: the token direction mirrors the percentage even though
      // the `ctx_tokens_dir` field itself was never touched.
      assert_eq!(cfg.ctx_tokens_dir(), PctMode::Left);

      cfg.ctx_dir_linked = false;
      cfg.ctx_tokens_dir = PctMode::Spent;
      // Split: each figure answers for itself now.
      assert_eq!(cfg.ctx_tokens_dir(), PctMode::Spent);
      assert_eq!(cfg.ctx_pct_dir, PctMode::Left);
   }

   #[test]
   fn the_context_settings_are_clamped_like_the_rest() {
      let mut cfg = Config {
         ctx_limit_tokens: 10,
         ..Config::default()
      };
      assert_eq!(cfg.ctx_limit_tokens(), 8_192);
      cfg.ctx_limit_tokens = 9_000_000;
      assert_eq!(cfg.ctx_limit_tokens(), 1_000_000);
      cfg.session_scan_secs = 0;
      assert_eq!(cfg.session_scan_secs(), 5);
      cfg.session_scan_secs = 99_999;
      assert_eq!(cfg.session_scan_secs(), 600);
      // The fourth source ages on its own clock like the other three, and
      // is clamped where it is read rather than where it is written.
      cfg.session_stale_secs = 1;
      assert_eq!(cfg.stale_secs(SourceKind::Session), 30);
   }

   /// A directory of this test's own, under the system temp. Nothing here
   /// goes near `config_path()`: the real config belongs to whoever is
   /// running the tests.
   fn scratch(name: &str) -> PathBuf {
      let dir = std::env::temp_dir().join(format!("ccd-watch-{}-{name}", std::process::id()));
      let _ = std::fs::create_dir_all(&dir);
      dir.join("config.toml")
   }

   #[test]
   fn a_file_that_has_not_moved_is_not_a_change() {
      let path = scratch("still");
      std::fs::write(
         &path,
         "lang = \"ru\"
",
      )
      .unwrap();
      let mut watch = Watch::at(path.clone());
      assert!(!watch.changed(), "nobody touched it");
      assert!(!watch.changed(), "and asking twice does not make it so");
      let _ = std::fs::remove_file(&path);
   }

   #[test]
   fn a_config_written_for_the_first_time_is_a_change() {
      // Someone who has never opened the settings window has no config file
      // at all; creating one by hand has to count.
      let path = scratch("appears");
      let _ = std::fs::remove_file(&path);
      let mut watch = Watch::at(path.clone());
      std::fs::write(
         &path,
         "lang = \"en\"
",
      )
      .unwrap();
      assert!(watch.changed());
      let _ = std::fs::remove_file(&path);
   }

   #[test]
   fn the_widgets_own_write_is_not_an_edit_but_a_comment_added_to_it_is() {
      // The trap this watcher exists to avoid: the widget writes the config
      // from some forty places, and a watcher that reads its own writing
      // back either loops or overwrites the click that caused it. Both halves
      // are one test because they share `LAST_WRITTEN`, which is global.
      let path = scratch("own-write");
      let ours = "lang = \"ru\"
scale = 1.1
";
      std::fs::write(&path, ours).unwrap();
      *LAST_WRITTEN.lock().unwrap() = Some(ours.to_string());

      let mut watch = Watch::at(path.clone());
      // The timestamp moved — that is what a write does. Forced rather than
      // slept for: two writes inside one clock tick share an mtime on
      // Windows, and a test that waits out the tick is a test that sometimes
      // does not.
      watch.mtime = Some(std::time::SystemTime::UNIX_EPOCH);
      assert!(!watch.changed(), "this is our own text coming back");

      std::fs::write(
         &path,
         format!(
            "# a note from a person
{ours}"
         ),
      )
      .unwrap();
      watch.mtime = Some(std::time::SystemTime::UNIX_EPOCH);
      assert!(watch.changed(), "one comment more than we wrote is an edit");

      *LAST_WRITTEN.lock().unwrap() = None;
      let _ = std::fs::remove_file(&path);
   }

   #[test]
   fn the_short_weekly_form_reads_its_old_name_and_a_sane_separator() {
      // Written `colon` before the separator could be chosen.
      let cfg = Config::parse("days_form = \"colon\"\npace_form = \"colon\"\n").0;
      assert_eq!(cfg.days_form, DaysForm::Short);
      assert_eq!(cfg.pace_form, DaysForm::Short);
      assert_eq!(cfg.weekly_sep(), "'");
      assert_eq!(
         cfg.days_span(),
         crate::pace::Span::Short {
            sep: "'",
            pad: false
         }
      );
      assert_eq!(Config::default().days_span(), crate::pace::Span::Units);
      let sep = |s: &str| Config {
         weekly_sep: s.into(),
         ..Config::default()
      };
      assert_eq!(
         sep("").weekly_sep(),
         "'",
         "nothing between the two says nothing"
      );
      assert_eq!(sep(" ").weekly_sep(), " ", "a space is a fair choice");
      assert_eq!(sep("·").weekly_sep(), "·");
      assert_eq!(sep("д··ч").weekly_sep(), "д··");
   }

   #[test]
   fn a_context_form_knows_which_parts_it_shows() {
      assert!(CtxView::PctTokens.has_pct() && CtxView::PctTokens.has_tokens());
      assert!(!CtxView::PctTokens.tokens_first());
      assert!(CtxView::TokensPct.tokens_first());
      assert!(!CtxView::Off.has_pct() && !CtxView::Off.has_tokens());
      assert!(
         CtxView::Tokens.tokens_first(),
         "with nothing else, the count is first"
      );
   }
}
