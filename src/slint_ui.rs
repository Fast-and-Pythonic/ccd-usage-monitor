//! The pill, drawn by Slint from a `.slint` file read at runtime (A20).
//!
//! This module is the replacement for `ui.rs`: it owns the config, the
//! runtime, the tray and the windows, and it knows nothing about what any
//! figure means. Everything on screen arrives from `view::cells` and
//! `view::detail_lines` already worded, already coloured by role (A19).
//!
//! Three things here are not obvious and are written down where they happen:
//! the window handle must be asked for after the loop starts and never kept
//! (G18), the frame strip has to be followed by the size and not preceded by
//! it (G19), and a `show` undoes every one of those (G21).

#![cfg(windows)]

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use slint_interpreter::{ComponentHandle, ComponentInstance, Value};

use crate::config::Config;
use crate::geometry::Geometry;
use crate::runtime::Runtime;
use crate::state;
use crate::tray::{self, Tray};
use crate::view::{self, Ink, Paint, Tone};

/// The theme that ships in the binary. Also the one a broken theme falls back
/// to, and the file written to disk on first run so there is something to
/// edit.
const BUILT_IN: &str = include_str!("../themes/pill.slint");

/// The same quarter-second the file sources are watched on.
const TICK: Duration = Duration::from_millis(250);

/// Which version of the theme contract this build speaks.
///
/// Bumped whenever a struct sent to the theme changes shape. A *property* the
/// theme has never heard of costs nothing — it is simply not set — but a
/// struct is matched field by field, and one that no longer lines up is
/// refused whole, which leaves the pill blank with nothing said about it
/// (G22). So the version is checked before the theme is used at all.
const THEME_API: i64 = 2;

pub fn theme_path() -> std::path::PathBuf {
    crate::config::config_path().with_file_name("theme.slint")
}

/// What the widget is drawing with, and what went wrong getting there.
struct Theme {
    pill: slint_interpreter::ComponentDefinition,
    panel: slint_interpreter::ComponentDefinition,
    /// Empty when the theme on disk compiled. Otherwise the compiler's own
    /// words, kept for the tray menu: a theme that does not compile must not
    /// mean an application that does not start.
    trouble: Vec<String>,
    /// The built-in was used instead of the file on disk.
    fell_back: bool,
}

fn compile(text: &str, path: &std::path::Path) -> (Option<(slint_interpreter::ComponentDefinition, slint_interpreter::ComponentDefinition)>, Vec<String>) {
    let mut compiler = slint_interpreter::Compiler::default();
    compiler.set_style("fluent".into());
    let result = spin_on(compiler.build_from_source(text.to_string(), path.to_path_buf()));
    let trouble: Vec<String> = result
        .diagnostics()
        .filter(|d| d.level() == slint_interpreter::DiagnosticLevel::Error)
        .map(|d| d.to_string())
        .collect();
    match (result.component("Pill"), result.component("Panel")) {
        (Some(pill), Some(panel)) if trouble.is_empty() => (Some((pill, panel)), trouble),
        (Some(_), None) => {
            let mut t = trouble;
            t.push("the theme exports no component named `Panel`".into());
            (None, t)
        }
        (None, _) => {
            let mut t = trouble;
            t.push("the theme exports no component named `Pill`".into());
            (None, t)
        }
        _ => (None, trouble),
    }
}

/// The theme on disk if it compiles, the built-in one otherwise.
///
/// A theme is a file a person edits, so it will be broken sometimes. When it
/// is, the widget starts anyway on the copy in the binary and says what was
/// wrong in the tray — the alternative is an application that vanishes because
/// of a missing semicolon.
fn load_theme() -> Theme {
    let path = theme_path();
    if !path.exists() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&path, BUILT_IN);
    }
    if let Ok(text) = std::fs::read_to_string(&path) {
        let (built, mut trouble) = compile(&text, &path);
        if let Some((pill, panel)) = built {
            match speaks_our_version(&pill) {
                Ok(()) => return Theme { pill, panel, trouble, fell_back: false },
                Err(says) => trouble.push(says),
            }
        }
        let (built, _) = compile(BUILT_IN, &path);
        if let Some((pill, panel)) = built {
            return Theme { pill, panel, trouble, fell_back: true };
        }
    }
    let (built, trouble) =
        compile(BUILT_IN, std::path::Path::new("built-in/pill.slint"));
    let (pill, panel) = built.expect("the built-in theme must compile");
    Theme { pill, panel, trouble, fell_back: true }
}

/// Whether a compiled theme was written against the contract this build sends.
///
/// Checked by building one throwaway instance of it, which is the only way to
/// read a property's value: a `ComponentDefinition` lists what exists, not
/// what it is set to.
fn speaks_our_version(
    def: &slint_interpreter::ComponentDefinition,
) -> Result<(), String> {
    let Ok(instance) = def.create() else {
        return Err("the theme compiled but could not be built".into());
    };
    match instance.get_property("theme_api") {
        Ok(Value::Number(n)) if n as i64 == THEME_API => Ok(()),
        Ok(Value::Number(n)) => Err(format!(
            "the theme is written for contract {n}, this build sends {THEME_API}"
        )),
        _ => Err(format!(
            "the theme declares no `theme_api`; this build sends {THEME_API}"
        )),
    }
}

/// The palette the theme declared, read back so that a role can be turned into
/// a colour.
///
/// This is the division A19 asks for, made concrete: the theme owns what green
/// looks like, and this module owns which figures are green.
#[derive(Clone, Copy)]
struct Palette {
    green: slint::Color,
    amber: slint::Color,
    orange: slint::Color,
    red: slint::Color,
    ink: slint::Color,
    dim: slint::Color,
}

impl Palette {
    fn read(instance: &ComponentInstance) -> Palette {
        let pick = |name: &str, fallback: u32| -> slint::Color {
            match instance.get_property(name) {
                Ok(Value::Brush(b)) => b.color(),
                _ => slint::Color::from_argb_encoded(fallback),
            }
        };
        Palette {
            green: pick("palette_green", 0xff6fcf97),
            amber: pick("palette_amber", 0xffe6b65c),
            orange: pick("palette_orange", 0xffe8994c),
            red: pick("palette_red", 0xffe8695e),
            ink: pick("palette_ink", 0xffececf0),
            dim: pick("palette_dim", 0xff8c8e99),
        }
    }

    fn of(self, ink: Ink) -> slint::Color {
        match ink {
            Ink::Green => self.green,
            Ink::Amber => self.amber,
            Ink::Orange => self.orange,
            Ink::Red => self.red,
        }
    }

    /// A point on the ramp between two roles. The interpolation is here rather
    /// than in the theme because *where* a figure sits is the meaning; the two
    /// ends of the ramp are the appearance.
    fn tone(self, tone: Tone) -> slint::Color {
        let (a, b) = (self.of(tone.from), self.of(tone.to));
        a.mix(&b, 1.0 - tone.t.clamp(0.0, 1.0))
    }

    fn paint(self, paint: Paint) -> slint::Color {
        match paint {
            Paint::Plain => self.ink,
            Paint::Dim => self.dim,
            Paint::Tone(t) => self.tone(t),
        }
    }
}

/// Turn the cells and bars into the model the theme repeats over.
///
/// Grouped rather than flat, because a bar spans exactly its group's cells and
/// saying so in the model is easier to draw — and impossible to get wrong —
/// than having the theme work out where each group started.
fn groups_model(
    cfg: &Config,
    cells: &[view::Cell],
    bars: &[view::Bar],
    palette: Palette,
) -> slint::ModelRc<Value> {
    let last_group = cells.iter().map(|c| c.group).max().unwrap_or(0);
    let mut rows: Vec<Value> = Vec::new();
    for group in 0..=last_group {
        let members: Vec<Value> = cells
            .iter()
            .filter(|c| c.group == group)
            .map(|c| {
                Value::Struct(slint_interpreter::Struct::from_iter([
                    ("text".to_string(), Value::String(c.text.as_str().into())),
                    (
                        "name".to_string(),
                        Value::String(
                            c.field
                                .map(|f| view::field_name(f, cfg))
                                .unwrap_or_default()
                                .as_str()
                                .into(),
                        ),
                    ),
                    ("template".to_string(), Value::String(c.template.as_str().into())),
                    ("role".to_string(), Value::Number(role_code(c.role) as f64)),
                    ("ink".to_string(), Value::Brush(palette.paint(c.paint).into())),
                    ("gap".to_string(), Value::Number(gap_code(c.gap) as f64)),
                    ("side".to_string(), Value::Number(side_code(c.side) as f64)),
                ]))
            })
            .collect();
        if members.is_empty() {
            continue;
        }
        let bar = bars.iter().find(|b| b.group == group);
        rows.push(Value::Struct(slint_interpreter::Struct::from_iter([
            (
                "cells".to_string(),
                Value::Model(slint::ModelRc::new(slint::VecModel::from(members))),
            ),
            ("has_bar".to_string(), Value::Bool(bar.is_some())),
            (
                "bar_frac".to_string(),
                Value::Number(bar.map(|b| b.frac).unwrap_or(0.0) as f64),
            ),
            (
                "bar_ink".to_string(),
                Value::Brush(palette.paint(bar.map(|b| b.paint).unwrap_or(Paint::Dim)).into()),
            ),
            ("last".to_string(), Value::Bool(group == last_group)),
        ])));
    }
    slint::ModelRc::new(slint::VecModel::from(rows))
}

fn role_code(role: view::Role) -> i32 {
    match role {
        view::Role::Pct => 0,
        view::Role::Time => 1,
        view::Role::Small => 2,
    }
}

fn gap_code(gap: view::Gap) -> i32 {
    match gap {
        view::Gap::Cell => 0,
        view::Gap::Tight => 1,
        view::Gap::Rule => 2,
        view::Gap::Group => 3,
    }
}

fn side_code(side: view::Side) -> i32 {
    match side {
        view::Side::Centre => 0,
        view::Side::Right => 1,
        view::Side::Left => 2,
    }
}

/// Set a property only if the theme declared it.
///
/// The contract is deliberately one-sided: a theme written against an older
/// version of the widget keeps working with fewer values rather than refusing
/// to load. The reverse — a theme asking for something the widget does not
/// send — costs nothing either, since the property simply keeps its default.
struct Declared {
    names: Vec<String>,
    complained: RefCell<std::collections::BTreeSet<String>>,
}

impl Declared {
    fn of(def: &slint_interpreter::ComponentDefinition) -> Declared {
        Declared {
            names: def.properties().map(|(name, _)| name).collect(),
            complained: RefCell::new(std::collections::BTreeSet::new()),
        }
    }

    /// Errors are reported, once per property, rather than dropped.
    ///
    /// Swallowing them is how a theme whose structs had drifted produced a
    /// pill 36 pixels wide and not one word about why (G22). "Tolerant" has to
    /// mean "carries on and says so", not "carries on".
    fn set(&self, instance: &ComponentInstance, name: &str, value: Value) {
        if !self.names.iter().any(|n| n == name) {
            return;
        }
        if let Err(e) = instance.set_property(name, value)
            && self.complained.borrow_mut().insert(name.to_string())
        {
            eprintln!("ccd-usage-monitor: the theme would not take `{name}`: {e}");
        }
    }

    fn has(&self, name: &str) -> bool {
        self.names.iter().any(|n| n == name)
    }
}

fn number(instance: &ComponentInstance, name: &str, fallback: f64) -> f64 {
    match instance.get_property(name) {
        Ok(Value::Number(n)) if n.is_finite() && n > 0.0 => n,
        _ => fallback,
    }
}

struct App {
    cfg: Config,
    geom: Geometry,
    rt: Runtime,
    watch: crate::config::Watch,
    demo: bool,
    pill: ComponentInstance,
    declared: Declared,
    palette: Palette,
    frame: Option<crate::win::Frame>,
    /// The detail panel: a window of its own, so that opening it cannot move
    /// the pill (A25). It is never hidden, only parked off-screen (G21).
    panel: ComponentInstance,
    panel_declared: Declared,
    panel_frame: Option<crate::win::Frame>,
    panel_up: bool,
    panel_lines: usize,
    panel_sized: (f32, f32),
    /// The pointer is over the pill or over the panel under it — read from the
    /// cursor position rather than from the theme, because "is the pointer
    /// over the widget" has to keep the same answer while the pointer is
    /// still, and a toolkit's own pointer position goes stale exactly then
    /// (G16). Both windows count: a panel that closes when you move onto it to
    /// read it is a panel nobody can read.
    hovering: bool,
    /// The figure under the pointer, for the line that names it. Held while
    /// the pointer is anywhere over the pill and cleared when it leaves.
    hint: Option<String>,
    tray: Option<Tray>,
    /// Built the first time it is asked for and kept after that.
    settings: Option<crate::settings_ui::Form>,
    /// What was wrong with the theme on disk, for the form to show.
    trouble: String,
    visible: bool,
    /// Set by the tray, acted on by the next tick.
    ///
    /// The tray is drained while the app is borrowed, and opening the settings
    /// window needs that borrow back — building the form hands it a callback
    /// that borrows the same cell.
    wants_settings: bool,
    /// The size last pushed, so the window is not resized on every tick.
    sized: (f32, f32),
    /// What was last handed to the theme.
    ///
    /// Every push makes Slint lay the pill out again and repaint it, and the
    /// figures change about once a minute while the tick runs four times a
    /// second. Pushing regardless cost twelve times the CPU of the egui front
    /// end at rest, for four identical repaints a second.
    shown: Option<(Vec<view::Cell>, Vec<view::Bar>, bool)>,
}

/// Far enough off any real desktop that no arrangement of monitors reaches it.
const PARKED: (i32, i32) = (-30_000, -30_000);

impl App {
    /// Whether the detail panel should be open right now.
    fn detail_wanted(&self) -> bool {
        match self.cfg.detail_mode {
            crate::config::DetailMode::Hover => self.hovering,
            // Restored now that there is a settings window to be open: the
            // panel is a companion to it, so that what a setting changes can
            // be watched while it is changed.
            crate::config::DetailMode::WithSettings => {
                self.settings.as_ref().is_some_and(|f| f.is_open())
            }
            crate::config::DetailMode::Never => false,
        }
    }
}

pub fn run(demo: bool) -> Result<(), String> {
    crate::win::dark_menus();

    let mut cfg = Config::load();
    if demo {
        cfg.demo = true;
    }
    let geom = crate::geometry::load();
    let theme = load_theme();
    if theme.fell_back {
        eprintln!(
            "ccd-usage-monitor: {} was not used; drawing with the built-in theme.",
            theme_path().display()
        );
        for line in &theme.trouble {
            eprintln!("  {line}");
        }
        eprintln!("  delete that file to have a current one written in its place.");
    }

    let declared = Declared::of(&theme.pill);
    let panel_declared = Declared::of(&theme.panel);
    let callbacks: Vec<String> = theme.pill.callbacks().collect();
    let pill = theme.pill.create().map_err(|e| format!("cannot build the pill: {e}"))?;
    let panel = theme.panel.create().map_err(|e| format!("cannot build the panel: {e}"))?;
    let palette = Palette::read(&pill);

    let tray = Tray::build(cfg.is_ru());
    let app = Rc::new(RefCell::new(App {
        cfg,
        geom,
        rt: Runtime::new(),
        watch: crate::config::Watch::new(),
        demo,
        pill: pill.clone_strong(),
        declared,
        palette,
        frame: None,
        panel: panel.clone_strong(),
        panel_declared,
        panel_frame: None,
        panel_up: false,
        panel_lines: 0,
        panel_sized: (0.0, 0.0),
        hovering: false,
        hint: None,
        tray,
        settings: None,
        trouble: theme.trouble.join("
"),
        visible: true,
        wants_settings: false,
        sized: (0.0, 0.0),
        shown: None,
    }));

    // The drag is the system's own move loop, started from a press anywhere on
    // the pill (G12). Bound only if the theme asked for it.
    if callbacks.iter().any(|c| c == "pressed") {
        let app = app.clone();
        let _ = pill.set_callback("pressed", move |_| {
            // `start_drag` does not return until the move loop ends, so the
            // rectangle read straight after it is where the pill was put down.
            // The borrow is released first: the loop runs the timer, and a
            // `RefCell` held across it would panic.
            let frame = app.borrow().frame;
            if let Some(frame) = frame {
                frame.start_drag();
                let (x, y, _, _) = frame.rect();
                let mut a = app.borrow_mut();
                let s = a.pill.window().scale_factor();
                a.geom.position = Some([x as f32 / s, y as f32 / s]);
                crate::geometry::save(&a.geom);
            }
            Value::Void
        });
    }
    if callbacks.iter().any(|c| c == "menu") {
        let app = app.clone();
        let _ = pill.set_callback("menu", move |_| {
            open_settings(&app);
            Value::Void
        });
    }
    if callbacks.iter().any(|c| c == "entered") {
        let app = app.clone();
        let _ = pill.set_callback("entered", move |args| {
            if let Some(Value::String(name)) = args.first() {
                app.borrow_mut().hint = Some(name.to_string());
            }
            Value::Void
        });
    }

    pill.show().map_err(|e| format!("cannot show the window: {e}"))?;
    // Shown once so that it has a window at all; from here on it is moved, not
    // hidden (G21).
    panel.show().map_err(|e| format!("cannot show the panel: {e}"))?;

    let ticker = slint::Timer::default();
    {
        let app = app.clone();
        ticker.start(slint::TimerMode::Repeated, TICK, move || tick(&app));
    }

    slint::run_event_loop_until_quit().map_err(|e| e.to_string())
}

fn tick(app: &Rc<RefCell<App>>) {
    if std::mem::take(&mut app.borrow_mut().wants_settings) {
        open_settings(app);
    }
    let mut a = app.borrow_mut();

    // G18: asked for here, every time, rather than kept from before `show`.
    if a.frame.is_none() {
        let Some(frame) = crate::win::Frame::from(&a.pill.window().window_handle()) else {
            return;
        };
        frame.subclass();
        a.frame = Some(frame);
        apply_policy(&mut a);
        // `window.json` holds logical pixels, because that is what the window
        // manager deals in and what a person reading the file would expect.
        // `SetWindowPos` deals in device pixels, so the scale goes on here and
        // comes off again where the position is saved — getting this backwards
        // moves the pill by a quarter of the screen on a 125 % display and by
        // nothing at all on a 100 % one, which is how it survives testing.
        if let Some([x, y]) = a.geom.position {
            let s = a.pill.window().scale_factor();
            frame.place((x * s) as i32, (y * s) as i32);
        }
    }

    // The panel's window exists one turn of the loop after it was shown, the
    // same as the pill's (G18). It is marked and parked here, once — and from
    // then on it is moved rather than hidden, because a `show` would take back
    // the bit that stops it stealing the focus (G21).
    if a.panel_frame.is_none()
        && let Some(pf) = crate::win::Frame::from(&a.panel.window().window_handle())
    {
        pf.strip();
        pf.no_activate();
        pf.tool_window();
        pf.place(PARKED.0, PARKED.1);
        a.panel_frame = Some(pf);
    }

    // Sizes come from the layout the *previous* tick produced. An
    // `out property` bound to `preferred-width` is recomputed when the theme
    // lays itself out, which happens after this callback returns — so reading
    // it straight after setting the text that changed it gives the old number.
    // A quarter of a second of lag is invisible; a window sized to the
    // previous sentence is not.
    resize(&mut a);
    place_panel(&mut a);
    read_hover(&mut a);
    if a.settings.as_ref().is_some_and(|f| f.is_open()) {
        let state = view::api_state_text(&a.cfg, &a.rt, state::now());
        if let Some(form) = &a.settings {
            form.set_api_state(state);
        }
    }

    if let Some(cfg) = a.watch.poll() {
        let demo = a.demo;
        a.cfg = cfg;
        if demo {
            a.cfg.demo = true;
        }
        a.rt.invalidate_files();
    }

    pump_tray(&mut a);

    let now = state::now();
    let cfg = a.cfg.clone();
    a.rt.pump(&cfg, now);
    let snap = a.rt.snapshot(&cfg, now);
    let (snap, stale, ctx_stale) = a.rt.age_filter(&cfg, snap, now);

    let cells = view::cells(&cfg, &snap, stale, ctx_stale, now);
    let bars = view::bars(&cfg, &snap, ctx_stale, now);
    let fresh = (cells, bars, stale);
    if a.shown.as_ref() != Some(&fresh) {
        let (cells, bars, stale) = &fresh;
        let model = groups_model(&cfg, cells, bars, a.palette);
        let pill = a.pill.clone_strong();
        a.declared.set(&pill, "groups", Value::Model(model));
        a.declared.set(&pill, "stale", Value::Bool(*stale));
        a.declared.set(&pill, "scale", Value::Number(cfg.scale() as f64));
        a.declared
            .set(&pill, "alpha", Value::Number(cfg.opacity.clamp(0.3, 1.0) as f64));
        a.shown = Some(fresh);
    }

    show_panel(&mut a, &cfg, &snap, stale, now);
}

/// Put the detail panel under the pill, or take it away.
///
/// It flips above the pill near the bottom of the screen, which is all that is
/// left of A06 now that the panel is its own window: what used to be growing
/// the pill's window upwards — and moving it, which is what made the pill's
/// size depend on where the pointer was (G16) — is now one `SetWindowPos` on a
/// different window.
fn show_panel(
    a: &mut App,
    cfg: &Config,
    snap: &crate::model::Snapshot,
    stale: bool,
    now: i64,
) {
    let Some(pf) = a.panel_frame else { return };
    let mut lines = if a.visible && a.detail_wanted() {
        view::detail_lines(cfg, &a.rt, snap, stale, now)
    } else {
        Vec::new()
    };
    // With the panel open the name of the figure goes into it as the first
    // line: two surfaces answering one hover is one too many. With the panel
    // shut, the name is the whole of what the panel says.
    if let Some(hint) = &a.hint {
        let label = if cfg.is_ru() { "поле" } else { "field" };
        if lines.is_empty() && a.visible {
            // Even with the detail panel switched off entirely: "what is that
            // number" is a different question from "tell me everything", and
            // switching off the second should not take away the first.
            lines.push((label.to_string(), hint.clone()));
        } else if !lines.is_empty() {
            lines.insert(0, (label.to_string(), hint.clone()));
        }
    }
    if lines.is_empty() {
        if a.panel_up {
            a.panel_up = false;
            a.panel_lines = 0;
            pf.place(PARKED.0, PARKED.1);
        }
        return;
    }

    let panel = a.panel.clone_strong();
    let rows: Vec<Value> = lines
        .iter()
        .map(|(label, value)| {
            Value::Struct(slint_interpreter::Struct::from_iter([
                ("label".to_string(), Value::String(label.as_str().into())),
                ("value".to_string(), Value::String(value.as_str().into())),
            ]))
        })
        .collect();
    a.panel_declared.set(
        &panel,
        "lines",
        Value::Model(slint::ModelRc::new(slint::VecModel::from(rows))),
    );
    a.panel_declared.set(&panel, "scale", Value::Number(cfg.scale() as f64));
    a.panel_declared
        .set(&panel, "alpha", Value::Number(cfg.opacity.clamp(0.3, 1.0) as f64));

    a.panel_lines = lines.len();
    a.panel_up = true;
}

/// Is the pointer over the widget, and if it has just left, forget the figure
/// it was over.
///
/// Asked of the system on every tick rather than answered by the theme. The
/// difference matters: a toolkit reports a pointer position that is only
/// refreshed by a real mouse event, so a *still* pointer over a window that
/// has just moved reads as being somewhere it is not — which is how the hint
/// used to blink on and off several times a second (G16).
fn read_hover(a: &mut App) {
    let (cx, cy) = crate::win::cursor();
    let inside = |r: (i32, i32, i32, i32)| {
        cx >= r.0 && cx < r.0 + r.2 && cy >= r.1 && cy < r.1 + r.3
    };
    let over_pill = a.frame.is_some_and(|f| inside(f.rect()));
    let over_panel = a.panel_up && a.panel_frame.is_some_and(|f| inside(f.rect()));
    a.hovering = over_pill || over_panel;
    if !over_pill {
        // The name is held while the pointer is anywhere over the pill, gaps
        // between figures included, and only dropped when it leaves.
        a.hint = None;
    }
}

/// Size the panel to its lines and put it under the pill — or above it, when
/// the pill is too near the bottom of the screen.
///
/// Separate from `show_panel` because it works on the layout that tick
/// produced, which only exists on the next one.
fn place_panel(a: &mut App) {
    let (Some(frame), Some(pf)) = (a.frame, a.panel_frame) else { return };
    if !a.panel_up {
        if a.panel_lines == 0 && pf.rect().0 != PARKED.0 {
            pf.place(PARKED.0, PARKED.1);
        }
        return;
    }
    let panel = a.panel.clone_strong();
    let scale = a.pill.window().scale_factor();
    let (area_x, area_y, area_w, area_h) = frame.work_area();

    // Sized to its longest line rather than to the pill — the lines carry a
    // reset time, a verdict and a source name, and a pill five figures wide
    // has no room for that — but never wider than the screen it is on.
    let want_w = number(&panel, "wanted_width", 320.0) as f32;
    let want_h = number(&panel, "wanted_height", 120.0) as f32;
    let limit = (area_w as f32 / scale) - 20.0;
    let w = want_w.min(limit.max(120.0));
    if (w - a.panel_sized.0).abs() > 0.5 || (want_h - a.panel_sized.1).abs() > 0.5 {
        a.panel_sized = (w, want_h);
        panel.window().set_size(slint::LogicalSize::new(w, want_h));
        pf.strip();
    }

    let (x, y, _, pill_h) = frame.rect();
    let (_, _, panel_w, panel_h) = pf.rect();
    let gap = (6.0 * scale) as i32;
    let below = y + pill_h + gap;
    let above = y - gap - panel_h;
    let top = if below + panel_h <= area_y + area_h { below } else { above.max(area_y) };
    // A panel wider than the pill hangs off the right; near that edge of the
    // screen it slides left instead, ending flush with the pill's right edge.
    let left = x.min(area_x + area_w - panel_w).max(area_x);
    pf.place(left, top);
}

/// Show the settings window, building it the first time.
///
/// Everything a click in it can ask for comes back through one closure, which
/// is why the window is built here rather than up in `run`: the closure needs
/// the same `Rc` the rest of the program is holding.
fn open_settings(app: &Rc<RefCell<App>>) {
    if app.borrow().settings.is_none() {
        let (cfg, trouble) = {
            let a = app.borrow();
            (a.cfg.clone(), a.trouble.clone())
        };
        let handle = app.clone();
        let form = crate::settings_ui::Form::new(
            &cfg,
            &theme_path(),
            &trouble,
            move |key, change| changed(&handle, key, change),
        );
        match form {
            Ok(form) => app.borrow_mut().settings = Some(form),
            Err(e) => {
                eprintln!("ccd-usage-monitor: {e}");
                return;
            }
        }
        // Where it was left, if it has ever been moved.
        let a = app.borrow();
        if let Some(form) = &a.settings {
            if let Some([w, h]) = a.geom.settings_size {
                form.window().set_size(slint::LogicalSize::new(w, h));
            }
            if let Some([x, y]) = a.geom.settings_pos {
                form.window()
                    .set_position(slint::LogicalPosition::new(x, y));
            }
        }
    }
    {
        let mut a = app.borrow_mut();
        let (title, rows) = sessions_for(&mut a);
        let state = view::api_state_text(&a.cfg, &a.rt, state::now());
        if let Some(form) = &a.settings {
            form.set_sessions(&title, rows);
            form.set_api_state(state);
        }
    }
    if let Some(form) = app.borrow_mut().settings.as_mut() {
        form.show();
    }
}

/// At most this many conversations are offered. The tree can hold hundreds;
/// past a couple of dozen a list stops being a list and becomes a search
/// problem, and the answer to that is the id in `config.toml`.
const SESSION_LIST_MAX: usize = 25;

/// The conversations the context figure could be about, newest first.
///
/// Only when one is pinned: otherwise the widget picks for itself and a list
/// would be decoration. Names and working directories cost a file open each,
/// so `describe` is called here — when somebody has asked to see them — and
/// never on the polling path.
fn sessions_for(a: &mut App) -> (String, Vec<crate::settings_ui::SessionRow>) {
    use crate::config::SessionPick;
    if a.cfg.session_pick != SessionPick::Pinned {
        return (String::new(), Vec::new());
    }
    let now = state::now();
    let ru = a.cfg.is_ru();
    // Asking to see the list is the one moment a scan is worth its cost, and
    // the moment it is most likely not to have happened yet: the form can be
    // opened on the first tick, before the source has swept the tree once.
    if a.rt.session_list.is_empty() {
        a.rt.session_list = crate::session::list(&crate::session::live_sessions());
    }
    crate::session::describe(&mut a.rt.session_list);
    let rows = a
        .rt
        .session_list
        .iter()
        .take(SESSION_LIST_MAX)
        .map(|e| {
            let mut label = e.short_name();
            if let Some(p) = e.project() {
                label.push_str(" · ");
                label.push_str(&p);
            }
            label.push_str(" · ");
            label.push_str(&crate::pace::fmt_duration(now - e.modified));
            crate::settings_ui::SessionRow {
                id: e.id.as_str().into(),
                label: label.into(),
                chosen: e.id == a.cfg.session_id,
            }
        })
        .collect();
    (
        if ru { "Разговор".to_string() } else { "Conversation".to_string() },
        rows,
    )
}

/// One change made in the settings window.
///
/// The form has no opinion about when a file is written or what a setting
/// means; it reports what was clicked and this decides the rest.
fn changed(app: &Rc<RefCell<App>>, key: &str, change: crate::settings_ui::Change) {
    use crate::settings_ui::Change;
    match change {
        Change::Restart => return restart(),
        Change::OpenConfig => return open_config(),
        Change::OpenTheme => return open_theme(),
        Change::Closed { pos, size } => {
            let mut a = app.borrow_mut();
            a.geom.settings_pos = Some(pos);
            a.geom.settings_size = Some(size);
            crate::geometry::save(&a.geom);
            if let Some(form) = a.settings.as_mut() {
                form.mark_closed();
            }
            return;
        }
        _ => {}
    }
    let mut a = app.borrow_mut();
    if let Change::Pin(id) = &change {
        if a.cfg.session_id == *id {
            return;
        }
        a.cfg.session_id = id.clone();
        a.cfg.save();
        a.rt.session_scan = None;
        a.shown = None;
        let (title, rows) = sessions_for(&mut a);
        if let Some(form) = &a.settings {
            form.set_sessions(&title, rows);
        }
        return;
    }
    if !crate::settings_ui::apply(&mut a.cfg, key, &change) {
        return;
    }
    a.cfg.save();
    // The sources' remembered mtimes describe answers to the question the old
    // config asked (A23), and the pill has to be told to redraw: it only
    // pushes a model when the figures move, and a setting can change how the
    // same figures are written.
    a.rt.invalidate_files();
    a.shown = None;
    let cfg = a.cfg.clone();
    let (title, rows) = sessions_for(&mut a);
    if let Some(form) = &a.settings {
        form.refresh(&cfg);
        form.set_sessions(&title, rows);
    }
    let (visible, click_through, ru) = (a.visible, cfg.click_through, cfg.is_ru());
    if let Some(tray) = &mut a.tray {
        tray.sync_labels(visible, click_through, ru);
    }
}

/// Open the theme file in whatever edits `.slint` — a text editor, in
/// practice. The button beside it says which file that is.
fn open_theme() {
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", &theme_path().to_string_lossy()])
        .spawn();
}

/// Start again with the same arguments, and let this process end.
///
/// How a changed theme is picked up. Slint's stable API cannot replace the
/// element tree of a window that is already on screen, and rebuilding the
/// window inside the process would mean rebuilding every piece of state hung
/// off it — so the honest version is the cheap one, and the poller's budget
/// and the history log are read back off disk on the way up.
fn restart() {
    let Ok(exe) = std::env::current_exe() else { return };
    let args: Vec<String> = std::env::args().skip(1).collect();
    if std::process::Command::new(exe).args(args).spawn().is_ok() {
        let _ = slint::quit_event_loop();
    }
}

/// Open `config.toml` in whatever the system opens `.toml` with. Stands in for
/// the settings window until stage 7 builds one.
fn open_config() {
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", &crate::config::config_path().to_string_lossy()])
        .spawn();
}

/// Everything the window is, as opposed to what is drawn in it.
///
/// Re-applied after every `show`, because a `show` puts all of it back the way
/// winit wants it — the frame, the taskbar bit, the passthrough (G21).
fn apply_policy(a: &mut App) {
    let Some(frame) = a.frame else { return };
    frame.strip();
    frame.topmost();
    frame.tool_window();
    frame.click_through(a.cfg.click_through && a.tray.is_some());
    a.sized = (0.0, 0.0);
}

/// Follow the theme's own idea of how big it wants to be.
///
/// The size is stated *after* the frame strip and never before it: stripping
/// hands the client area the margins the frame used to occupy, so a size set
/// first is a size that grows by exactly a frame (G19).
fn resize(a: &mut App) {
    let pill = a.pill.clone_strong();
    if !a.declared.has("wanted_width") {
        return;
    }
    let w = number(&pill, "wanted_width", 320.0) as f32;
    let h = number(&pill, "wanted_height", 34.0) as f32;
    if (w - a.sized.0).abs() < 0.5 && (h - a.sized.1).abs() < 0.5 {
        return;
    }
    a.sized = (w, h);
    pill.window().set_size(slint::LogicalSize::new(w, h));
}

fn pump_tray(a: &mut App) {
    let commands = match &a.tray {
        Some(tray) => tray.poll(),
        None => return,
    };
    for command in commands {
        match command {
            tray::Command::ToggleVisible => {
                a.visible = !a.visible;
                if a.visible {
                    let _ = a.pill.show();
                    apply_policy(a);
                } else {
                    let _ = a.pill.hide();
                }
            }
            tray::Command::ToggleClickThrough => {
                a.cfg.click_through = !a.cfg.click_through;
                a.cfg.save();
                if let Some(frame) = a.frame {
                    frame.click_through(a.cfg.click_through && a.tray.is_some());
                }
            }
            tray::Command::ResetPosition => {
                a.geom.position = Some([60.0, 60.0]);
                crate::geometry::save(&a.geom);
                a.cfg.click_through = false;
                a.cfg.save();
                if let Some(frame) = a.frame {
                    frame.click_through(false);
                    frame.place(60, 60);
                }
            }
            tray::Command::OpenSettings => a.wants_settings = true,
            tray::Command::Restart => restart(),
            tray::Command::AboutSlint => {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "", "https://slint.dev"])
                    .spawn();
            }
            tray::Command::Quit => {
                let _ = slint::quit_event_loop();
            }
        }
    }
    let (visible, click_through, ru) = (a.visible, a.cfg.click_through, a.cfg.is_ru());
    if let Some(tray) = &mut a.tray {
        tray.sync_labels(visible, click_through, ru);
    }
}

/// Drive a future to completion on this thread.
///
/// The compiler API is async and this program has no runtime to hand it to —
/// nor does it want one for a call that reads a file and parses it. Spinning
/// is honest here: the future never yields to anything that could make
/// progress on another thread.
fn spin_on<F: std::future::Future>(mut fut: F) -> F::Output {
    use std::task::{Context, Poll, Waker};
    let mut cx = Context::from_waker(Waker::noop());
    let mut fut = unsafe { std::pin::Pin::new_unchecked(&mut fut) };
    loop {
        if let Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
            return v;
        }
    }
}
