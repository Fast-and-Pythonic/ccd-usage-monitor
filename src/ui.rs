//! The floating pill.
//!
//! Everything here is painted by hand: the window is frameless and transparent.
//! The only ordinary egui widgets are in the settings window (A11), which is a
//! window of its own and is what a right-click on the pill opens.

use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Align2, Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2};

use crate::geometry::Geometry;
use crate::config::{
    Config, CtxLimit, CtxView, DetailMode, PaceView, SessionPick, SourceKind,
};
use crate::model::Snapshot;
use crate::pace;
use crate::runtime::Runtime;
use crate::view::{self, Ink, Tone};
use crate::state;
use crate::session;
use crate::tray::{self, Tray};

/// Breathing room around the pill, so the drop shadow has somewhere to land.
///
/// Deliberately *not* scaled: the anchor and the panel offset are bookkept in
/// these units, and a margin that changed with the size setting would make the
/// window jump every time the user nudged the scale (see A06).
const MARGIN: f32 = 10.0;
/// Base geometry at scale 1.0. Everything here is multiplied by `Config::scale`.
const PAD_X: f32 = 13.0;
const CELL_GAP: f32 = 9.0;
const GROUP_GAP: f32 = 11.0;
const PANEL_GAP: f32 = 6.0;
const PANEL_PT: f32 = 12.5;
const ROW_H: f32 = 26.0;
const LINE_H: f32 = 19.0;
/// Starting width of the settings window, in logical pixels.
const SETTINGS_W: f32 = 340.0;
/// How many conversations the picker offers. There are eighty-odd transcripts
/// on a working machine and all but the newest few are finished business.
const SESSION_LIST_MAX: usize = 25;
/// The settings window is a normal opaque window, unlike the pill.
const SETTINGS_BG: Color32 = Color32::from_rgb(0x18, 0x18, 0x1B);
/// Detail panel: padding around the lines, and the width reserved for labels.
const DETAIL_PAD: f32 = 14.0;
const DETAIL_GUTTER: f32 = 34.0;
const FONT_NUM: &str = "ccd-usage-monitor-num";
const FONT_UI: &str = "ccd-usage-monitor-ui";

const INK: Color32 = Color32::from_rgb(0xEC, 0xEC, 0xF0);
const DIM: Color32 = Color32::from_rgb(0x8C, 0x8E, 0x99);
const GREEN: Color32 = Color32::from_rgb(0x6F, 0xCF, 0x97);
const AMBER: Color32 = Color32::from_rgb(0xE6, 0xB6, 0x5C);
/// Where the pace scale starts once spending is over the line. Amber was too
/// close to the green below it: a two-point overspend has to look like a
/// warning, not like a slightly tired "fine".
const ORANGE: Color32 = Color32::from_rgb(0xE8, 0x99, 0x4C);
const RED: Color32 = Color32::from_rgb(0xE8, 0x69, 0x5E);

/// Every size the pill is drawn with, resolved once per frame from the config.
/// Pulling them out of constants is what makes the size controls in the menu
/// possible at all.
#[derive(Clone)]
struct Metrics {
    scale: f32,
    pill_h: f32,
    pad_x: f32,
    cell_gap: f32,
    group_gap: f32,
    /// Corner radius for the pill, the panel and the hint, in pixels.
    corner: f32,
    track_h: f32,
    panel_gap: f32,
    /// Clear space above the figures, between them and the bars, and below the
    /// bars — all three settable.
    pad_top: f32,
    text_gap: f32,
    pad_bottom: f32,
    /// Height of the row of figures, as laid out.
    text_h: f32,
    /// The large percentages.
    f_pct: FontId,
    /// Countdown, day count, daily allowance.
    f_time: FontId,
    /// The `ctx` tag.
    f_small: FontId,
    /// Menu rows and detail values.
    f_panel: FontId,
    /// Detail labels.
    f_panel_dim: FontId,
    row_h: f32,
    line_h: f32,
}

impl Metrics {
    fn new(cfg: &Config) -> Metrics {
        let s = cfg.scale();
        let g = cfg.gaps_x();
        let track_h = cfg.track_height() * s;
        let pad_top = cfg.pad_top() * s;
        let pad_bottom = cfg.pad_bottom() * s;
        let text_gap = cfg.gap_text_bars() * s;
        // egui lays Segoe UI out at roughly 1.35 times the point size; close
        // enough to know how much room the row of figures wants.
        let text_h = cfg.font_pct().max(cfg.font_time()) * s * 1.35;
        // The pill is never shorter than its contents: thicker bars or bigger
        // paddings push it taller instead of hanging out of the bottom.
        let needed =
            pad_top + text_h + pad_bottom + if track_h > 0.0 { text_gap + track_h } else { 0.0 };
        Metrics {
            scale: s,
            pad_top,
            text_gap,
            pad_bottom,
            text_h,
            pill_h: (cfg.pill_height() * s).max(needed),
            corner: cfg.corner_radius() * s,
            pad_x: PAD_X * s * g,
            cell_gap: CELL_GAP * s * g,
            group_gap: GROUP_GAP * s * g,
            panel_gap: PANEL_GAP * s,
            track_h,
            f_pct: font_num(cfg.font_pct() * s),
            f_time: font_ui(cfg.font_time() * s),
            f_small: font_ui(cfg.font_time() * s * 0.86),
            f_panel: font_ui(PANEL_PT * s),
            f_panel_dim: font_ui((PANEL_PT - 0.5) * s),
            row_h: ROW_H * s,
            line_h: LINE_H * s,
        }
    }

    /// The room a cell asks for after itself. A rule needs a gap on each side,
    /// a little tighter than a plain cell gap; a group divider needs two full
    /// ones, which is why the next group starts `group_gap * 2` along.
    fn gap_width(&self, gap: view::Gap) -> f32 {
        match gap {
            view::Gap::Cell => self.cell_gap,
            view::Gap::Tight => 0.0,
            view::Gap::Rule => self.cell_gap * 1.5,
            view::Gap::Group => self.group_gap * 2.0,
        }
    }
}

pub fn run(demo: bool) -> eframe::Result {
    // Before the tray icon exists, though the switch only matters when a menu
    // is drawn: everything this process pops up should be dark, like the pill.
    #[cfg(windows)]
    crate::win::dark_menus();

    let cfg = Config::load();
    // After the config, never before: loading it is what moves the old
    // in-config rectangles into `window.json` (`Config::migrate_geometry`).
    let geom = crate::geometry::load();
    let start_h = cfg.pill_height() * cfg.scale() + MARGIN * 2.0;
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("ccd-usage-monitor")
        .with_inner_size(Vec2::new(420.0, start_h))
        .with_decorations(false)
        .with_transparent(true)
        .with_resizable(false)
        .with_taskbar(false)
        .with_always_on_top();
    if let Some([x, y]) = geom.position {
        viewport = viewport.with_position(Pos2::new(x, y));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "ccd-usage-monitor",
        options,
        Box::new(move |cc| {
            install_fonts(&cc.egui_ctx);
            let mut app = App::new(cfg, geom, demo);
            // The window exists by now, so this is the first moment its frame
            // can be taken apart. See `win::Frame::strip`.
            #[cfg(windows)]
            {
                app.frame = crate::win::Frame::from(cc);
                if let Some(f) = app.frame {
                    f.subclass();
                    f.strip();
                }
            }
            Ok(Box::new(app))
        }),
    )
}

/// Segoe UI keeps the pill looking like part of Windows. If it cannot be read
/// we simply stay on egui's bundled font.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    // egui's own bundled families stay behind ours as fallback: a glyph Segoe
    // UI lacks then still renders instead of turning into a box.
    let fallback: Vec<String> = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let mut installed = false;
    let add = |fonts: &mut egui::FontDefinitions, name: &str, file: &str| -> bool {
        let Ok(bytes) = std::fs::read(format!("C:/Windows/Fonts/{file}")) else {
            return false;
        };
        fonts.font_data.insert(
            name.to_owned(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        let chain: Vec<String> = std::iter::once(name.to_owned())
            .chain(fallback.iter().cloned())
            .collect();
        fonts.families.insert(FontFamily::Name(name.into()), chain);
        true
    };

    installed |= add(&mut fonts, FONT_NUM, "seguisb.ttf") || add(&mut fonts, FONT_NUM, "segoeui.ttf");
    installed |= add(&mut fonts, FONT_UI, "segoeui.ttf");

    if !installed {
        // No Segoe UI: point both families at egui's own fonts so the rest of
        // the code can keep asking for them by name.
        fonts
            .families
            .insert(FontFamily::Name(FONT_NUM.into()), fallback.clone());
        fonts
            .families
            .insert(FontFamily::Name(FONT_UI.into()), fallback);
    }
    ctx.set_fonts(fonts);
}

fn font_num(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FONT_NUM.into()))
}

fn font_ui(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FONT_UI.into()))
}

// ------------------------------------------------------------------- app ---

struct App {
    cfg: Config,
    /// Notices when `config.toml` was edited outside the program (A23).
    watch: crate::config::Watch,
    /// `--demo` was passed on the command line. Kept because a re-read config
    /// would otherwise switch the synthetic figures off under a session that
    /// asked for them.
    demo_arg: bool,
    /// Where the windows were left. Written to `window.json` rather than to
    /// the config, because a drag writes it and a file rewritten on every
    /// drag cannot hold a comment (see `geometry`).
    geom: Geometry,
    /// Everything the widget knows, and none of how it looks. Lives in
    /// `runtime` so a change of GUI does not touch it.
    rt: Runtime,
    /// The picker is open, which is the only time names are worth reading out
    /// of eighty-odd files.
    session_list_open: bool,
    /// The context figure is older than its source's threshold. Kept apart
    /// from `stale`, which is about the quota windows.
    ctx_stale: bool,
    /// The pointer is over the pill.
    hovering: bool,
    /// The field under the pointer, named for the next frame.
    hint: Option<Hint>,
    /// The settings window is open.
    settings_open: bool,
    was_dragging: bool,
    last_width: f32,
    last_height: f32,
    tray: Option<Tray>,
    window_visible: bool,
    /// The passthrough state actually pushed to the window, so the command is
    /// sent on change rather than every frame.
    applied_passthrough: Option<bool>,
    applied_visible: Option<bool>,
    source_list_open: bool,
    size_list_open: bool,
    pace_list_open: bool,
    /// When the settings window's position was last written to the config.
    settings_geom_saved: Option<Instant>,
    /// The settings window has drawn at least one frame, so it may be shown.
    settings_painted: bool,
    /// The window itself, for the frame surgery in `win`.
    #[cfg(windows)]
    frame: Option<crate::win::Frame>,
    /// Set when a viewport command was sent that makes winit rebuild the
    /// window style, which puts the drop shadow back.
    restrip: bool,
    /// Screen position of the pill's top-left corner, kept across panel
    /// open/close so the pill does not walk up and down the screen.
    anchor: Option<Pos2>,
    /// How far the window is currently shifted to fit a panel above it or to
    /// its left.
    applied_offset: Vec2,
    /// Set each frame: the figures on screen are older than their own source's
    /// `*_stale_secs` (A14).
    stale: bool,
}

impl App {
    fn new(mut cfg: Config, geom: Geometry, demo: bool) -> App {
        if demo {
            cfg.demo = true;
        }
        let tray = Tray::build(cfg.is_ru());
        App {
            cfg,
            watch: crate::config::Watch::new(),
            demo_arg: demo,
            geom,
            rt: Runtime::new(),
            session_list_open: false,
            ctx_stale: false,
            hovering: false,
            hint: None,
            settings_open: false,
            was_dragging: false,
            last_width: 0.0,
            last_height: 0.0,
            tray,
            window_visible: true,
            applied_passthrough: None,
            applied_visible: None,
            source_list_open: false,
            size_list_open: false,
            pace_list_open: false,
            settings_geom_saved: None,
            settings_painted: false,
            #[cfg(windows)]
            frame: None,
            restrip: false,
            anchor: None,
            applied_offset: Vec2::ZERO,
            stale: false,
        }
    }

    /// Tray menu clicks arrive on a global channel; they are handled in
    /// `logic` so they still work while the window is hidden.
    fn pump_tray(&mut self, ctx: &egui::Context) {
        // The tray reports clicks; what they mean is decided here, where the
        // state they touch lives. `poll` takes `&self`, so the borrow is over
        // before the loop starts mutating.
        let Some(commands) = self.tray.as_ref().map(Tray::poll) else {
            return;
        };
        for command in commands {
            match command {
                tray::Command::ToggleVisible => self.window_visible = !self.window_visible,
                tray::Command::OpenSettings => self.settings_open = true,
                tray::Command::ToggleClickThrough => {
                    self.cfg.click_through = !self.cfg.click_through;
                    self.cfg.save();
                }
                tray::Command::ResetPosition => {
                    self.window_visible = true;
                    self.geom.position = Some([60.0, 60.0]);
                    crate::geometry::save(&self.geom);
                    self.cfg.click_through = false;
                    self.cfg.save();
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(Pos2::new(
                        60.0, 60.0,
                    )));
                }
                tray::Command::Quit => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            }
        }

        let (visible, click_through, ru) =
            (self.window_visible, self.cfg.click_through, self.cfg.is_ru());
        if let Some(tray) = &mut self.tray {
            tray.sync_labels(visible, click_through, ru);
        }
    }

    /// Take up an edit made to `config.toml` by hand (A23).
    ///
    /// Everything the drawing code reads is read afresh each frame, so
    /// appearance and wording need no help here; what needs saying out loud is
    /// the state the sources keep. Their remembered mtimes describe answers to
    /// the question the old config asked — which files to read, which session
    /// to follow, how big a context window is — so they are dropped whole
    /// rather than reasoned about field by field. The poller is not restarted:
    /// `pump` hands it the new interval and budget on its next lap, and a
    /// setting must never cost a request (G14).
    fn reload_config(&mut self) {
        let Some(mut cfg) = self.watch.poll() else { return };
        if self.demo_arg {
            cfg.demo = true;
        }
        self.cfg = cfg;
        self.rt.invalidate_files();
    }

    /// Push window-level state that only changes on user action.
    ///
    /// Both commands here make winit recompute the window style from its own
    /// flags, which restores the undecorated-window shadow we removed. The
    /// stripping is therefore repeated on the frame after either of them.
    fn apply_window_state(&mut self, ctx: &egui::Context) {
        // Checked first: a command sent below is only applied once this frame
        // ends, so the repair belongs to the frame after it.
        if std::mem::take(&mut self.restrip) {
            #[cfg(windows)]
            if let Some(f) = self.frame {
                f.strip();
            }
        }

        let want = self.cfg.click_through && self.tray.is_some();
        if self.applied_passthrough != Some(want) {
            self.applied_passthrough = Some(want);
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(want));
            self.restrip = true;
        }
        if self.applied_visible != Some(self.window_visible) {
            self.applied_visible = Some(self.window_visible);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(self.window_visible));
            self.restrip = true;
        }
        if self.restrip {
            // Don't wait out the idle second before repairing the frame.
            ctx.request_repaint();
        }
    }
}


impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    /// Runs even while the window is hidden, which is what keeps the tray menu
    /// alive in that state.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.reload_config();
        self.rt.pump(&self.cfg, state::now());
        self.pump_tray(ctx);
        self.apply_window_state(ctx);
        // One repaint a second is enough for a countdown in minutes; a pointer
        // over the pill or an open settings window are the reasons to go faster.
        let busy = self.hovering || self.settings_open;
        ctx.request_repaint_after(Duration::from_millis(if busy { 100 } else { 1000 }));
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let now = state::now();
        let snap = self.rt.snapshot(&self.cfg, now);
        let (snap, stale, ctx_stale) = self.rt.age_filter(&self.cfg, snap, now);
        self.stale = stale;
        self.ctx_stale = ctx_stale;
        self.draw(ui, &snap, now);
        // The settings live in their own native window, opened from here while
        // it is wanted. Immediate viewports are rendered inside the parent's
        // pass, which is why this is called from `ui` and not from `logic`.
        let ctx = ui.ctx().clone();
        self.settings_window(&ctx, now);
    }
}

// --------------------------------------------------------------- drawing ---

/// What the pointer was over when the last frame ended, and where.
///
/// The window is measured and resized before a single figure is painted, so
/// the field under the pointer has to be known before the walk that would
/// discover it. One frame of lag is not visible; guessing the size is.
#[derive(Clone, PartialEq)]
struct Hint {
    text: String,
    centre_x: f32,
}

/// A `view::Cell` with the two things only the drawing layer can work out:
/// which font it is set in, and how wide it therefore is.
///
/// The composition itself — which figures, in what order, in whose group — is
/// decided in `view::cells`, where it can be tested without a window and read
/// by a front end that is not egui (A19).
struct Drawn {
    cell: view::Cell,
    font: FontId,
    color: Color32,
    width: f32,
}

fn measure(ctx: &egui::Context, text: &str, font: &FontId) -> f32 {
    ctx.fonts_mut(|f| f.layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE).size().x)
}

/// Resolve a role into a font and a paint into a colour, and measure the
/// result. A cell is at least as wide as its template, which is the widest
/// string it will ever hold.
fn lay_out(ctx: &egui::Context, cells: Vec<view::Cell>, m: &Metrics) -> Vec<Drawn> {
    cells
        .into_iter()
        .map(|cell| {
            let font = match cell.role {
                view::Role::Pct => m.f_pct.clone(),
                view::Role::Time => m.f_time.clone(),
                view::Role::Small => m.f_small.clone(),
            };
            let width = measure(ctx, &cell.text, &font).max(measure(ctx, &cell.template, &font));
            let color = paint(cell.paint);
            Drawn { cell, font, color, width }
        })
        .collect()
}

fn paint(p: view::Paint) -> Color32 {
    match p {
        view::Paint::Plain => INK,
        view::Paint::Dim => DIM,
        view::Paint::Tone(t) => tone_colour(t),
    }
}

impl App {
    /// Whether the detail panel under the pill is showing right now.
    fn detail_visible(&self) -> bool {
        match self.cfg.detail_mode {
            DetailMode::Hover => self.hovering,
            DetailMode::WithSettings => self.settings_open,
            DetailMode::Never => false,
        }
    }

    fn draw(&mut self, ui: &mut egui::Ui, snap: &Snapshot, now: i64) {
        let ctx = ui.ctx().clone();
        let m = Metrics::new(&self.cfg);
        let cells = lay_out(
            &ctx,
            view::cells(&self.cfg, snap, self.stale, self.ctx_stale, now),
            &m,
        );
        let tracks = view::bars(&self.cfg, snap, self.ctx_stale, now);

        // The same sum the painting loop walks: every gap but the one after
        // the last cell, each as wide as the cell before it asked for.
        let content_w: f32 = cells.iter().map(|c| c.width).sum::<f32>()
            + cells
                .iter()
                .rev()
                .skip(1)
                .map(|c| m.gap_width(c.cell.gap))
                .sum::<f32>();
        let pill_w = (content_w + m.pad_x * 2.0).max(180.0 * m.scale);

        // The detail panel is sized to its longest line rather than to the
        // pill: the lines carry a reset time, a verdict and a source name,
        // and a pill five figures wide has no room for that.
        // Last frame's hover, which is what makes it available this early.
        let hint = self.hint.clone();
        let pad = DETAIL_PAD * m.scale;
        let mut lines = if self.detail_visible() {
            view::detail_lines(&self.cfg, &self.rt, snap, self.stale, now)
        } else {
            Vec::new()
        };
        if let Some(h) = &hint {
            // With the panel open the name goes in it as the first line: two
            // surfaces answering one hover is one too many.
            if !lines.is_empty() {
                let label = if self.cfg.is_ru() { "поле" } else { "field" };
                lines.insert(0, (label.to_string(), h.text.clone()));
            }
        }
        // A box of its own, when there is no panel to put the name in.
        let hint_box = match &hint {
            Some(h) if lines.is_empty() => Some((
                h.clone(),
                pad * 2.0 + measure(&ctx, &h.text, &m.f_panel),
                pad + m.line_h,
            )),
            _ => None,
        };
        let (panel_w, panel_h) = if let Some((_, w, h)) = &hint_box {
            (w.max(pill_w), *h)
        } else if lines.is_empty() {
            (pill_w, 0.0)
        } else {
            let widest = lines
                .iter()
                .map(|(_, value)| measure(&ctx, value, &m.f_panel))
                .fold(0.0_f32, f32::max);
            let monitor_w = ctx
                .input(|i| i.viewport().monitor_size.map(|s| s.x))
                .unwrap_or(f32::INFINITY);
            let w = (pad * 2.0 + DETAIL_GUTTER * m.scale + widest)
                .max(pill_w)
                .min((monitor_w - MARGIN * 2.0).max(pill_w));
            (w, pad + lines.len() as f32 * m.line_h)
        };
        let panel_span = if panel_h > 0.0 { panel_h + m.panel_gap } else { 0.0 };

        // The pill keeps its place on screen; the panel opens downwards, or
        // upwards when the pill sits too close to the bottom edge. Growing the
        // window upwards means moving it, hence the anchor bookkeeping.
        //
        // The anchor is in screen coordinates and is only ever taken from
        // `outer_rect` — at startup and after a drag. Recomputing it every
        // frame would let it drift by one panel height each time the window
        // moves, since `outer_rect` lags a frame behind the command.
        let outer_min = ctx.input(|i| i.viewport().outer_rect.map(|r| r.min));
        if self.anchor.is_none() {
            self.anchor = outer_min.map(|o| o + Vec2::new(MARGIN, MARGIN) + self.applied_offset);
        }
        let monitor = ctx.input(|i| i.viewport().monitor_size);
        let anchor_y = self.anchor.map(|a| a.y);
        let room_below = match (monitor.map(|s| s.y), anchor_y) {
            (Some(h), Some(y)) => y + m.pill_h + panel_span + MARGIN <= h,
            _ => true,
        };
        let flip = panel_span > 0.0 && !room_below && anchor_y.is_some_and(|y| y - panel_span >= 0.0);

        // The same thought sideways: a panel wider than the pill hangs off the
        // right, and near that edge of the screen it would push the whole
        // window — pill included — left. Instead the window grows leftwards and
        // the pill keeps its place, the panel ending flush with the pill.
        let overhang = (panel_w - pill_w).max(0.0);
        let room_right = match (monitor.map(|s| s.x), self.anchor.map(|a| a.x)) {
            (Some(w), Some(x)) => x + panel_w + MARGIN <= w,
            _ => true,
        };
        let shift_x = if overhang > 0.0 && !room_right {
            overhang.min((self.anchor.map(|a| a.x).unwrap_or(0.0) - MARGIN).max(0.0))
        } else {
            0.0
        };
        let offset = Vec2::new(shift_x, if flip { panel_span } else { 0.0 });

        let want_w = pill_w.max(panel_w) + MARGIN * 2.0;
        let want_h = m.pill_h + MARGIN * 2.0 + panel_span;
        let resized = (want_w - self.last_width).abs() > 0.5 || (want_h - self.last_height).abs() > 0.5;
        if resized || (offset - self.applied_offset).length() > 0.5 {
            self.last_width = want_w;
            self.last_height = want_h;
            self.applied_offset = offset;
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(want_w, want_h)));
            if let Some(anchor) = self.anchor {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(
                    anchor - Vec2::new(MARGIN, MARGIN) - offset,
                ));
            }
        }

        let origin = ui.max_rect().min;
        let pill = Rect::from_min_size(
            origin + Vec2::new(MARGIN, MARGIN) + offset,
            Vec2::new(pill_w, m.pill_h),
        );
        // Half the height is the roundest a rectangle can be; past that the
        // ends stop being ends. Beyond that the number is the user's.
        let corner = m.corner.min(m.pill_h / 2.0);
        let radius = CornerRadius::same(corner as u8);
        let painter = ui.painter().clone();

        // Shadow: a few nested rounded rects are cheaper than a real blur and
        // read the same at this size.
        for i in 1..=4 {
            let k = i as f32;
            painter.rect_filled(
                pill.expand(k * 1.6).translate(Vec2::new(0.0, k * 0.5)),
                CornerRadius::same((corner + k * 1.6) as u8),
                Color32::from_black_alpha((14.0 / k) as u8),
            );
        }

        let alpha = (self.cfg.opacity.clamp(0.3, 1.0) * 255.0) as u8;
        painter.rect_filled(pill, radius, Color32::from_rgba_unmultiplied(19, 19, 22, alpha));
        painter.rect_stroke(
            pill,
            radius,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 22)),
            StrokeKind::Inside,
        );

        // The three paddings are exact and the block they wrap is centred in
        // whatever height is left over, so raising one of them moves what it is
        // supposed to move instead of being eaten by centring. A block that
        // needs the whole pill (thick bars, wide paddings) starts at the top —
        // the pill has already grown to fit it.
        let bars = if m.track_h > 0.0 { m.text_gap + m.track_h } else { 0.0 };
        let block_h = m.pad_top + m.text_h + bars + m.pad_bottom;
        let block_top = pill.top() + ((m.pill_h - block_h) * 0.5).max(0.0);
        let text_y = block_top + m.pad_top + m.text_h * 0.5;
        let track_y = if m.track_h > 0.0 {
            Some(block_top + m.pad_top + m.text_h + m.text_gap)
        } else {
            None
        };

        if self.stale {
            // A quiet marker beats a confident wrong number: the figures are
            // history, and the pill says so without needing to be hovered.
            painter.circle_filled(
                Pos2::new(pill.left() + 6.0 * m.scale, text_y),
                2.5 * m.scale,
                AMBER,
            );
        }

        let mut x = pill.left() + m.pad_x;
        let mut group_start = x;
        let mut group_index = 0usize;

        // Every part of the pill answers to a name, so that crossing a gap
        // between two figures does not blink the hint off and on again. A
        // cell owns half the space either side of it; the bars own the strip
        // under the figures.
        let mut regions: Vec<(Rect, String)> = Vec::new();
        let split_y = track_y.map_or(pill.bottom(), |y| y - m.text_gap * 0.5);
        let mut gap_before = m.pad_x;

        for (i, c) in cells.iter().enumerate() {
            let start = x;
            let gap_after = if i + 1 == cells.len() {
                m.pad_x
            } else {
                m.gap_width(c.cell.gap)
            };
            if let Some(field) = c.cell.field {
                regions.push((
                    Rect::from_min_max(
                        Pos2::new(start - gap_before * 0.5, pill.top()),
                        Pos2::new(start + c.width + gap_after * 0.5, split_y),
                    ),
                    view::field_name(field, &self.cfg),
                ));
            }
            gap_before = gap_after;
            let (text_x, anchor) = match c.cell.side {
                view::Side::Centre => (x + c.width / 2.0, Align2::CENTER_CENTER),
                view::Side::Right => (x + c.width, Align2::RIGHT_CENTER),
                view::Side::Left => (x, Align2::LEFT_CENTER),
            };
            painter.text(
                Pos2::new(text_x, text_y),
                anchor,
                &c.cell.text,
                c.font.clone(),
                c.color,
            );
            let end = x + c.width;
            x = end;

            if c.cell.gap == view::Gap::Group || i + 1 == cells.len() {
                // The bar spans exactly the cells of its group, and what it
                // says is decided in `view::bars` — this only knows where the
                // group started and where it ended.
                let bar = tracks.iter().find(|b| b.group == group_index);
                if let (Some(y), Some(bar)) = (track_y, bar) {
                    App::draw_bar(&painter, group_start, end, y, bar.frac, paint(bar.paint), &m);
                    regions.push((
                        Rect::from_min_max(
                            Pos2::new(group_start, split_y),
                            Pos2::new(end, pill.bottom()),
                        ),
                        view::field_name(bar.field, &self.cfg),
                    ));
                }
                group_index += 1;
                // Two gaps, not one: the divider sits between them, so the next
                // group's first cell starts `group_gap * 2` past this one. With
                // a single gap the track was drawn one gap left of its group.
                group_start = end + m.gap_width(view::Gap::Group);
            }

            if i + 1 == cells.len() {
                break;
            }
            let rule = |x: f32, inset: f32, alpha: u8| {
                painter.line_segment(
                    [
                        Pos2::new(x, pill.top() + inset),
                        Pos2::new(x, pill.bottom() - inset),
                    ],
                    Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, alpha)),
                );
            };
            match c.cell.gap {
                view::Gap::Group => {
                    x += m.group_gap;
                    rule(x, m.pill_h * 0.26, 26);
                    x += m.group_gap;
                }
                view::Gap::Rule => {
                    // Shorter and quieter than the one between groups: it joins
                    // two halves of one figure, it does not separate two things.
                    let gap = m.gap_width(view::Gap::Rule) * 0.5;
                    x += gap;
                    rule(x, m.pill_h * 0.34, 20);
                    x += gap;
                }
                view::Gap::Cell | view::Gap::Tight => x += m.gap_width(c.cell.gap),
            }
        }

        // Interaction covers the pill only; the panel below is passive.
        let response = ui.interact(pill, ui.id().with("pill"), egui::Sense::click_and_drag());
        if response.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            self.was_dragging = true;
        }
        if self.was_dragging && !response.dragged() {
            self.was_dragging = false;
            if let Some(rect) = ctx.input(|i| i.viewport().outer_rect) {
                self.anchor = Some(rect.min + Vec2::new(MARGIN, MARGIN) + offset);
                // Saved position is the window's, panel offsets removed, so a
                // restart puts the pill back where the user left it.
                self.geom.position = Some([rect.min.x + offset.x, rect.min.y + offset.y]);
                crate::geometry::save(&self.geom);
            }
        }
        if response.clicked_by(egui::PointerButton::Secondary) {
            self.settings_open = !self.settings_open;
        }

        // Named for the next frame; a change is worth a repaint, since the
        // pointer may well have stopped moving by now.
        let found = match ctx.pointer_hover_pos() {
            None => None,
            Some(p) => match regions.iter().find(|(r, _)| r.contains(p)) {
                Some((r, name)) => Some(Hint {
                    text: (*name).to_owned(),
                    centre_x: r.center().x,
                }),
                // Nothing under the pointer, but the pointer is still over the
                // widget: keep the name. Opening the hint moves the window up,
                // and the pointer's position inside it is only refreshed by a
                // real mouse event — so a still pointer reads as if it were in
                // the empty band above the pill, which would close the hint,
                // which would move the window back, which would open it again.
                // The panel avoids the same loop with `has_pointer`.
                None => self.hint.clone(),
            },
        };
        if found != self.hint {
            self.hint = found;
            ctx.request_repaint();
        }

        let hovered = response.hovered() || ctx.input(|i| i.pointer.has_pointer());
        let was_hovering = self.hovering;
        self.hovering = hovered;
        // Only worth a request if looking at the pill is about to show
        // something more than the pill already says.
        if hovered && !was_hovering && self.cfg.detail_mode == DetailMode::Hover {
            self.rt.request_api_refresh(&self.cfg, false);
        }

        if !lines.is_empty() {
            let panel_top = if flip {
                pill.top() - m.panel_gap - panel_h
            } else {
                pill.bottom() + m.panel_gap
            };
            // The same corner as the pill above it: two boxes a few pixels
            // apart, rounded differently, read as an accident.
            let corner = CornerRadius::same(m.corner as u8);
            let panel = Rect::from_min_size(
                Pos2::new(pill.left() - shift_x, panel_top),
                Vec2::new(panel_w, panel_h),
            );
            painter.rect_filled(panel, corner, Color32::from_rgba_unmultiplied(19, 19, 22, alpha));
            painter.rect_stroke(
                panel,
                corner,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 22)),
                StrokeKind::Inside,
            );
            self.draw_details(&painter, panel, &lines, &m);
        } else if let Some((h, w, h_box)) = &hint_box {
            let top = if flip {
                pill.top() - m.panel_gap - h_box
            } else {
                pill.bottom() + m.panel_gap
            };
            // Under the field it names, and never past the edge of the window,
            // which is only as wide as the pill and the panel need.
            let band_left = pill.left() - shift_x;
            let left = (h.centre_x - w * 0.5).clamp(band_left, band_left + panel_w - w);
            let rect = Rect::from_min_size(Pos2::new(left, top), Vec2::new(*w, *h_box));
            let corner = CornerRadius::same(m.corner as u8);
            painter.rect_filled(rect, corner, Color32::from_rgba_unmultiplied(19, 19, 22, alpha));
            painter.rect_stroke(
                rect,
                corner,
                Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 22)),
                StrokeKind::Inside,
            );
            painter.text(
                Pos2::new(rect.left() + pad, rect.center().y),
                Align2::LEFT_CENTER,
                &h.text,
                m.f_panel.clone(),
                INK,
            );
        }
    }

    /// One hairline bar: the track, and the part of it that is filled.
    ///
    /// Takes the fraction already worked out rather than a percentage and a
    /// direction: which way a figure reads is a question about meaning, and
    /// `view::bars` has answered it — including that the context window fills
    /// where the quotas drain.
    fn draw_bar(
        painter: &egui::Painter,
        left: f32,
        right: f32,
        y: f32,
        frac: f32,
        colour: Color32,
        m: &Metrics,
    ) {
        if m.track_h <= 0.0 {
            return;
        }
        let rect = Rect::from_min_size(Pos2::new(left, y), Vec2::new((right - left).max(1.0), m.track_h));
        let r = CornerRadius::same((m.track_h / 2.0).ceil() as u8);
        painter.rect_filled(rect, r, Color32::from_rgba_unmultiplied(255, 255, 255, 24));

        if frac > 0.0 {
            let fill = Rect::from_min_size(rect.min, Vec2::new(rect.width() * frac, m.track_h));
            painter.rect_filled(fill, r, colour);
        }
    }

    fn draw_details(
        &self,
        painter: &egui::Painter,
        panel: Rect,
        lines: &[(String, String)],
        m: &Metrics,
    ) {
        let pad = DETAIL_PAD * m.scale;
        let x = panel.left() + pad;
        let mut y = panel.top() + pad;
        for (label, value) in lines {
            painter.text(
                Pos2::new(x, y),
                Align2::LEFT_CENTER,
                label,
                m.f_panel_dim.clone(),
                DIM,
            );
            painter.text(
                Pos2::new(x + DETAIL_GUTTER * m.scale, y),
                Align2::LEFT_CENTER,
                value,
                m.f_panel.clone(),
                INK,
            );
            y += m.line_h;
        }
    }

    /// The settings, in a window of their own.
    ///
    /// They used to hang under the pill, which made the indicator grow a panel
    /// taller than itself and put four stepper buttons into a strip 300 pixels
    /// wide. A real window is detached, resizable, and moved and closed by the
    /// system like any other.
    fn settings_window(&mut self, ctx: &egui::Context, now: i64) {
        if !self.settings_open {
            return;
        }
        let ru = self.cfg.is_ru();
        let m = Metrics::new(&self.cfg);
        let size = self.geom.settings_size.unwrap_or([SETTINGS_W, 470.0]);
        let mut builder = egui::ViewportBuilder::default()
            .with_title(if ru {
                "ccd-usage-monitor — настройки"
            } else {
                "ccd-usage-monitor settings"
            })
            .with_inner_size(Vec2::new(size[0], size[1]))
            .with_min_inner_size(Vec2::new(280.0, 180.0))
            .with_resizable(true)
            // Born hidden: an unpainted window is white, and a white flash is
            // what a dark widget must not do. Shown below, once it has a frame.
            .with_visible(self.settings_painted);
        if let Some([x, y]) = self.geom.settings_pos {
            builder = builder.with_position(Pos2::new(x, y));
        }

        let mut close_requested = false;
        let mut geometry: Option<([f32; 2], [f32; 2])> = None;

        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("ccd-usage-monitor-settings"),
            builder,
            |ui, _class| {
                // The app clears every viewport to transparent for the pill's
                // sake, so this window paints its own background.
                ui.painter().rect_filled(ui.max_rect(), 0.0, SETTINGS_BG);
                let child = ui.ctx().clone();
                // egui wraps tooltips at 600 points by default, which in a
                // window this narrow means the text runs off the edge and is
                // clipped mid-sentence.
                let tip_w = (260.0 * m.scale)
                    .min(ui.max_rect().width() - 24.0)
                    .max(160.0);
                if (child.global_style().spacing.tooltip_width - tip_w).abs() > 0.5 {
                    child.global_style_mut(|s| s.spacing.tooltip_width = tip_w);
                }
                close_requested = child.input(|i| i.viewport().close_requested());
                geometry = child.input(|i| {
                    let v = i.viewport();
                    match (v.outer_rect, v.inner_rect) {
                        (Some(o), Some(inner)) => {
                            Some(([o.min.x, o.min.y], [inner.width(), inner.height()]))
                        }
                        _ => None,
                    }
                });

                // The scroll area spans the full width so its bar sits against
                // the window edge; the padding lives inside the content.
                let pad = 12.0 * m.scale;
                let inner = ui.max_rect().shrink2(Vec2::new(0.0, pad));
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                    self.settings_body(ui, &m, now);
                });
                if !self.settings_painted {
                    self.settings_painted = true;
                    child.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                }
            },
        );

        if let Some((pos, size)) = geometry {
            let moved = self.geom.settings_pos.is_none_or(|p| {
                (p[0] - pos[0]).abs() > 1.0 || (p[1] - pos[1]).abs() > 1.0
            });
            let resized = self.geom.settings_size.is_none_or(|s| {
                (s[0] - size[0]).abs() > 1.0 || (s[1] - size[1]).abs() > 1.0
            });
            if moved || resized {
                self.geom.settings_pos = Some(pos);
                self.geom.settings_size = Some(size);
                // Dragging a window produces a position every frame; the file
                // does not need to hear about all of them.
                let due = self
                    .settings_geom_saved
                    .is_none_or(|t| t.elapsed() > Duration::from_secs(2));
                if due {
                    self.settings_geom_saved = Some(Instant::now());
                    crate::geometry::save(&self.geom);
                }
            }
        }

        if close_requested {
            self.settings_open = false;
            self.settings_painted = false;
            self.cfg.save();
        }
    }

    fn settings_body(&mut self, ui: &mut egui::Ui, m: &Metrics, now: i64) {
        let ru = self.cfg.is_ru();
        ui.spacing_mut().item_spacing = Vec2::new(6.0 * m.scale, 6.0 * m.scale);
        let indent = 14.0 * m.scale;
        let pad = 12.0 * m.scale;

        // Used by rows in several sections below.
        let on_off = |v: bool| match (v, ru) {
            (true, true) => "вкл",
            (false, true) => "выкл",
            (true, false) => "on",
            (false, false) => "off",
        };
        egui::ScrollArea::vertical().show(ui, |ui| {
            let width = (ui.available_width() - pad * 2.0).max(80.0);
            ui.add_space(-ui.spacing().item_spacing.y);
            ui.horizontal(|ui| {
            ui.add_space(pad);
            ui.vertical(|ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            // ----------------------------------------------------- sources ---
            let head = setting_row(
                ui,
                m,
                if ru { "Источники".to_string() } else { "Sources".to_string() },
                false,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Откуда виджет берёт цифры. Каждый источник включается отдельно, \
                     а то, что они дают, сводится по полям."
                } else {
                    "Where the figures come from. Each source switches on separately, \
                     and what they report is merged field by field."
                },
            ));
            chevron(ui, head.rect, self.source_list_open, m);
            if head.clicked() {
                self.source_list_open = !self.source_list_open;
            }
            if self.source_list_open {
                for kind in SourceKind::ALL {
                    let on = self.cfg.enabled(kind);
                    let label = format!(
                        "{}: {}",
                        kind.label(ru),
                        match (on, ru) {
                            (true, true) => "вкл",
                            (false, true) => "выкл",
                            (true, false) => "on",
                            (false, false) => "off",
                        }
                    );
                    let hint = match (kind, ru) {
                        (SourceKind::Bridge, true) => {
                            "Данные от команды statusLine самого Claude Code. Идут, \
                             только пока открыта сессия в терминале; зато размер \
                             окна контекста называет только он."
                        }
                        (SourceKind::Bridge, false) => {
                            "Figures from Claude Code's own status-line command. Only \
                             while a terminal session is running — but it is the only \
                             source that states the size of the context window."
                        }
                        (SourceKind::Api, true) => {
                            "Прямой запрос к серверу Anthropic токеном Claude Code. \
                             Единственный источник, который знает время сброса."
                        }
                        (SourceKind::Api, false) => {
                            "A direct request to Anthropic with Claude Code's token. \
                             The only source that knows the reset times."
                        }
                        (SourceKind::Desktop, true) => {
                            "Файл, который ведёт само приложение Claude. Ничего не \
                             требует, но пишется раз в ~15 минут и без времени сброса."
                        }
                        (SourceKind::Desktop, false) => {
                            "The history file the Claude desktop app keeps. Asks for \
                             nothing, but is written every ~15 min and carries no \
                             reset times."
                        }
                        (SourceKind::Session, true) => {
                            "Читает файлы разговоров, которые Claude Code пишет сам, \
                             в ~/.claude/projects. Работает и в приложении, где \
                             строка состояния не запускается. Число токенов точное, \
                             а вот размер окна приходится выводить по модели."
                        }
                        (SourceKind::Session, false) => {
                            "Reads the conversation files Claude Code writes for \
                             itself under ~/.claude/projects. Works in the desktop \
                             app, where the status line never runs. The token count \
                             is exact; the window size has to be inferred from the \
                             model."
                        }
                    };
                    if setting_row(ui, m, label, on, indent)
                        .on_hover_text(tip(m, hint))
                        .clicked()
                    {
                        self.cfg.set_enabled(kind, !on);
                        self.cfg.save();
                        // Each source keeps what it gathered — age_filter
                        // decides what is still worth showing. Re-reading the
                        // files and waking the poller makes the switch instant.
                        self.rt.state_mtime = None;
                        self.rt.desktop_mtime = None;
                        self.rt.last_state_check = Instant::now() - Duration::from_secs(1);
                        // Only switching the endpoint itself back on is worth a
                        // request; the other two are files, re-read for free
                        // just above.
                        if kind == SourceKind::Api && !on {
                            self.rt.request_api_refresh(&self.cfg, true);
                        }
                    }

                    // Each source's own settings sit under its switch, which is
                    // the only place they mean anything.
                    let deep = indent * 2.0;
                    let d = stepper_row(
                        ui,
                        m,
                        if ru { "Устаревает через" } else { "Stale after" },
                        pace::fmt_duration(self.cfg.stale_secs(kind)),
                        60.0,
                        300.0,
                        deep,
                        if ru {
                            "После этого срока цифры этого источника сереют, а на \
                             пилюле загорается янтарная точка."
                        } else {
                            "Past this the figures from this source go grey and the \
                             pill shows an amber dot."
                        },
                    );
                    if d != 0.0 {
                        let next = self.cfg.stale_secs(kind) + d as i64;
                        self.cfg.set_stale_secs(kind, next);
                        self.cfg.save();
                    }

                    if kind == SourceKind::Session {
                        // Which conversation the figure is about. A cycling row
                        // rather than a dropdown, like every other short list
                        // of modes in this window.
                        if setting_row(
                            ui,
                            m,
                            format!(
                                "{}: {}",
                                if ru { "Сессия" } else { "Session" },
                                self.cfg.session_pick.label(ru)
                            ),
                            false,
                            deep,
                        )
                        .on_hover_text(tip(
                            m,
                            if ru {
                                "Запущенная — из тех, что Claude Code сейчас считает \
                                 живыми; если таких нет, поле молчит, а не подставляет \
                                 старую. Свежайшая — тот разговор, в который писали \
                                 последним, где бы он ни был. Выбранная — та, что \
                                 указана ниже."
                            } else {
                                "Running — one Claude Code currently lists as alive; \
                                 with none the field goes quiet rather than falling \
                                 back to an old one. Freshest — whichever transcript \
                                 was written to last, wherever it is. Pinned — the one \
                                 chosen below."
                            },
                        ))
                        .clicked()
                        {
                            self.cfg.session_pick = self.cfg.session_pick.next();
                            self.cfg.save();
                            self.rt.session_scan = None;
                        }

                        if self.cfg.session_pick == SessionPick::Pinned {
                            let head = setting_row(
                                ui,
                                m,
                                if ru { "Разговор".to_string() } else { "Conversation".to_string() },
                                false,
                                deep,
                            )
                            .on_hover_text(tip(
                                m,
                                if ru {
                                    "Запоминается id разговора. Если его файл исчезнет, \
                                     поле замолчит — виджет не станет молча показывать \
                                     соседний."
                                } else {
                                    "The conversation's id is remembered. If its file \
                                     goes, the field goes quiet — the widget will not \
                                     quietly show a neighbour instead."
                                },
                            ));
                            chevron(ui, head.rect, self.session_list_open, m);
                            if head.clicked() {
                                self.session_list_open = !self.session_list_open;
                                if self.session_list_open {
                                    // Names and directories cost a file open
                                    // each, so they are read when a list is
                                    // asked for and never on the polling path.
                                    session::describe(&mut self.rt.session_list);
                                }
                            }
                            if self.session_list_open {
                                let mut chosen: Option<String> = None;
                                for e in self.rt.session_list.iter().take(SESSION_LIST_MAX) {
                                    let mut label = String::new();
                                    if e.live {
                                        label.push_str("● ");
                                    }
                                    label.push_str(&e.short_name());
                                    if let Some(p) = e.project() {
                                        label.push_str(" · ");
                                        label.push_str(&p);
                                    }
                                    label.push_str(" · ");
                                    label.push_str(&pace::fmt_duration(now - e.modified));
                                    let on = e.id == self.cfg.session_id;
                                    if setting_row(ui, m, label, on, deep + indent)
                                        .on_hover_text(tip(m, &e.id))
                                        .clicked()
                                    {
                                        chosen = Some(e.id.clone());
                                    }
                                }
                                if let Some(id) = chosen {
                                    self.cfg.session_id = id;
                                    self.cfg.save();
                                    self.rt.session_scan = None;
                                }
                            }
                        }

                        let d = stepper_row(
                            ui,
                            m,
                            if ru { "Пересматривать список" } else { "Rescan the list" },
                            pace::fmt_duration(self.cfg.session_scan_secs() as i64),
                            10.0,
                            60.0,
                            deep,
                            if ru {
                                "Как часто виджет заново перечитывает папку разговоров. \
                                 Между просмотрами он смотрит только на один выбранный \
                                 файл, так что это почти ничего не стоит."
                            } else {
                                "How often the widget enumerates the transcripts again. \
                                 Between scans it stats one chosen file, so this costs \
                                 next to nothing either way."
                            },
                        );
                        if d != 0.0 {
                            let next = (self.cfg.session_scan_secs() as f32 + d).clamp(5.0, 600.0);
                            self.cfg.session_scan_secs = next as i64;
                            self.cfg.save();
                        }

                        // The denominator: the only guess in the whole figure,
                        // so it is a setting and it says which way it was got.
                        if setting_row(
                            ui,
                            m,
                            format!(
                                "{}: {}",
                                if ru { "Размер окна" } else { "Window size" },
                                self.cfg.ctx_limit.label(ru)
                            ),
                            false,
                            deep,
                        )
                        .on_hover_text(tip(
                            m,
                            if ru {
                                "На что делится число токенов. У моделей пятой серии \
                                 окно — миллион, у прежних — двести тысяч, и ошибка \
                                 здесь это разница между 9% и 44% для одного и того же \
                                 разговора. statusLine называет размер сам: запустите \
                                 Claude Code в терминале хотя бы раз, и он запомнится."
                            } else {
                                "What the token count is divided by. The 5-series has a \
                                 million-token window and everything before it two \
                                 hundred thousand, so getting this wrong is the \
                                 difference between 9% and 44% for one conversation. \
                                 The status line states the size itself: run Claude \
                                 Code in a terminal once and it is remembered."
                            },
                        ))
                        .clicked()
                        {
                            self.cfg.ctx_limit = self.cfg.ctx_limit.next();
                            self.cfg.save();
                            self.rt.session_mtime = None;
                        }

                        if self.cfg.ctx_limit == CtxLimit::Fixed {
                            let d = stepper_row(
                                ui,
                                m,
                                if ru { "Окно" } else { "Window" },
                                pace::fmt_tokens(self.cfg.ctx_limit_tokens()),
                                8_000.0,
                                200_000.0,
                                deep,
                                if ru {
                                    "Знаменатель вручную. Нужен там, где модель \
                                     держат в меньшем окне переменной среды: на диске \
                                     это никак не отражается, и угадать неоткуда."
                                } else {
                                    "The denominator by hand. Needed when a model is \
                                     held to a smaller window by an environment \
                                     variable: nothing on disk says so."
                                },
                            );
                            if d != 0.0 {
                                let next = (self.cfg.ctx_limit_tokens() as f64 + d as f64)
                                    .clamp(8_192.0, 1_000_000.0);
                                self.cfg.ctx_limit_tokens = next as u64;
                                self.cfg.save();
                                self.rt.session_mtime = None;
                            }
                        }

                        // What the source is actually reading, in one row. The
                        // question this whole section exists to answer.
                        let state = match &self.rt.session.context {
                            Some(c) => {
                                let mut s = String::new();
                                if let Some(model) = &c.model {
                                    s.push_str(model.trim_start_matches("claude-"));
                                    s.push_str(" · ");
                                }
                                if let (Some(t), Some(l)) = (c.used_tokens, c.limit_tokens) {
                                    s.push_str(&format!(
                                        "{} {} {}",
                                        pace::fmt_tokens(t),
                                        if ru { "из" } else { "of" },
                                        pace::fmt_tokens(l)
                                    ));
                                }
                                if !c.limit_stated {
                                    s.push_str(if ru { " (оценка)" } else { " (guessed)" });
                                }
                                s
                            }
                            None => match self.rt.session_chosen.is_some() {
                                true => if ru { "читаю…" } else { "reading…" }.to_string(),
                                false => if ru { "сессия не выбрана" } else { "no session picked" }
                                    .to_string(),
                            },
                        };
                        if setting_row(
                            ui,
                            m,
                            format!("{}: {state}", if ru { "Сейчас" } else { "Now" }),
                            false,
                            deep,
                        )
                        .on_hover_text(tip(
                            m,
                            if ru {
                                "Что виджет читает прямо сейчас. «(оценка)» значит, что \
                                 размер окна никто не называл и он взят по модели. \
                                 Нажатие пересматривает список и перечитывает файл."
                            } else {
                                "What the widget is reading right now. \"(guessed)\" \
                                 means nobody stated the window size and it came from \
                                 the model. Clicking rescans and re-reads."
                            },
                        ))
                        .clicked()
                        {
                            self.rt.session_scan = None;
                            self.rt.session_mtime = None;
                            self.rt.last_state_check = Instant::now() - Duration::from_secs(1);
                        }
                    }

                    if kind == SourceKind::Api {
                        let d = stepper_row(
                            ui,
                            m,
                            if ru { "Опрашивать каждые" } else { "Poll every" },
                            pace::fmt_duration(self.cfg.api_interval() as i64),
                            10.0,
                            60.0,
                            deep,
                            if ru {
                                "Как часто виджет сам ходит за цифрами в фоне. Реже — \
                                 меньше запросов, но цифры дольше остаются старыми."
                            } else {
                                "How often the widget fetches in the background. \
                                 Rarer means fewer requests but older figures."
                            },
                        );
                        if d != 0.0 {
                            let next = (self.cfg.api_interval() as f32 + d).clamp(30.0, 3600.0);
                            self.cfg.api_interval_secs = next as u64;
                            self.cfg.save();
                        }

                        let d = stepper_row(
                            ui,
                            m,
                            if ru { "Запросов в минуту" } else { "Requests per min" },
                            format!("{}", self.cfg.api_budget()),
                            1.0,
                            5.0,
                            deep,
                            if ru {
                                "Потолок, который виджет держит сам, чтобы сервер не \
                                 ответил отказом. Сверх него запрос не отменяется, а \
                                 откладывается на несколько секунд."
                            } else {
                                "A cap the widget keeps for itself so the server never \
                                 has to refuse. Past it a request is delayed by a few \
                                 seconds, not dropped."
                            },
                        );
                        if d != 0.0 {
                            let next = (self.cfg.api_budget() as f32 + d).clamp(1.0, 30.0);
                            self.cfg.api_budget_per_min = next as u32;
                            self.cfg.save();
                        }

                        if self.cfg.api_enabled {
                            let label = if self.rt.api_polling {
                                if ru { "Опрос…".to_string() } else { "Polling…".to_string() }
                            } else if ru {
                                "Обновить сейчас".to_string()
                            } else {
                                "Refresh now".to_string()
                            };
                            if setting_row(ui, m, label, false, deep)
                                .on_hover_text(tip(
                                    m,
                                    if ru {
                                        "Запросить цифры немедленно. Если бюджет \
                                         исчерпан, запрос уйдёт сам через несколько \
                                         секунд."
                                    } else {
                                        "Fetch the figures now. If the budget is spent \
                                         the request goes out by itself a few seconds \
                                         later."
                                    },
                                ))
                                .clicked()
                            {
                                // Re-reading the local files costs nothing and
                                // sometimes answers the question without the
                                // network being involved at all.
                                self.rt.invalidate_files();
                                self.rt.request_api_refresh(&self.cfg, true);
                            }
                        }
                    }
                }

                let label = format!(
                    "{}: {}",
                    if ru { "Демо-данные" } else { "Demo figures" },
                    match (self.cfg.demo, ru) {
                        (true, true) => "вкл",
                        (false, true) => "выкл",
                        (true, false) => "on",
                        (false, false) => "off",
                    }
                );
                if setting_row(ui, m, label, self.cfg.demo, indent)
                    .on_hover_text(tip(
                        m,
                        if ru {
                            "Показывать выдуманные цифры — чтобы настраивать вид, не \
                             дожидаясь настоящих."
                        } else {
                            "Show made-up figures, for working on the look without \
                             waiting for real ones."
                        },
                    ))
                    .clicked()
                {
                    self.cfg.demo = !self.cfg.demo;
                    self.cfg.save();
                }
            }

            // ------------------------------------------------- percentages ---
            if setting_row(
                ui,
                m,
                format!(
                    "{}: {}",
                    if ru { "Проценты" } else { "Percentages" },
                    self.cfg.pct_mode.label(ru)
                ),
                false,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Что показывают большие проценты и полоски под ними: сколько                      квоты уже потрачено или сколько её осталось. Цвет в обоих                      случаях считается по расходу, поэтому «10% осталось» красное."
                } else {
                    "What the big percentages and the bars under them show: how                      much of the quota is spent, or how much is left. The colour                      is keyed on spending either way, so 10% left shows red."
                },
            ))
            .clicked()
            {
                self.cfg.pct_mode = self.cfg.pct_mode.other();
                self.cfg.save();
            }

            // ----------------------------------------------------- context ---
            if setting_row(
                ui,
                m,
                format!(
                    "{}: {}",
                    if ru { "Контекст" } else { "Context" },
                    self.cfg.ctx_view.label(ru)
                ),
                false,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Что стоит в группе ctx: доля окна, число токенов или оба \
                     подряд. В варианте «заполнено» токены не зависят от \
                     угаданного размера окна, поэтому их стоит держать рядом с \
                     процентом, пока нет доверия к знаменателю."
                } else {
                    "What the ctx group shows: the share of the window, the token \
                     count, or both. In \"spent\" mode the count owes nothing to a \
                     guessed window size, so it is worth keeping beside the \
                     percentage until the denominator is trusted."
                },
            ))
            .clicked()
            {
                let all = CtxView::ALL;
                let i = all.iter().position(|v| *v == self.cfg.ctx_view).unwrap_or(0);
                self.cfg.ctx_view = all[(i + 1) % all.len()];
                self.cfg.save();
            }

            // ------------------------------------------- context direction ---
            if setting_row(
                ui,
                m,
                format!(
                    "{}: {}",
                    if ru { "Направление ctx" } else { "Ctx direction" },
                    match (self.cfg.ctx_dir_linked, ru) {
                        (true, true) => "вместе",
                        (true, false) => "together",
                        (false, true) => "раздельно",
                        (false, false) => "split",
                    }
                ),
                false,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "«Вместе» — процент и токены заполнения ctx читаются в одну \
                     сторону. «Раздельно» — у каждого своя настройка ниже: \
                     например, проценты остатка при токенах заполнения."
                } else {
                    "\"Together\" — the ctx percentage and token count read the \
                     same direction. \"Split\" gives each its own setting below: \
                     e.g. a remaining percentage alongside a spent token count."
                },
            ))
            .clicked()
            {
                self.cfg.ctx_dir_linked = !self.cfg.ctx_dir_linked;
                if self.cfg.ctx_dir_linked {
                    self.cfg.ctx_tokens_dir = self.cfg.ctx_pct_dir;
                }
                self.cfg.save();
            }

            let ctx_dir_hint = if ru {
                "Заполненность окна растёт к пределу, а не убывает от него, как \
                 пятичасовая и недельная квоты — поэтому у ctx свой выбор, а не \
                 общий «Проценты» выше."
            } else {
                "The window fills toward its limit instead of draining from it \
                 like the 5-hour and weekly quotas — so ctx gets its own choice \
                 instead of the \"Percentages\" setting above."
            };
            if self.cfg.ctx_dir_linked {
                if setting_row(
                    ui,
                    m,
                    format!(
                        "{}: {}",
                        if ru { "Ctx заполнение" } else { "Ctx fill" },
                        self.cfg.ctx_pct_dir.label(ru)
                    ),
                    false,
                    indent,
                )
                .on_hover_text(tip(m, ctx_dir_hint))
                .clicked()
                {
                    self.cfg.ctx_pct_dir = self.cfg.ctx_pct_dir.other();
                    self.cfg.ctx_tokens_dir = self.cfg.ctx_pct_dir;
                    self.cfg.save();
                }
            } else {
                if setting_row(
                    ui,
                    m,
                    format!(
                        "{}: {}",
                        if ru { "Ctx проценты" } else { "Ctx percent" },
                        self.cfg.ctx_pct_dir.label(ru)
                    ),
                    false,
                    indent,
                )
                .on_hover_text(tip(m, ctx_dir_hint))
                .clicked()
                {
                    self.cfg.ctx_pct_dir = self.cfg.ctx_pct_dir.other();
                    self.cfg.save();
                }
                if setting_row(
                    ui,
                    m,
                    format!(
                        "{}: {}",
                        if ru { "Ctx токены" } else { "Ctx tokens" },
                        self.cfg.ctx_tokens_dir.label(ru)
                    ),
                    false,
                    indent,
                )
                .on_hover_text(tip(m, ctx_dir_hint))
                .clicked()
                {
                    self.cfg.ctx_tokens_dir = self.cfg.ctx_tokens_dir.other();
                    self.cfg.save();
                }
            }

            // -------------------------------------------------------- pace ---
            let pace_head = setting_row(
                ui,
                m,
                if ru { "Темп".to_string() } else { "Pace".to_string() },
                false,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Насколько быстро уходит квота: обогнал ли расход часы и \
                     успеет ли то, что осталось, дожить до сброса."
                } else {
                    "How fast the quota is going: whether spending has run ahead \
                     of the clock, and whether what is left reaches the reset."
                },
            ));
            chevron(ui, pace_head.rect, self.pace_list_open, m);
            if pace_head.clicked() {
                self.pace_list_open = !self.pace_list_open;
            }
            if self.pace_list_open {
                let head = if ru { "Показывать" } else { "Show" };
                let hint = if ru {
                    "Что стоит рядом с каждым лимитом на пилюле. Пункты — на сколько \
                     расход обогнал часы; кратность — во сколько раз придётся \
                     замедлиться. Пункт списка выглядит так же, как будет на пилюле."
                } else {
                    "What stands beside each limit on the pill. Points are how far \
                     spending has run ahead of the clock; the ratio is how many times \
                     slower the rest has to go. Each entry looks the way the pill will."
                };
                let mut chosen = self.cfg.pace_view;
                ui.scope(|ui| {
                    // The one dropdown in this window, dressed as the rows
                    // around it rather than as egui's default widget.
                    let radius = CornerRadius::same((6.0 * m.scale).round() as u8);
                    let quiet = Color32::from_rgba_unmultiplied(255, 255, 255, 12);
                    let lit = Color32::from_rgba_unmultiplied(255, 255, 255, 22);
                    {
                        let w = &mut ui.visuals_mut().widgets;
                        for (state, fill) in [
                            (&mut w.inactive, quiet),
                            (&mut w.hovered, lit),
                            (&mut w.active, lit),
                            (&mut w.open, lit),
                        ] {
                            state.weak_bg_fill = fill;
                            state.bg_fill = fill;
                            state.corner_radius = radius;
                            state.fg_stroke.color = INK;
                        }
                    }
                    ui.spacing_mut().interact_size.y = m.row_h;
                    ui.horizontal(|ui| {
                        ui.add_space(indent);
                        let width = (ui.available_width() - 2.0).max(60.0);
                        let response = egui::ComboBox::from_id_salt("pace_view")
                            .width(width)
                            // The list is a window of its own, so the row
                            // styling above does not reach it.
                            .popup_style(egui::style::StyleModifier::from(
                                move |style: &mut egui::Style| {
                                let v = &mut style.visuals;
                                v.window_fill = Color32::from_rgb(0x1B, 0x1B, 0x1F);
                                v.window_stroke = Stroke::new(
                                    1.0,
                                    Color32::from_rgba_unmultiplied(255, 255, 255, 26),
                                );
                                v.window_corner_radius = radius;
                                // egui's default selection is a bright blue,
                                // which belongs to no other part of this app.
                                v.selection.bg_fill =
                                    Color32::from_rgba_unmultiplied(0x6F, 0xCF, 0x97, 60);
                                v.selection.stroke.color = INK;
                                v.widgets.hovered.weak_bg_fill = lit;
                                v.widgets.active.weak_bg_fill = lit;
                                v.widgets.inactive.weak_bg_fill = Color32::TRANSPARENT;
                                },
                            ))
                            .selected_text(
                                egui::RichText::new(format!(
                                    "{head}: {}",
                                    self.cfg.pace_view.label(ru)
                                ))
                                .font(m.f_panel.clone())
                                .color(INK),
                            )
                            .show_ui(ui, |ui| {
                                for view in PaceView::ALL {
                                    let text = egui::RichText::new(view.label(ru))
                                        .font(m.f_panel.clone())
                                        .color(INK);
                                    if ui
                                        .selectable_label(self.cfg.pace_view == view, text)
                                        .clicked()
                                    {
                                        chosen = view;
                                    }
                                }
                            });
                        response.response.on_hover_text(tip(m, hint));
                    });
                });
                if chosen != self.cfg.pace_view {
                    self.cfg.pace_view = chosen;
                    self.cfg.save();
                }

                if setting_row(
                    ui,
                    m,
                    format!(
                        "{}: {}",
                        if ru { "Цвет баров" } else { "Bar colour" },
                        self.cfg.bar_colour.label(ru)
                    ),
                    false,
                    indent,
                )
                .on_hover_text(tip(
                    m,
                    if ru {
                        "По расходу — краснеет к концу квоты. По темпу — краснеет, \
                         когда тратишь быстрее часов, даже если потрачено мало. \
                         Короткий красный бар значит: квота есть, времени нет."
                    } else {
                        "By fill it reddens as the window runs out. By pace it \
                         reddens when spending outruns the clock, however little is \
                         gone. A short red bar means quota left and no time to use it."
                    },
                ))
                .clicked()
                {
                    self.cfg.bar_colour = self.cfg.bar_colour.other();
                    self.cfg.save();
                }

                if setting_row(
                    ui,
                    m,
                    format!(
                        "{}: {}",
                        if ru { "Скорость и прогноз" } else { "Rate and forecast" },
                        on_off(self.cfg.pace_rate)
                    ),
                    self.cfg.pace_rate,
                    indent,
                )
                .on_hover_text(tip(
                    m,
                    if ru {
                        "В панели: сколько процентов в час уходит сейчас и когда при \
                         таком темпе упрёмся в потолок. Считается по журналу замеров, \
                         который виджет ведёт сам."
                    } else {
                        "In the panel: how many percent an hour are going now, and \
                         when that runs into the ceiling. Read from the log of \
                         readings the widget keeps."
                    },
                ))
                .clicked()
                {
                    self.cfg.pace_rate = !self.cfg.pace_rate;
                    self.cfg.save();
                }
            }

            // ------------------------------------------------------ detail ---
            if setting_row(
                ui,
                m,
                format!(
                    "{}: {}",
                    if ru { "Панель подробностей" } else { "Detail panel" },
                    self.cfg.detail_mode.label(ru)
                ),
                false,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Когда показывать панель под пилюлей: пока открыто это окно, \
                     при наведении мыши на пилюлю, или никогда."
                } else {
                    "When the panel under the pill appears: while this window is \
                     open, on hovering the pill, or never."
                },
            ))
            .clicked()
            {
                self.cfg.detail_mode = self.cfg.detail_mode.next();
                self.cfg.save();
            }

            if setting_row(
                ui,
                m,
                format!(
                    "{}: {}",
                    if ru { "Строка на источник" } else { "Line per source" },
                    match (self.cfg.detail_all_sources, ru) {
                        (true, true) => "вкл",
                        (false, true) => "выкл",
                        (true, false) => "on",
                        (false, false) => "off",
                    }
                ),
                self.cfg.detail_all_sources,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Добавляет в панель по строке на каждый включённый источник — \
                     то же, что печатает `ccd-usage-monitor --sources`."
                } else {
                    "Adds a line per enabled source to the panel — the same thing \
                     `ccd-usage-monitor --sources` prints."
                },
            ))
            .clicked()
            {
                self.cfg.detail_all_sources = !self.cfg.detail_all_sources;
                self.cfg.save();
            }

            // ---------------------------------------------------- day count ---
            if setting_row(
                ui,
                m,
                format!(
                    "{}: {}",
                    if ru { "Считать сегодня" } else { "Count today" },
                    on_off(self.cfg.count_today)
                ),
                self.cfg.count_today,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Входит ли текущий день в счётчик дней и в дневную норму. \
                     Сегодня обычно уже наполовину прожито, поэтому по умолчанию \
                     не считается."
                } else {
                    "Whether the current day counts towards the day figure and \
                     the daily allowance. Today is usually half spent by the \
                     time you look, so it does not by default."
                },
            ))
            .clicked()
            {
                self.cfg.count_today = !self.cfg.count_today;
                self.cfg.save();
            }

            if setting_row(
                ui,
                m,
                format!(
                    "{}: {}",
                    if ru { "Считать день сброса" } else { "Count reset day" },
                    on_off(self.cfg.count_reset_day)
                ),
                self.cfg.count_reset_day,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Входит ли день, в который квота обновляется. Он стоит только \
                     тех часов, что до сброса: если сброс в 7 утра, этот день \
                     лучше выключить."
                } else {
                    "Whether the day the quota resets counts. It is worth only \
                     the hours before the reset: with a reset at 07:00 that day \
                     is better left out."
                },
            ))
            .clicked()
            {
                self.cfg.count_reset_day = !self.cfg.count_reset_day;
                self.cfg.save();
            }

            // ------------------------------------------------------- sizes ---
            let sizes = setting_row(
                ui,
                m,
                if ru { "Размеры".to_string() } else { "Sizes".to_string() },
                false,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Размеры и отступы пилюли. Всё, что ниже, умножается на общий \
                     масштаб."
                } else {
                    "The pill's sizes and paddings. Everything below is multiplied \
                     by the overall scale."
                },
            ));
            chevron(ui, sizes.rect, self.size_list_open, m);
            if sizes.clicked() {
                self.size_list_open = !self.size_list_open;
            }
            if self.size_list_open {
                let d = stepper_row(
                    ui,
                    m,
                    if ru { "Общий масштаб" } else { "Overall scale" },
                    format!("{:.0}%", self.cfg.scale() * 100.0),
                    0.01,
                    0.05,
                    indent,
                    if ru {
                        "Множитель для всего сразу: шрифтов, высоты, отступов. Первое, что стоит \
                     крутить на другом мониторе."
                    } else {
                        "One multiplier for the fonts, the height and the paddings alike. The knob \
                     to reach for on a different monitor."
                    },
                );
                if d != 0.0 {
                    self.cfg.scale = self.cfg.scale() + d;
                    self.cfg.save();
                }

                let d = stepper_row(
                    ui,
                    m,
                    if ru { "Кегль процентов" } else { "Percent font" },
                    format!("{:.1}", self.cfg.font_pct()),
                    0.5,
                    2.0,
                    indent,
                    if ru {
                        "Кегль крупных процентов на пилюле."
                    } else {
                        "Point size of the large percentages on the pill."
                    },
                );
                if d != 0.0 {
                    self.cfg.font_pct = self.cfg.font_pct() + d;
                    self.cfg.save();
                }

                let d = stepper_row(
                    ui,
                    m,
                    if ru { "Кегль времени" } else { "Time font" },
                    format!("{:.1}", self.cfg.font_time()),
                    0.5,
                    2.0,
                    indent,
                    if ru {
                        "Кегль обратного отсчёта, времени сброса, числа дней и дневной нормы."
                    } else {
                        "Point size of the countdown, the reset clock, the day count and the \
                     daily allowance."
                    },
                );
                if d != 0.0 {
                    self.cfg.font_time = self.cfg.font_time() + d;
                    self.cfg.save();
                }

                let d = stepper_row(
                    ui,
                    m,
                    if ru { "Высота пилюли" } else { "Pill height" },
                    format!("{:.0}", self.cfg.pill_height()),
                    1.0,
                    4.0,
                    indent,
                    if ru {
                        "Высота самой пилюли. Если содержимому мало места, она вырастет сама."
                    } else {
                        "Height of the pill itself. It grows on its own when the contents need \
                     more room."
                    },
                );
                if d != 0.0 {
                    self.cfg.pill_height = self.cfg.pill_height() + d;
                    self.cfg.save();
                }

                let d = stepper_row(
                    ui,
                    m,
                    if ru { "Толщина полосок" } else { "Bar thickness" },
                    format!("{:.1}", self.cfg.track_height()),
                    0.5,
                    2.0,
                    indent,
                    if ru {
                        "Толщина полосок расхода под цифрами. 0 убирает их совсем."
                    } else {
                        "Thickness of the usage bars under the figures. Zero hides them."
                    },
                );
                if d != 0.0 {
                    self.cfg.track_height = self.cfg.track_height() + d;
                    self.cfg.save();
                }

                let d = stepper_row(
                    ui,
                    m,
                    if ru { "Над цифрами" } else { "Above figures" },
                    format!("{:.1}", self.cfg.pad_top()),
                    0.5,
                    2.0,
                    indent,
                    if ru {
                        "Отступ между верхом пилюли и строкой цифр."
                    } else {
                        "Space between the top of the pill and the row of figures."
                    },
                );
                if d != 0.0 {
                    self.cfg.pad_top = self.cfg.pad_top() + d;
                    self.cfg.save();
                }

                let d = stepper_row(
                    ui,
                    m,
                    if ru { "Цифры и бары" } else { "Figures to bars" },
                    format!("{:.1}", self.cfg.gap_text_bars()),
                    0.5,
                    2.0,
                    indent,
                    if ru {
                        "Просвет между цифрами и полосками расхода."
                    } else {
                        "The gap between the figures and the usage bars."
                    },
                );
                if d != 0.0 {
                    self.cfg.gap_text_bars = self.cfg.gap_text_bars() + d;
                    self.cfg.save();
                }

                let d = stepper_row(
                    ui,
                    m,
                    if ru { "Под барами" } else { "Below bars" },
                    format!("{:.1}", self.cfg.pad_bottom()),
                    0.5,
                    2.0,
                    indent,
                    if ru {
                        "Отступ между полосками и низом пилюли."
                    } else {
                        "Space between the bars and the bottom of the pill."
                    },
                );
                if d != 0.0 {
                    self.cfg.pad_bottom = self.cfg.pad_bottom() + d;
                    self.cfg.save();
                }

                let d = stepper_row(
                    ui,
                    m,
                    if ru { "Отступы по горизонтали" } else { "Horizontal gaps" },
                    format!("{:.0}%", self.cfg.gaps_x() * 100.0),
                    0.05,
                    0.2,
                    indent,
                    if ru {
                        "Все горизонтальные просветы сразу: между числами, вокруг \
                         разделителей групп и от края пилюли."
                    } else {
                        "Every horizontal gap at once: between the figures, either \
                         side of the group dividers, and from the edge of the pill."
                    },
                );
                if d != 0.0 {
                    self.cfg.gaps_x = self.cfg.gaps_x() + d;
                    self.cfg.save();
                }

                let d = stepper_row(
                    ui,
                    m,
                    if ru { "Скругление углов" } else { "Corner radius" },
                    format!("{:.0}", self.cfg.corner_radius()),
                    1.0,
                    4.0,
                    indent,
                    if ru {
                        "Скругление пилюли, панели и подсказок. 0 даёт прямые углы, \
                         половина высоты — прежнюю пилюлю."
                    } else {
                        "How round the pill, the panel and the hints are. Zero gives \
                         square corners; half the height gives back the lozenge."
                    },
                );
                if d != 0.0 {
                    self.cfg.corner_radius = self.cfg.corner_radius() + d;
                    self.cfg.save();
                }

                if setting_row(
                    ui,
                    m,
                    if ru {
                        "Сбросить размеры".to_string()
                    } else {
                        "Reset sizes".to_string()
                    },
                    false,
                    indent,
                )
                .on_hover_text(tip(
                    m,
                    if ru {
                        "Вернуть все размеры и отступы к исходным значениям."
                    } else {
                        "Put every size and padding back to the shipped values."
                    },
                ))
                .clicked()
                {
                    self.cfg.reset_sizes();
                    self.cfg.save();
                }
            }

            // ------------------------------------------------------ the rest ---
            let d = stepper_row(
                ui,
                m,
                if ru { "Непрозрачность" } else { "Opacity" },
                format!("{:.0}%", self.cfg.opacity * 100.0),
                0.01,
                0.05,
                0.0,
                if ru {
                    "Насколько плотная подложка у пилюли и панели. Текст и цифры \
                     не бледнеют."
                } else {
                    "How solid the background of the pill and the panel is. The \
                     text and the figures stay as they are."
                },
            );
            if d != 0.0 {
                self.cfg.opacity = (self.cfg.opacity + d).clamp(0.3, 1.0);
                self.cfg.save();
            }

            if setting_row(
                ui,
                m,
                format!(
                    "{}: {}",
                    if ru { "Язык" } else { "Language" },
                    self.cfg.lang.to_uppercase()
                ),
                false,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Язык подписей и панели подробностей: русский или английский."
                } else {
                    "Language of the labels and the detail panel: Russian or English."
                },
            ))
            .clicked()
            {
                self.cfg.lang = if ru { "en".into() } else { "ru".into() };
                self.cfg.save();
            }

            if self.tray.is_some() {
                let label = format!(
                    "{}: {}",
                    if ru { "Клики насквозь" } else { "Click-through" },
                    match (self.cfg.click_through, ru) {
                        (true, true) => "вкл",
                        (false, true) => "выкл",
                        (true, false) => "on",
                        (false, false) => "off",
                    }
                );
                if setting_row(ui, m, label, false, 0.0)
                    .on_hover_text(tip(
                        m,
                        if ru {
                            "Пилюля перестаёт ловить мышь: клики уходят в окно под \
                             ней. Вернуть — из меню в трее."
                        } else {
                            "The pill stops taking the mouse; clicks go to whatever \
                             is underneath. Turn it back from the tray menu."
                        },
                    ))
                    .clicked()
                {
                    self.cfg.click_through = !self.cfg.click_through;
                    self.cfg.save();
                }
            }

            if setting_row(
                ui,
                m,
                if ru {
                    "Открыть config.toml".to_string()
                } else {
                    "Open config.toml".to_string()
                },
                false,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Открыть файл настроек в редакторе по умолчанию. Там есть и то, \
                     чего нет в этом окне."
                } else {
                    "Open the settings file in the default editor. It holds a few \
                     things this window does not show."
                },
            ))
            .clicked()
            {
                let _ = std::process::Command::new("cmd")
                    .args([
                        "/C",
                        "start",
                        "",
                        &crate::config::config_path().to_string_lossy(),
                    ])
                    .spawn();
            }

            if setting_row(
                ui,
                m,
                if ru { "Выход".to_string() } else { "Quit".to_string() },
                false,
                0.0,
            )
            .on_hover_text(tip(
                m,
                if ru {
                    "Закрыть виджет полностью, вместе со значком в трее."
                } else {
                    "Close the widget entirely, tray icon and all."
                },
            ))
            .clicked()
            {
                // Addressed to the pill's window: a plain `Close` from here
                // would only shut the settings.
                ui.ctx()
                    .send_viewport_cmd_to(egui::ViewportId::ROOT, egui::ViewportCommand::Close);
            }

            if let Some(api) = view::api_state_text(&self.cfg, &self.rt, now) {
                ui.add_space(4.0 * m.scale);
                ui.label(
                    egui::RichText::new(api)
                        .font(m.f_panel_dim.clone())
                        .color(DIM),
                );
            }
            });
            });
        });
    }
}

// ------------------------------------------------------- settings widgets ---

/// Hover text for a settings row, in the panel's own small font.
///
/// Every row has one. The rows are as short as a stepper leaves them, so the
/// label can only name a setting — what it actually does has to live here.
fn tip(m: &Metrics, text: &str) -> egui::RichText {
    egui::RichText::new(text).font(m.f_panel_dim.clone()).color(INK)
}

/// A full-width row of the settings window. Ordinary egui layout, unlike the
/// pill: inside a real window there is width to spare and nothing to gain from
/// placing rectangles by hand.
fn setting_row(
    ui: &mut egui::Ui,
    m: &Metrics,
    text: String,
    active: bool,
    indent: f32,
) -> egui::Response {
    ui.horizontal(|ui| {
        if indent > 0.0 {
            ui.add_space(indent);
        }
        let w = ui.available_width();
        let fill = if active {
            Color32::from_rgba_unmultiplied(0x6F, 0xCF, 0x97, 38)
        } else {
            Color32::from_rgba_unmultiplied(255, 255, 255, 12)
        };
        ui.add_sized(
            [w, m.row_h],
            egui::Button::new(egui::RichText::new(text).font(m.f_panel.clone()).color(INK))
                .fill(fill)
                .corner_radius(6.0 * m.scale),
        )
    })
    .inner
}

/// A value with a pair of buttons on each side: `−− −  label  + ++`. Two step
/// sizes because the useful gestures differ — nudge a percent to taste, or move
/// in jumps while hunting for a size.
fn stepper_row(
    ui: &mut egui::Ui,
    m: &Metrics,
    label: &str,
    value: String,
    small: f32,
    big: f32,
    indent: f32,
    hint: &str,
) -> f32 {
    let mut delta = 0.0;
    ui.horizontal(|ui| {
        if indent > 0.0 {
            ui.add_space(indent);
        }
        let bw = 30.0 * m.scale;
        let gaps = ui.spacing().item_spacing.x * 4.0;
        let middle = (ui.available_width() - bw * 4.0 - gaps).max(40.0);
        // The hint goes on every part of the row: egui shows the tooltip of
        // whatever is directly under the pointer, so one on the container alone
        // would never appear over the buttons.
        let button = |ui: &mut egui::Ui, text: &str| -> bool {
            ui.add_sized(
                [bw, m.row_h],
                egui::Button::new(egui::RichText::new(text).font(m.f_panel.clone()).color(INK))
                    .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 20))
                    .corner_radius(4.0 * m.scale),
            )
            .on_hover_text(tip(m, hint))
            .clicked()
        };
        if button(ui, "−−") {
            delta -= big;
        }
        if button(ui, "−") {
            delta -= small;
        }
        ui.allocate_ui_with_layout(
            Vec2::new(middle, m.row_h),
            egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
            |ui| {
                ui.label(
                    egui::RichText::new(format!("{label}: {value}"))
                        .font(m.f_panel.clone())
                        .color(INK),
                )
                .on_hover_text(tip(m, hint));
            },
        );
        if button(ui, "+") {
            delta += small;
        }
        if button(ui, "++") {
            delta += big;
        }
    });
    delta
}

/// The fold marker on a section header, painted rather than typed: neither
/// Segoe UI nor egui's bundled fonts carry U+25BE, and a missing glyph renders
/// as a box.
fn chevron(ui: &egui::Ui, r: Rect, open: bool, m: &Metrics) {
    let cx = r.right() - 14.0 * m.scale;
    let cy = r.center().y;
    let dy = if open { -1.0 } else { 1.0 };
    let t = 3.0 * m.scale;
    let w = 4.0 * m.scale;
    ui.painter().add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(cx - w, cy - dy * t),
            Pos2::new(cx + w, cy - dy * t),
            Pos2::new(cx, cy + dy * t),
        ],
        DIM,
        Stroke::NONE,
    ));
}
/// Resolve a `view::Tone` against the pill's palette.
///
/// The meaning is decided in `view`; only the values live here, which is what
/// lets a different front end map the same tones onto CSS variables instead.
fn ink(ink: Ink) -> Color32 {
    match ink {
        Ink::Green => GREEN,
        Ink::Amber => AMBER,
        Ink::Orange => ORANGE,
        Ink::Red => RED,
    }
}

fn tone_colour(tone: Tone) -> Color32 {
    let (a, b) = (ink(tone.from), ink(tone.to));
    let t = tone.t.clamp(0.0, 1.0);
    Color32::from_rgb(
        (a.r() as f32 + (b.r() as f32 - a.r() as f32) * t) as u8,
        (a.g() as f32 + (b.g() as f32 - a.g() as f32) * t) as u8,
        (a.b() as f32 + (b.b() as f32 - a.b() as f32) * t) as u8,
    )
}
