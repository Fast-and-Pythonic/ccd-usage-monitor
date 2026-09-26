//! The settings window, walked out of `schema.rs`.
//!
//! Nothing here knows what any particular setting is. The schema describes
//! every one once — kind, bounds, step sizes, label and tooltip in both
//! languages, the card it sits on and what it depends on — and this turns that
//! list into pages of cards, and a click on a row back into a value. The window
//! it replaces was thirteen hundred lines of egui in which every setting was
//! spelled out four times over, which is why adding one used to mean touching
//! four places and why two of them were usually forgotten.
//!
//! Compiled rather than interpreted, unlike the pill's theme (A24): a form's
//! appearance is not a setting, and it wants `std-widgets`.

#![cfg(windows)]

use std::cell::RefCell;
use std::rc::Rc;

use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::config::Config;
use crate::pace;
use crate::schema::{self, Group, Kind, Otherwise, Page, Setting, Show};

slint::include_modules!();

/// What a row's `control` means to the `.slint` side.
const SWITCH: i32 = 0;
const SLIDER: i32 = 1;
const STEPPER: i32 = 2;
const SEGMENTED: i32 = 3;
const COMBO: i32 = 4;
const TEXT: i32 = 5;
const ACTION: i32 = 6;
const NOTE: i32 = 7;
const SESSIONS: i32 = 8;
const ABOUT: i32 = 9;

/// Up to this many choices sit side by side as buttons; more go in a menu.
const SEGMENTS_MAX: usize = 3;

/// What control a setting gets. Decided by its kind alone, so a new setting
/// needs no word about how it is drawn.
///
/// Durations and token counts get a stepper rather than a slider: their ranges
/// run from half a minute to a day, or from eight thousand tokens to a million,
/// and a linear track that long puts every value anyone uses in its first few
/// pixels.
fn control(setting: &Setting) -> i32 {
   match setting.kind {
      Kind::Bool => SWITCH,
      Kind::Choice(options) if options.len() <= SEGMENTS_MAX => SEGMENTED,
      Kind::Choice(_) => COMBO,
      Kind::Int { show, .. } | Kind::Float { show, .. } => match show {
         Show::Duration | Show::Tokens => STEPPER,
         Show::Plain | Show::Percent | Show::Points => SLIDER,
      },
      Kind::Text => TEXT,
   }
}

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

/// A row on a card that is not a setting: a button, a line of status, the
/// conversation list, the licence block. Each belongs to the card it is listed
/// with here and stands after that card's settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Extra {
   ApiState,
   Sessions,
   OpenTheme,
   OpenConfig,
   Restart,
   About,
}

const EXTRAS: &[(Group, Extra)] = &[
   (Group::SrcApi, Extra::ApiState),
   (Group::Session, Extra::Sessions),
   (Group::Theme, Extra::OpenTheme),
   (Group::Files, Extra::OpenConfig),
   (Group::Files, Extra::Restart),
   (Group::About, Extra::About),
];

/// What the extras say, which comes from the running widget rather than from
/// the config.
#[derive(Clone, Default)]
pub struct Context {
   pub theme_path: String,
   pub api_state: String,
   /// Whether there is a conversation list to show: only while one is pinned.
   pub sessions: bool,
}

/// The side list of the window.
fn pages(ru: bool) -> ModelRc<PageRow> {
   let rows: Vec<PageRow> = Page::ALL
      .iter()
      .map(|p| PageRow {
         title: p.title(ru).into(),
         icon: p.icon().into(),
      })
      .collect();
   ModelRc::new(VecModel::from(rows))
}

/// Every card, each with its rows, in the order they are shown.
///
/// The number of rows on a card never depends on the config: a row that does
/// not apply is marked hidden rather than left out, so the form can rewrite its
/// rows in place instead of rebuilding them — which is what keeps the scroll
/// position and a slider being dragged from being thrown away on every click.
fn cards(cfg: &Config, ctx: &Context) -> Vec<(Group, Vec<SettingRow>)> {
   let ru = cfg.is_ru();
   Group::ALL
      .iter()
      .map(|&group| {
         let headed = group.title(ru).is_none();
         let mut rows: Vec<SettingRow> = schema::ALL
            .iter()
            .filter(|s| s.group == Some(group))
            .filter_map(|s| {
               let value = schema::read(cfg, s.key)?;
               Some(row(s, &value, cfg))
            })
            .collect();
         rows.extend(
            EXTRAS
               .iter()
               .filter(|(g, _)| *g == group)
               .map(|(_, extra)| extra_row(*extra, ru, ctx)),
         );
         if headed && let Some(first) = rows.first_mut() {
            first.headed = true;
         }
         (group, rows)
      })
      .collect()
}

fn blank() -> SettingRow {
   SettingRow {
      key: SharedString::new(),
      label: SharedString::new(),
      lead: SharedString::new(),
      tip: SharedString::new(),
      more: false,
      control: NOTE,
      on: false,
      value: 0.0,
      min: 0.0,
      max: 1.0,
      step: 1.0,
      big: 1.0,
      text: SharedString::new(),
      choices: ModelRc::default(),
      choice: 0,
      sub: false,
      dim: false,
      hidden: false,
      headed: false,
      action: SharedString::new(),
   }
}

fn row(setting: &Setting, value: &toml::Value, cfg: &Config) -> SettingRow {
   let ru = cfg.is_ru();
   // Backticks mark a name in the source; on screen they are only noise.
   let tip = setting.tip.get(ru).replace('`', "");
   let lead = schema::lead(&tip);
   let (dim, hidden) = match setting.needs {
      Some(n) if !n.met(cfg) => (
         n.otherwise == Otherwise::Dim,
         n.otherwise == Otherwise::Hide,
      ),
      _ => (false, false),
   };
   let mut r = SettingRow {
      key: setting.key.into(),
      label: setting.label.get(ru).into(),
      lead: lead.into(),
      tip: tip.as_str().into(),
      more: lead.len() < tip.len(),
      control: control(setting),
      sub: setting.needs.is_some(),
      dim,
      hidden,
      ..blank()
   };
   match setting.kind {
      Kind::Bool => r.on = value.as_bool().unwrap_or(false),
      Kind::Int {
         min,
         max,
         step,
         big,
         show,
      } => {
         r.value = value.as_integer().unwrap_or(min) as f32;
         (r.min, r.max, r.step, r.big) = (min as f32, max as f32, step as f32, big as f32);
         r.text = shown(value, show).into();
      }
      Kind::Float {
         min,
         max,
         step,
         big,
         show,
      } => {
         r.value = value.as_float().unwrap_or(min as f64) as f32;
         (r.min, r.max, r.step, r.big) = (min, max, step, big);
         r.text = shown(value, show).into();
      }
      Kind::Choice(options) => {
         let current = value.as_str().unwrap_or_default();
         r.choice = options.iter().position(|o| o.value == current).unwrap_or(0) as i32;
         let names: Vec<SharedString> = options.iter().map(|o| o.label.get(ru).into()).collect();
         r.choices = ModelRc::new(VecModel::from(names));
      }
      Kind::Text => r.text = value.as_str().unwrap_or_default().into(),
   }
   r
}

fn extra_row(extra: Extra, ru: bool, ctx: &Context) -> SettingRow {
   let t =
      |ru_text: &str, en_text: &str| -> SharedString { if ru { ru_text } else { en_text }.into() };
   let open = t("Открыть", "Open");
   match extra {
      Extra::ApiState => SettingRow {
         control: NOTE,
         label: ctx.api_state.as_str().into(),
         hidden: ctx.api_state.is_empty(),
         sub: true,
         ..blank()
      },
      Extra::Sessions => SettingRow {
         control: SESSIONS,
         hidden: !ctx.sessions,
         sub: true,
         ..blank()
      },
      Extra::OpenTheme => SettingRow {
         control: ACTION,
         label: t("Файл темы", "Theme file"),
         lead: ctx.theme_path.as_str().into(),
         text: open,
         action: "open_theme".into(),
         ..blank()
      },
      Extra::OpenConfig => SettingRow {
         control: ACTION,
         label: "config.toml".into(),
         lead: t(
            "Все настройки одним файлом; правка вручную подхватывается на ходу.",
            "Every setting in one file; an edit by hand is picked up while running.",
         ),
         text: open,
         action: "open_config".into(),
         ..blank()
      },
      Extra::Restart => SettingRow {
         control: ACTION,
         label: t("Перезапуск", "Restart"),
         lead: t(
            "Закрыть виджет и запустить его снова.",
            "Close the widget and start it again.",
         ),
         text: t("Перезапустить", "Restart"),
         action: "restart".into(),
         ..blank()
      },
      Extra::About => SettingRow {
         control: ABOUT,
         ..blank()
      },
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
   let current = value
      .as_float()
      .or_else(|| value.as_integer().map(|n| n as f64))?;
   let by = match setting.kind {
      Kind::Int {
         step, big: jump, ..
      } => (if big { jump } else { step }) as f64,
      Kind::Float {
         step, big: jump, ..
      } => f64::from(if big { jump } else { step }),
      _ => return None,
   };
   landed(setting, current + by * sign, by)
}

/// A number chosen outright — where a slider was let go — put on its setting's
/// small step and inside its bounds.
fn set_to(setting: &Setting, value: f64) -> Option<toml::Value> {
   let by = match setting.kind {
      Kind::Int { step, .. } => step as f64,
      Kind::Float { step, .. } => f64::from(step),
      _ => return None,
   };
   landed(setting, value, by)
}

/// Round to a multiple of `by` and clamp: the one place both a step and a
/// slider end up, so a run of clicks does not accumulate the dust of binary
/// fractions into a number nobody typed.
fn landed(setting: &Setting, value: f64, by: f64) -> Option<toml::Value> {
   let snapped = if by > 0.0 {
      (value / by).round() * by
   } else {
      value
   };
   match setting.kind {
      Kind::Int { min, max, .. } => Some(toml::Value::Integer(
         (snapped.round() as i64).clamp(min, max),
      )),
      Kind::Float { min, max, .. } => {
         Some(toml::Value::Float(snapped.clamp(min as f64, max as f64)))
      }
      _ => None,
   }
}

/// The one closure every click in the form reaches, shared by the callbacks
/// that can produce one.
type Handler = Rc<RefCell<Box<dyn FnMut(&str, Change)>>>;

/// The settings window, and everything it can ask the widget to do.
///
/// Built the first time it is wanted and kept after that: a form is cheap to
/// leave in memory and slow to rebuild, and its position is worth remembering
/// between openings.
pub struct Form {
   window: Settings,
   open: bool,
   /// One model per card, in `Group::ALL` order, kept so a refresh can
   /// rewrite rows where they stand.
   cards: Vec<Rc<VecModel<SettingRow>>>,
   /// Where the usage endpoint's status line stands, as card and row. It
   /// changes every second while the window is open, so it is rewritten on
   /// its own rather than with every row.
   api_at: Option<(usize, usize)>,
   ctx: Context,
   ru: bool,
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
         window.on_slid(move |key, value, commit| {
            (h.borrow_mut())(
               &key,
               Change::Set {
                  value: f64::from(value),
                  commit,
               },
            )
         });
      }
      {
         let h = handlers.clone();
         window.on_reset(move |key| (h.borrow_mut())(&key, Change::Reset));
      }
      {
         let h = handlers.clone();
         window.on_chose(move |key, at| (h.borrow_mut())(&key, Change::Choice(at)));
      }
      {
         let h = handlers.clone();
         window.on_edited(move |key, text| (h.borrow_mut())(&key, Change::Text(text.to_string())));
      }
      {
         let h = handlers.clone();
         window.on_pin(move |id| (h.borrow_mut())("", Change::Pin(id.to_string())));
      }
      {
         let h = handlers.clone();
         window.on_action(move |what| {
            let change = match what.as_str() {
               "restart" => Change::Restart,
               "open_config" => Change::OpenConfig,
               "open_theme" => Change::OpenTheme,
               _ => return,
            };
            (h.borrow_mut())("", change)
         });
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

      let mut form = Form {
         window,
         open: false,
         cards: Vec::new(),
         api_at: None,
         ctx: Context {
            theme_path: theme_path.to_string_lossy().into_owned(),
            ..Context::default()
         },
         ru: cfg.is_ru(),
      };
      form.rebuild(cfg);
      Ok(form)
   }

   /// Build every card afresh. Only for the first showing and for a change
   /// of language; everything else is `refresh`.
   fn rebuild(&mut self, cfg: &Config) {
      self.ru = cfg.is_ru();
      let built = cards(cfg, &self.ctx);
      self.api_at = built.iter().enumerate().find_map(|(g, (group, _))| {
         let at = EXTRAS
            .iter()
            .filter(|(e, _)| e == group)
            .position(|(_, x)| *x == Extra::ApiState)?;
         let settings = schema::ALL
            .iter()
            .filter(|s| s.group == Some(*group))
            .count();
         Some((g, settings + at))
      });
      self.cards = built
         .into_iter()
         .map(|(_, rows)| Rc::new(VecModel::from(rows)))
         .collect();
      let groups: Vec<GroupRow> = Group::ALL
         .iter()
         .zip(&self.cards)
         .map(|(g, rows)| GroupRow {
            page: Page::ALL.iter().position(|p| *p == g.page()).unwrap_or(0) as i32,
            title: g.title(self.ru).unwrap_or_default().into(),
            rows: ModelRc::from(rows.clone()),
         })
         .collect();
      self.window.set_ru(self.ru);
      self.window.set_pages(pages(self.ru));
      self.window.set_groups(ModelRc::new(VecModel::from(groups)));
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
   pub fn set_sessions(&mut self, cfg: &Config, title: &str, sessions: Vec<SessionRow>) {
      self.ctx.sessions = !sessions.is_empty();
      self.window.set_sessions_title(title.into());
      self
         .window
         .set_sessions(ModelRc::new(VecModel::from(sessions)));
      self.refresh(cfg);
   }

   /// What the usage endpoint is doing, in the words `view.rs` gives for it.
   ///
   /// Called on every tick while the window is open, and the age in it moves
   /// every second: only that one row is touched, and only when its words
   /// changed (G23).
   pub fn set_api_state(&mut self, state: Option<String>) {
      let state = state.unwrap_or_default();
      if state == self.ctx.api_state {
         return;
      }
      self.ctx.api_state = state;
      let Some((g, at)) = self.api_at else {
         return;
      };
      let Some(model) = self.cards.get(g) else {
         return;
      };
      if let Some(mut r) = model.row_data(at) {
         r.label = self.ctx.api_state.as_str().into();
         r.hidden = self.ctx.api_state.is_empty();
         model.set_row_data(at, r);
      }
   }

   /// Bring every row up to the config.
   ///
   /// In place: each row is rewritten where it stands, so nothing on the
   /// page is rebuilt under the pointer. A change of language is the one
   /// exception, since it renames the pages and the cards too.
   pub fn refresh(&mut self, cfg: &Config) {
      if cfg.is_ru() != self.ru {
         return self.rebuild(cfg);
      }
      for ((_, rows), model) in cards(cfg, &self.ctx).into_iter().zip(&self.cards) {
         for (at, r) in rows.into_iter().enumerate() {
            if at < model.row_count() {
               model.set_row_data(at, r);
            }
         }
      }
   }
}

/// What one interaction with the form was.
pub enum Change {
   Bool(bool),
   Step(i32),
   /// A number chosen outright on a slider. `commit` is false while it is
   /// still being dragged: the pill follows, the file waits for the release.
   Set {
      value: f64,
      commit: bool,
   },
   /// Back to the default, by double click.
   Reset,
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
   Closed {
      pos: [f32; 2],
      size: [f32; 2],
   },
}

/// Apply one change to the config. Returns whether anything actually moved.
///
/// The clamping lives in `config.rs`, where a hand-edited file meets it too;
/// what happens here is only that a value is chosen. Both have to agree, and a
/// test in `schema.rs` is what keeps them agreeing.
pub fn apply(cfg: &mut Config, key: &str, change: &Change) -> bool {
   let Some(setting) = schema::get(key) else {
      return false;
   };
   let Some(current) = schema::read(cfg, key) else {
      return false;
   };
   let next = match change {
      Change::Bool(on) => Some(toml::Value::Boolean(*on)),
      Change::Step(delta) => stepped(setting, &current, *delta),
      Change::Set { value, .. } => set_to(setting, *value),
      Change::Reset => schema::read(&Config::default(), key),
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

   fn rows_of(cfg: &Config) -> Vec<SettingRow> {
      cards(cfg, &Context::default())
         .into_iter()
         .flat_map(|(_, rows)| rows)
         .collect()
   }

   fn find(rows: &[SettingRow], key: &str) -> SettingRow {
      rows
         .iter()
         .find(|r| r.key == key)
         .cloned()
         .unwrap_or_else(|| panic!("no row for {key}"))
   }

   #[test]
   fn every_setting_given_a_card_gets_exactly_one_row() {
      // The failure this catches is a setting that exists, persists, and is
      // simply not offered — invisible from either end.
      let cfg = Config::default();
      let rows = rows_of(&cfg);
      for s in schema::ALL {
         let n = rows.iter().filter(|r| r.key == s.key).count();
         let wanted = usize::from(s.group.is_some());
         assert_eq!(n, wanted, "{}: {n} rows", s.key);
      }
   }

   #[test]
   fn no_card_is_empty() {
      // A card whose settings all moved elsewhere would stand on its page
      // as a title over nothing.
      for (group, rows) in cards(&Config::default(), &Context::default()) {
         assert!(!rows.is_empty(), "{group:?} has no rows");
      }
   }

   #[test]
   fn a_card_keeps_its_row_count_whatever_the_config() {
      // What lets a refresh rewrite rows in place: a setting that stops
      // applying is hidden, never taken out.
      let mut other = Config::default();
      other.api_enabled = !other.api_enabled;
      other.ctx_limit = crate::config::CtxLimit::Fixed;
      other.days_by = crate::config::DaysBy::Time;
      other.ctx_dir_linked = !other.ctx_dir_linked;
      let ctx = Context {
         api_state: "polling".into(),
         sessions: true,
         ..Context::default()
      };
      let a: Vec<usize> = cards(&Config::default(), &Context::default())
         .iter()
         .map(|(_, r)| r.len())
         .collect();
      let b: Vec<usize> = cards(&other, &ctx).iter().map(|(_, r)| r.len()).collect();
      assert_eq!(a, b);
   }

   #[test]
   fn a_setting_that_does_not_apply_is_hidden_or_greyed_as_described() {
      let mut cfg = Config {
         ctx_limit: crate::config::CtxLimit::Learned,
         api_enabled: false,
         ..Config::default()
      };
      let rows = rows_of(&cfg);
      assert!(find(&rows, "ctx_limit_tokens").hidden);
      for key in ["api_stale_secs", "api_interval_secs", "api_budget_per_min"] {
         let r = find(&rows, key);
         assert!(r.dim && !r.hidden, "{key}");
      }

      cfg.ctx_limit = crate::config::CtxLimit::Fixed;
      cfg.api_enabled = true;
      let rows = rows_of(&cfg);
      assert!(!find(&rows, "ctx_limit_tokens").hidden);
      assert!(!find(&rows, "api_stale_secs").dim);
   }

   #[test]
   fn the_status_line_is_where_the_form_looks_for_it() {
      // `Form::rebuild` finds it by counting a card's settings and then its
      // extras; this is the same count, checked against what `cards` built.
      let ctx = Context {
         api_state: "polling".into(),
         ..Context::default()
      };
      let built = cards(&Config::default(), &ctx);
      let (g, (group, rows)) = built
         .iter()
         .enumerate()
         .find(|(_, (_, rows))| rows.iter().any(|r| r.label == "polling"))
         .expect("a card carries the status line");
      let settings = schema::ALL
         .iter()
         .filter(|s| s.group == Some(*group))
         .count();
      let at = EXTRAS
         .iter()
         .filter(|(e, _)| e == group)
         .position(|(_, x)| *x == Extra::ApiState)
         .unwrap();
      assert_eq!(rows[settings + at].control, NOTE);
      assert_eq!(rows[settings + at].label, "polling");
      assert!(g < built.len());
   }

   #[test]
   fn every_setting_gets_a_control_that_can_show_it() {
      for s in schema::ALL.iter().filter(|s| s.group.is_some()) {
         let c = control(s);
         let fits = match s.kind {
            Kind::Bool => c == SWITCH,
            Kind::Choice(o) => {
               (c == SEGMENTED) == (o.len() <= SEGMENTS_MAX) && (c == SEGMENTED || c == COMBO)
            }
            Kind::Int { .. } | Kind::Float { .. } => c == SLIDER || c == STEPPER,
            Kind::Text => c == TEXT,
         };
         assert!(fits, "{}: control {c}", s.key);
      }
   }

   #[test]
   fn every_number_stops_at_its_own_bounds() {
      // Walked against the schema's own numbers rather than against any
      // written down here: a bound copied into a test is a second place for
      // it to live, and the whole point of the schema is that there is one.
      for setting in schema::ALL.iter().filter(|s| s.group.is_some()) {
         let (min, max) = match setting.kind {
            Kind::Int { min, max, .. } => (min as f64, max as f64),
            Kind::Float { min, max, .. } => (min as f64, max as f64),
            _ => continue,
         };
         let read = |cfg: &Config| {
            schema::read(cfg, setting.key)
               .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|n| n as f64)))
               .unwrap_or(f64::NAN)
         };
         for (direction, bound) in [(2, max), (-2, min)] {
            let mut cfg = Config::default();
            for _ in 0..2_000 {
               if !apply(&mut cfg, setting.key, &Change::Step(direction)) {
                  break;
               }
            }
            let landed = read(&cfg);
            assert!(
               (landed - bound).abs() < 1e-6,
               "{}: stepping {} stopped at {landed}, not at {bound}",
               setting.key,
               if direction > 0 { "up" } else { "down" }
            );
            // A slider flung past either end lands on the end too.
            let mut cfg = Config::default();
            let far = bound + (bound - (min + max) / 2.0) * 4.0 + f64::from(direction);
            apply(
               &mut cfg,
               setting.key,
               &Change::Set {
                  value: far,
                  commit: true,
               },
            );
            let landed = read(&cfg);
            assert!(
               (landed - bound).abs() < 1e-6,
               "{}: a slider past the end landed at {landed}, not at {bound}",
               setting.key
            );
         }
      }
   }

   #[test]
   fn a_slider_lands_on_its_step() {
      let mut cfg = Config::default();
      assert!(apply(
         &mut cfg,
         "scale",
         &Change::Set {
            value: 1.23456,
            commit: false,
         }
      ));
      assert!((cfg.scale - 1.23).abs() < 1e-6, "{}", cfg.scale);
      assert!(apply(
         &mut cfg,
         "api_budget_per_min",
         &Change::Set {
            value: 7.6,
            commit: true,
         }
      ));
      assert_eq!(cfg.api_budget_per_min, 8);
   }

   #[test]
   fn a_double_click_puts_the_default_back() {
      let mut cfg = Config::default();
      let default = cfg.warn_pct;
      assert!(apply(&mut cfg, "warn_pct", &Change::Step(2)));
      assert_ne!(cfg.warn_pct, default);
      assert!(apply(&mut cfg, "warn_pct", &Change::Reset));
      assert_eq!(cfg.warn_pct, default);
      // Nothing to do when it is already there.
      assert!(!apply(&mut cfg, "warn_pct", &Change::Reset));
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
