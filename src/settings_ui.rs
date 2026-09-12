//! The settings window, walked out of `schema.rs`.
//!
//! Nothing here knows what any particular setting is. The schema describes all
//! forty-five once — kind, bounds, step sizes, label and tooltip in both
//! languages — and this turns that list into rows, and a click on a row back
//! into a value. The window it replaces was thirteen hundred lines of egui in
//! which every setting was spelled out four times over, which is why adding
//! one used to mean touching four places and why two of them were usually
//! forgotten.
//!
//! Compiled rather than interpreted, unlike the pill's theme (A24): a form's
//! appearance is not a setting, and it wants `std-widgets`.

#![cfg(windows)]

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::config::Config;
use crate::pace;
use crate::schema::{self, Kind, Section, Setting, Show};

slint::include_modules!();

/// What a row's `kind` means to the `.slint` side.
const KIND_BOOL: i32 = 0;
const KIND_NUMBER: i32 = 1;
const KIND_CHOICE: i32 = 2;
const KIND_TEXT: i32 = 3;

/// A number as it should read, rather than as it is stored.
///
/// `opacity` is a float from 0 to 1 and reads as a percentage;
/// `bridge_stale_secs` is a count of seconds and reads as `15:00`. A form that
/// showed the stored number would be asking people to do arithmetic to use it.
fn shown(value: &toml::Value, show: Show) -> String {
    match show {
        Show::Plain => match value {
            toml::Value::Integer(n) => n.to_string(),
            toml::Value::Float(f) => format!("{f:.2}"),
            other => other.to_string(),
        },
        Show::Percent => match value.as_float() {
            Some(f) => format!("{:.0}%", f * 100.0),
            None => value.to_string(),
        },
        Show::Points => match (value.as_float(), value.as_integer()) {
            (Some(f), _) => format!("{f:.0}%"),
            (_, Some(n)) => format!("{n}%"),
            _ => value.to_string(),
        },
        Show::Duration => match value.as_integer() {
            Some(n) => pace::fmt_duration(n),
            None => value.to_string(),
        },
        Show::Tokens => match value.as_integer() {
            Some(n) => pace::fmt_tokens(n.max(0) as u64),
            None => value.to_string(),
        },
    }
}

/// Every setting that belongs in a form, in the order the file is written.
///
/// The section title rides on the first row of its section rather than in a
/// model of its own: two lists to keep in step is one more than necessary, and
/// the one that drifts is always the one nobody is looking at.
pub fn rows(cfg: &Config) -> ModelRc<SettingRow> {
    let ru = cfg.is_ru();
    let mut out: Vec<SettingRow> = Vec::new();
    for section in Section::ALL {
        let mut first = true;
        for setting in schema::ALL.iter().filter(|s| s.section == section && s.form) {
            let Some(value) = schema::read(cfg, setting.key) else { continue };
            out.push(row(setting, &value, ru, if first { section.title(ru) } else { "" }));
            first = false;
        }
    }
    ModelRc::new(VecModel::from(out))
}

fn row(setting: &Setting, value: &toml::Value, ru: bool, section: &str) -> SettingRow {
    let (kind, on, text, choices, choice) = match setting.kind {
        Kind::Bool => (
            KIND_BOOL,
            value.as_bool().unwrap_or(false),
            String::new(),
            Vec::new(),
            0,
        ),
        Kind::Int { show, .. } | Kind::Float { show, .. } => {
            (KIND_NUMBER, false, shown(value, show), Vec::new(), 0)
        }
        Kind::Choice(options) => {
            let current = value.as_str().unwrap_or_default();
            let at = options.iter().position(|o| o.value == current).unwrap_or(0);
            (
                KIND_CHOICE,
                false,
                String::new(),
                options.iter().map(|o| SharedString::from(o.label.get(ru))).collect(),
                at as i32,
            )
        }
        Kind::Text => (
            KIND_TEXT,
            false,
            value.as_str().unwrap_or_default().to_string(),
            Vec::new(),
            0,
        ),
    };
    SettingRow {
        key: setting.key.into(),
        label: setting.label.get(ru).into(),
        tip: setting.tip.get(ru).into(),
        kind,
        on,
        text: text.into(),
        choices: ModelRc::new(VecModel::from(choices)),
        choice,
        section: section.into(),
    }
}

/// Move a number by one of its two step sizes. `delta` is −2, −1, 1 or 2: the
/// big step down, the small one, and the same upwards.
///
/// Two sizes because the gestures differ — a threshold is tuned by one, a size
/// is hunted for in fours — and both come from the schema rather than from
/// here, so the feel of a setting is described in the same place as the rest
/// of it.
fn stepped(setting: &Setting, value: &toml::Value, delta: i32) -> Option<toml::Value> {
    let big = delta.abs() > 1;
    let sign = if delta < 0 { -1.0 } else { 1.0 };
    match setting.kind {
        Kind::Int { min, max, step, big: jump, .. } => {
            let by = if big { jump } else { step } as f64 * sign;
            let next = (value.as_integer()? as f64 + by).round() as i64;
            Some(toml::Value::Integer(next.clamp(min, max)))
        }
        Kind::Float { min, max, step, big: jump, .. } => {
            let by = f64::from(if big { jump } else { step }) * sign;
            let next = value.as_float()? + by;
            // Rounded to the step, so a run of clicks does not accumulate the
            // dust of binary fractions into a number nobody typed.
            let by_abs = f64::from(if big { jump } else { step }).abs();
            let next = if by_abs > 0.0 { (next / by_abs).round() * by_abs } else { next };
            Some(toml::Value::Float(next.clamp(min as f64, max as f64)))
        }
        _ => None,
    }
}

/// The one closure every click in the form reaches, shared by the eight
/// callbacks that can produce one.
type Handler = Rc<RefCell<Box<dyn FnMut(&str, Change)>>>;

/// The settings window, and everything it can ask the widget to do.
///
/// Built the first time it is wanted and kept after that: a form is cheap to
/// leave in memory and slow to rebuild, and its position is worth remembering
/// between openings.
pub struct Form {
    window: Settings,
    open: bool,
}

impl Form {
    /// `sink` is handed every change; the caller owns the config and the
    /// saving, because this module has no business deciding when a file is
    /// written.
    pub fn new(
        cfg: &Config,
        theme_path: &std::path::Path,
        trouble: &str,
        apply: impl FnMut(&str, Change) + 'static,
    ) -> Result<Form, String> {
        let window = Settings::new().map_err(|e| format!("settings window: {e}"))?;
        window.set_rows(rows(cfg));
        window.set_ru(cfg.is_ru());
        window.set_theme_path(theme_path.to_string_lossy().as_ref().into());
        window.set_theme_trouble(trouble.into());

        let handlers: Handler = Rc::new(RefCell::new(Box::new(apply)));

        {
            let h = handlers.clone();
            window.on_toggled(move |key, on| (h.borrow_mut())(&key, Change::Bool(on)));
        }
        {
            let h = handlers.clone();
            window.on_stepped(move |key, delta| (h.borrow_mut())(&key, Change::Step(delta)));
        }
        {
            let h = handlers.clone();
            window.on_chose(move |key, at| (h.borrow_mut())(&key, Change::Choice(at)));
        }
        {
            let h = handlers.clone();
            window.on_edited(move |key, text| {
                (h.borrow_mut())(&key, Change::Text(text.to_string()))
            });
        }
        {
            let h = handlers.clone();
            window.on_pin(move |id| (h.borrow_mut())("", Change::Pin(id.to_string())));
        }
        {
            let h = handlers.clone();
            window.on_restart(move || (h.borrow_mut())("", Change::Restart));
        }
        {
            let h = handlers.clone();
            window.on_open_config(move || (h.borrow_mut())("", Change::OpenConfig));
        }
        {
            let h = handlers.clone();
            window.on_open_theme(move || (h.borrow_mut())("", Change::OpenTheme));
        }

        {
            let h = handlers.clone();
            let weak = window.as_weak();
            window.window().on_close_requested(move || {
                if let Some(w) = weak.upgrade() {
                    let scale = w.window().scale_factor();
                    let pos = w.window().position().to_logical(scale);
                    let size = w.window().size().to_logical(scale);
                    (h.borrow_mut())(
                        "",
                        Change::Closed {
                            pos: [pos.x, pos.y],
                            size: [size.width, size.height],
                        },
                    );
                }
                // Hidden rather than destroyed: the form is cheap to keep and
                // slow to rebuild, and closing it must not take the widget with
                // it (G20).
                slint::CloseRequestResponse::HideWindow
            });
        }

        Ok(Form { window, open: false })
    }

    pub fn show(&mut self) {
        let _ = self.window.show();
        self.open = true;
    }

    /// The window closed itself; remember that without touching it again.
    pub fn mark_closed(&mut self) {
        self.open = false;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn window(&self) -> &slint::Window {
        self.window.window()
    }

    /// The conversations to offer, and which one is followed now.
    ///
    /// Handed in rather than read here: enumerating them costs a directory
    /// walk and a file open per name, and what may be spent on that is the
    /// caller's business, not a form's.
    pub fn set_sessions(&self, title: &str, sessions: Vec<SessionRow>) {
        self.window.set_sessions_title(title.into());
        self.window.set_sessions(ModelRc::new(VecModel::from(sessions)));
    }

    /// What the usage endpoint is doing, in the words `view.rs` gives for it.
    pub fn set_api_state(&self, state: Option<String>) {
        self.window.set_api_state(state.unwrap_or_default().into());
    }

    /// Rewrite every row from the config.
    ///
    /// Whole rather than in place: forty-five rows is nothing to rebuild, and a
    /// setting that changes another one's meaning — `ctx_dir_linked`, the pace
    /// view, the language — would otherwise leave stale rows behind.
    pub fn refresh(&self, cfg: &Config) {
        self.window.set_rows(rows(cfg));
        self.window.set_ru(cfg.is_ru());
    }
}

/// What one interaction with the form was.
pub enum Change {
    Bool(bool),
    Step(i32),
    Choice(i32),
    Text(String),
    /// Follow this conversation, by id.
    Pin(String),
    Restart,
    OpenConfig,
    OpenTheme,
    /// The window was closed, and this is where it was left. Remembered in
    /// `window.json` like the pill's own position — it is the machine's memory
    /// of itself, not a setting (A22).
    Closed { pos: [f32; 2], size: [f32; 2] },
}

/// Apply one change to the config. Returns whether anything actually moved.
///
/// The clamping lives in `config.rs`, where a hand-edited file meets it too;
/// what happens here is only that a value is chosen. Both have to agree, and a
/// test in `schema.rs` is what keeps them agreeing.
pub fn apply(cfg: &mut Config, key: &str, change: &Change) -> bool {
    let Some(setting) = schema::get(key) else { return false };
    let Some(current) = schema::read(cfg, key) else { return false };
    let next = match change {
        Change::Bool(on) => Some(toml::Value::Boolean(*on)),
        Change::Step(delta) => stepped(setting, &current, *delta),
        Change::Choice(at) => match setting.kind {
            Kind::Choice(options) => options
                .get((*at).max(0) as usize)
                .map(|o| toml::Value::String(o.value.to_string())),
            _ => None,
        },
        Change::Text(text) => Some(toml::Value::String(text.clone())),
        _ => None,
    };
    match next {
        Some(value) if value != current => schema::write(cfg, key, value),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slint::Model;

    #[test]
    fn every_setting_marked_for_the_form_gets_a_row() {
        // The failure this catches is a setting that exists, persists, and is
        // simply not offered — invisible from either end.
        let cfg = Config::default();
        let rows = rows(&cfg);
        let wanted = schema::ALL.iter().filter(|s| s.form).count();
        assert_eq!(rows.row_count(), wanted);
    }

    #[test]
    fn each_section_is_announced_exactly_once() {
        let cfg = Config::default();
        let rows = rows(&cfg);
        let titles: Vec<String> = rows
            .iter()
            .map(|r| r.section.to_string())
            .filter(|t| !t.is_empty())
            .collect();
        assert_eq!(titles.len(), Section::ALL.len(), "{titles:?}");
        let mut sorted = titles.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), titles.len(), "a section announced twice");
    }

    #[test]
    fn every_stepper_stops_at_its_own_bounds() {
        // Walked against the schema's own numbers rather than against any
        // written down here: a bound copied into a test is a second place for
        // it to live, and the whole point of the schema is that there is one.
        for setting in schema::ALL.iter().filter(|s| s.form) {
            let (min, max) = match setting.kind {
                Kind::Int { min, max, .. } => (min as f64, max as f64),
                Kind::Float { min, max, .. } => (min as f64, max as f64),
                _ => continue,
            };
            for (direction, bound) in [(2, max), (-2, min)] {
                let mut cfg = Config::default();
                for _ in 0..2_000 {
                    if !apply(&mut cfg, setting.key, &Change::Step(direction)) {
                        break;
                    }
                }
                let landed = schema::read(&cfg, setting.key)
                    .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)))
                    .unwrap_or(f64::NAN);
                assert!(
                    (landed - bound).abs() < 1e-6,
                    "{}: stepping {} stopped at {landed}, not at {bound}",
                    setting.key,
                    if direction > 0 { "up" } else { "down" }
                );
            }
        }
    }

    #[test]
    fn a_choice_is_written_as_the_spelling_and_not_as_the_label() {
        // The label is Russian; the file must get `left`.
        let mut cfg = Config::default();
        assert!(apply(&mut cfg, "pct_mode", &Change::Choice(1)));
        let written = schema::read(&cfg, "pct_mode").unwrap();
        assert_eq!(written.as_str(), Some("left"));
    }

    #[test]
    fn a_change_that_changes_nothing_reports_as_much() {
        // What stops a form from writing the config file on every repaint.
        let mut cfg = Config::default();
        let was = cfg.pct_mode;
        assert!(!apply(&mut cfg, "pct_mode", &Change::Choice(0)));
        assert_eq!(cfg.pct_mode, was);
    }

    #[test]
    fn seconds_read_as_a_clock_and_tokens_as_tokens() {
        assert_eq!(shown(&toml::Value::Integer(900), Show::Duration), "15:00");
        assert_eq!(shown(&toml::Value::Integer(200_000), Show::Tokens), "200k");
        assert_eq!(shown(&toml::Value::Float(0.88), Show::Percent), "88%");
        assert_eq!(shown(&toml::Value::Float(85.0), Show::Points), "85%");
    }
}
