//! What the widget says, as opposed to how it is drawn.
//!
//! Everything here turns the merged snapshot and the runtime's state into the
//! text, numbers and verdicts a person reads. No egui: a colour comes out as a
//! meaning rather than as an RGB value, and the words come out as `String`.
//! That is deliberate — this module is the contract a different front end
//! would consume, and a front end that speaks CSS cannot use `Color32`.

use crate::config::{BarColour, Config, CtxView, PaceView, PctMode, SourceKind};
use crate::model::{ContextInfo, Snapshot, Window};
use crate::history;
use crate::pace;
use crate::runtime::Runtime;

/// A colour named by role rather than by value.
///
/// The palette belongs to whatever draws the pill — egui today, CSS variables
/// tomorrow. What this module decides is *which* colours a figure sits
/// between, and how far along; the values themselves are none of its business.
///
/// It serialises, because this module is the contract a second front end reads
/// (A19, A21) and a role name is exactly what travels: `--serve` publishes
/// `"green"`, a CSS widget turns that into `var(--green)`, and neither has to
/// agree with the other about an RGB value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Ink {
    Green,
    Amber,
    Orange,
    Red,
}

/// A point on a two-colour ramp: `t` of the way from `from` to `to`.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Tone {
    pub from: Ink,
    pub to: Ink,
    pub t: f32,
}

impl Tone {
    fn flat(ink: Ink) -> Tone {
        Tone { from: ink, to: ink, t: 0.0 }
    }
}

/// How full a window is, as a ramp from green through amber to red.
///
/// Fed `used_pct` and never the shown figure (A15): "10% left" must not come
/// up green at the moment it should be shouting.
pub fn usage_tone(pct: f64, cfg: &Config) -> Tone {
    if pct < cfg.warn_pct {
        Tone::flat(Ink::Green)
    } else if pct < cfg.danger_pct {
        Tone {
            from: Ink::Green,
            to: Ink::Amber,
            t: ((pct - cfg.warn_pct) / (cfg.danger_pct - cfg.warn_pct)) as f32,
        }
    } else {
        Tone {
            from: Ink::Amber,
            to: Ink::Red,
            t: ((pct - cfg.danger_pct) / (100.0 - cfg.danger_pct).max(1.0)) as f32,
        }
    }
}

/// Green while spending keeps up with the clock, red once it has run away.
///
/// The scale is the pace tolerance from the config: within it the burn is even,
/// one tolerance past it is a warning, two is trouble. Nothing here looks at
/// how full the window is — that is what `usage_tone` is for.
///
/// Green lives entirely below the line, and above it the ramp starts at
/// orange rather than at green: a ramp starting from green would make "just
/// over" look like "fine", which is the one reading this must not produce.
pub fn pace_tone(delta: f64, cfg: &Config) -> Tone {
    let tol = cfg.pace_tolerance.max(1.0);
    if delta <= 0.0 {
        Tone::flat(Ink::Green)
    } else {
        Tone { from: Ink::Orange, to: Ink::Red, t: (delta / (2.0 * tol)) as f32 }
    }
}


/// The widest each form of the pace cell gets, so the pill keeps its width as
/// the figures move.
pub fn pace_template(view: PaceView) -> &'static str {
    match view {
        PaceView::Off => "",
        PaceView::Ratio => "▲10.0×",
        PaceView::Points => "▲99%",
        PaceView::RatioPoints => "▲10.0× 99%",
        PaceView::PointsRatio => "▲99% 10.0×",
        // Two cells with a rule between them, each measured on its own.
        PaceView::Split => "▲99%",
        PaceView::Piped => "▲99%|10.0×",
    }
}


// ---------------------------------------------------------------------------
// What the pill is made of
// ---------------------------------------------------------------------------

/// Which of the pill's three type sizes a cell is set in.
///
/// A role, not a font: which face and how many points is a question for
/// whatever draws the pill, and for the theme after that. What is decided here
/// is that a percentage is a figure and the clock beside it is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The large percentages.
    Pct,
    /// Countdown, day count, daily allowance, the context figures.
    Time,
    /// The `ctx` tag.
    Small,
}

/// How much room to leave after a cell, and what to put in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Gap {
    /// The usual space between two figures of one limit.
    Cell,
    /// Nothing: the cells either side read as one word.
    Tight,
    /// A short faint rule, between two halves of one figure.
    Rule,
    /// The divider between two limits.
    Group,
}

/// Where the text sits inside the width a cell reserves.
///
/// Every cell is at least as wide as the widest figure it will ever hold, so
/// that the pill does not twitch as the numbers change; centring that slack is
/// right for a figure standing alone, and wrong for two halves that have to
/// touch a separator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Centre,
    /// Text against the right edge of its width: the slack falls to its left.
    Right,
    /// Text against the left edge: the slack falls to its right.
    Left,
}

/// What a cell's colour means.
///
/// Three cases rather than a colour, for the same reason `Ink` exists: the
/// palette belongs to whoever draws. `Plain` is ordinary text, `Dim` is text
/// that qualifies something else — a wall-clock time beside a countdown, a
/// figure that is only history — and `Tone` is a figure whose colour is part
/// of what it says.
#[derive(Clone, Copy, Debug, serde::Serialize)]
#[serde(rename_all = "lowercase", tag = "kind", content = "tone")]
pub enum Paint {
    Plain,
    Dim,
    Tone(Tone),
}

/// One figure in the pill's single line.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Cell {
    pub text: String,
    /// The widest string this cell will ever hold. Its width is the cell's
    /// width, so that the pill does not change size as the digits change —
    /// which matters more than it sounds, since the window is sized to fit and
    /// resizing it moves everything to its right.
    pub template: String,
    pub role: Role,
    pub paint: Paint,
    pub gap: Gap,
    pub side: Side,
    /// Which limit this cell belongs to: 0 the five-hour window, 1 the week,
    /// 2 the conversation. The bar under a group spans exactly its cells.
    pub group: usize,
    /// What the field is called, for the hint under the pointer. `None` for
    /// the separator inside a piped pace figure, which is punctuation rather
    /// than a field.
    pub field: Option<Field>,
}

/// The hairline under one group: the same figure as the cells above it.
#[derive(Clone, Copy, Debug, serde::Serialize)]
pub struct Bar {
    pub group: usize,
    /// The stored reading, always "spent" (A15).
    pub used_pct: f64,
    /// How much of the track is filled, 0..1, under the display convention in
    /// force. Length and colour answer different questions on purpose: a short
    /// red bar is a window with room left and no time to use it.
    pub frac: f32,
    pub paint: Paint,
    pub field: Field,
}

impl Cell {
    fn new(text: impl Into<String>, template: &str, role: Role, paint: Paint) -> Cell {
        Cell {
            text: text.into(),
            template: template.to_string(),
            role,
            paint,
            gap: Gap::Cell,
            side: Side::Centre,
            group: 0,
            field: None,
        }
    }

    fn named(mut self, field: Field) -> Cell {
        self.field = Some(field);
        self
    }

    fn with_gap(mut self, gap: Gap) -> Cell {
        self.gap = gap;
        self
    }

    fn aligned(mut self, side: Side) -> Cell {
        self.side = side;
        self
    }
}

/// The divider between two limits belongs to the cell before it.
fn end_group(cells: &mut [Cell]) {
    if let Some(c) = cells.last_mut() {
        c.gap = Gap::Group;
    }
}

/// Everything the pill says, in order, as cells.
///
/// No widths and no colours: a width needs a font and a font belongs to the
/// drawing layer, which since the move to Slint is not even in this process's
/// hands — the theme measures its own text. What this function fixes is the
/// composition: which figures appear, in what order, in whose group, worded in
/// the language the pill is speaking.
pub fn cells(
    cfg: &Config,
    snap: &Snapshot,
    stale: bool,
    ctx_stale: bool,
    now: i64,
) -> Vec<Cell> {
    let lang = cfg.lang.clone();
    // Stale figures are shown in grey, the same as unknown ones: they are
    // history, not the current state.
    let pct = |v: f64| {
        if stale {
            Paint::Dim
        } else {
            Paint::Tone(usage_tone(v, cfg))
        }
    };
    let plain = if stale { Paint::Dim } else { Paint::Plain };
    let mut cells: Vec<Cell> = Vec::new();

    // The figure shown is either what is gone or what is left, but the colour
    // is always keyed on spending: "10% left" has to read as danger, not as a
    // low number.
    let shown = |v: f64| cfg.pct_mode.shown(v);

    match snap.five_hour {
        Some(w) => {
            cells.push(
                Cell::new(format!("{:.0}%", shown(w.used_pct)), "100%", Role::Pct, pct(w.used_pct))
                    .named(Field::FiveHourPct),
            );
            // Second, against the figure it qualifies: how much is left and
            // how fast it is going are one reading, and the two clocks after
            // them are a different one.
            cells.extend(pace_cells(cfg, w, pace::FIVE_HOUR_SECS, Field::FiveHourPace, stale, now));
            let (left, clock) = match w.resets_at {
                Some(r) => (pace::fmt_remaining(r - now), pace::fmt_clock(r)),
                None => ("--:--".to_string(), "--:--".to_string()),
            };
            cells.push(Cell::new(left, "00:00", Role::Time, plain).named(Field::FiveHourLeft));
            cells.push(
                Cell::new(clock, "00:00", Role::Time, Paint::Dim).named(Field::FiveHourClock),
            );
        }
        None => {
            cells.push(Cell::new("--%", "100%", Role::Pct, Paint::Dim).named(Field::FiveHourPct));
            if cfg.pace_view != PaceView::Off {
                let t = pace_template(cfg.pace_view);
                cells.push(Cell::new("--", t, Role::Time, Paint::Dim).named(Field::FiveHourPace));
            }
            cells.push(
                Cell::new("--:--", "00:00", Role::Time, Paint::Dim).named(Field::FiveHourLeft),
            );
            cells.push(
                Cell::new("--:--", "00:00", Role::Time, Paint::Dim).named(Field::FiveHourClock),
            );
        }
    }
    end_group(&mut cells);

    let weekly_from = cells.len();
    match snap.seven_day {
        Some(w) => {
            cells.push(
                Cell::new(format!("{:.0}%", shown(w.used_pct)), "100%", Role::Pct, pct(w.used_pct))
                    .named(Field::SevenDayPct),
            );
            cells.extend(pace_cells(cfg, w, pace::SEVEN_DAY_SECS, Field::SevenDayPace, stale, now));
            // Days left, not the name of the reset day: the weekly figure is
            // only actionable next to the number of days it has to cover, and
            // counting from "Tue" to that number is work for the reader.
            let (day, allowance) = match w.resets_at {
                Some(r) => (
                    pace::fmt_days_until(r, now, &lang, cfg.day_count()),
                    format!(
                        "{:.0}%",
                        pace::daily_allowance(w.used_pct, pace::days_until(r, now, cfg.day_count()))
                    ),
                ),
                None => ("--".to_string(), "--%".to_string()),
            };
            cells.push(Cell::new(day, "00д", Role::Time, Paint::Dim).named(Field::SevenDayDays));
            cells.push(
                Cell::new(allowance, "100%", Role::Time, plain).named(Field::SevenDayAllowance),
            );
        }
        None => {
            cells.push(Cell::new("--%", "100%", Role::Pct, Paint::Dim).named(Field::SevenDayPct));
            if cfg.pace_view != PaceView::Off {
                let t = pace_template(cfg.pace_view);
                cells.push(Cell::new("--", t, Role::Time, Paint::Dim).named(Field::SevenDayPace));
            }
            cells.push(Cell::new("--", "00д", Role::Time, Paint::Dim).named(Field::SevenDayDays));
            cells.push(
                Cell::new("--%", "100%", Role::Time, Paint::Dim).named(Field::SevenDayAllowance),
            );
        }
    }
    for c in &mut cells[weekly_from..] {
        c.group = 1;
    }

    let ctx_from = cells.len();
    if cfg.ctx_view != CtxView::Off {
        match &snap.context {
            Some(c) => {
                end_group(&mut cells[..ctx_from]);
                cells.push(Cell::new("ctx", "ctx", Role::Small, Paint::Dim).named(Field::Context));
                cells.extend(ctx_cells(cfg, c, ctx_stale));
            }
            // The source is switched on and has found nothing yet, which is
            // worth saying: a group that simply is not there looks the same as
            // a setting nobody turned on.
            None if cfg.session_enabled => {
                end_group(&mut cells[..ctx_from]);
                cells.push(Cell::new("ctx", "ctx", Role::Small, Paint::Dim).named(Field::Context));
                if cfg.ctx_view.has_pct() {
                    cells.push(
                        Cell::new("--%", "100%", Role::Time, Paint::Dim).named(Field::ContextPct),
                    );
                }
                if cfg.ctx_view.has_tokens() {
                    cells.push(
                        Cell::new("--", "999k", Role::Time, Paint::Dim).named(Field::ContextTokens),
                    );
                }
            }
            None => {}
        }
    }
    for c in &mut cells[ctx_from..] {
        c.group = 2;
    }

    cells
}

/// The context group, in whichever of the five forms `ctx_view` names.
///
/// Two figures of one reading, so they share a group — but a context window
/// fills instead of draining like the 5-hour and weekly quotas do, so it reads
/// by its own pair of directions (`ctx_pct_dir` / `ctx_tokens_dir`) rather than
/// the quotas' `pct_mode`. A remaining token count owes as much to the guessed
/// window size as the remaining percentage already does, so nothing here is
/// more of a guess than the other.
fn ctx_cells(cfg: &Config, c: &ContextInfo, ctx_stale: bool) -> Vec<Cell> {
    let view = cfg.ctx_view;
    let paint = if ctx_stale { Paint::Dim } else { Paint::Plain };
    let pct = || {
        Cell::new(format!("{:.0}%", cfg.ctx_pct_dir.shown(c.used_pct)), "100%", Role::Time, paint)
            .named(Field::ContextPct)
    };
    let tokens = || {
        let text = match (c.used_tokens, cfg.ctx_tokens_dir()) {
            (Some(t), PctMode::Spent) => pace::fmt_tokens(t),
            (Some(t), PctMode::Left) => match c.limit_tokens {
                Some(limit) => pace::fmt_tokens(limit.saturating_sub(t)),
                None => "--".to_string(),
            },
            (None, _) => "--".to_string(),
        };
        Cell::new(text, "999k", Role::Time, paint).named(Field::ContextTokens)
    };
    let mut out = Vec::new();
    if view.tokens_first() {
        out.push(tokens());
        if view.has_pct() {
            out.push(pct());
        }
    } else {
        out.push(pct());
        if view.has_tokens() {
            out.push(tokens());
        }
    }
    out
}

/// The pace figure as a cell, when it is wanted and knowable.
fn pace_cells(
    cfg: &Config,
    w: Window,
    span: i64,
    field: Field,
    stale: bool,
    now: i64,
) -> Vec<Cell> {
    let view = cfg.pace_view;
    if view == PaceView::Off {
        return Vec::new();
    }
    let Some(delta) = pace_delta(w, span, now) else {
        return vec![Cell::new("--", pace_template(view), Role::Time, Paint::Dim).named(field)];
    };
    let target = w.used_pct - delta;
    let paint = if stale { Paint::Dim } else { Paint::Tone(pace_tone(delta, cfg)) };
    let parts = pace::pace_parts(view, w.used_pct, target);
    // Both halves and the separator between them answer to the same name.
    let half = |text: &str, template: &str| {
        Cell::new(text, template, Role::Time, paint).named(field)
    };
    match view {
        // Two cells with a rule painted between them.
        PaceView::Split if parts.len() == 2 => vec![
            half(&parts[0], "▲99%").with_gap(Gap::Rule),
            half(&parts[1], "10.0×"),
        ],
        // Three cells run together with no gaps: the separator is a cell of
        // its own only so that it can keep the grey of the clock beside it
        // while the two numbers carry the pace colour.
        PaceView::Piped if parts.len() == 2 => vec![
            half(&parts[0], "▲99%").with_gap(Gap::Tight).aligned(Side::Right),
            Cell::new(view.joiner(), view.joiner(), Role::Time, Paint::Dim)
                .with_gap(Gap::Tight)
                .named(field),
            half(&parts[1], "10.0×").aligned(Side::Left),
        ],
        _ => vec![half(&parts.join(view.joiner()), pace_template(view))],
    }
}

/// The hairline under each group that has one.
///
/// Not every group does: a window nobody has reported has a row of dashes
/// above it and nothing to draw underneath. The order matches `cells`, and
/// `group` says which run of cells each bar spans.
pub fn bars(cfg: &Config, snap: &Snapshot, ctx_stale: bool, now: i64) -> Vec<Bar> {
    let mut out = Vec::new();
    let quota = |w: Option<Window>, span: i64, group: usize, field: Field| -> Option<Bar> {
        let w = w?;
        // Length and colour answer different questions on purpose.
        let tone = match cfg.bar_colour {
            BarColour::Fill => usage_tone(w.used_pct, cfg),
            BarColour::Pace => match pace_delta(w, span, now) {
                Some(delta) => pace_tone(delta, cfg),
                None => usage_tone(w.used_pct, cfg),
            },
        };
        Some(Bar {
            group,
            used_pct: w.used_pct,
            // The bar carries the same number as the figure above it, so in
            // "left" mode it drains as the quota goes. Its colour does not:
            // that stays on spending, which is what a short red bar is telling
            // you.
            frac: (cfg.pct_mode.shown(w.used_pct) / 100.0).clamp(0.0, 1.0) as f32,
            paint: Paint::Tone(tone),
            field,
        })
    };
    out.extend(quota(snap.five_hour, pace::FIVE_HOUR_SECS, 0, Field::FiveHourBar));
    out.extend(quota(snap.seven_day, pace::SEVEN_DAY_SECS, 1, Field::SevenDayBar));
    // The context group gets the same bar. It has no window and no reset, so
    // there is no pace to colour it by — only how full it is, which is the
    // whole of what it says.
    if cfg.ctx_view != CtxView::Off
        && let Some(c) = &snap.context
    {
        out.push(Bar {
            group: 2,
            used_pct: c.used_pct,
            frac: (cfg.ctx_pct_dir.shown(c.used_pct) / 100.0).clamp(0.0, 1.0) as f32,
            paint: if ctx_stale {
                Paint::Dim
            } else {
                Paint::Tone(usage_tone(c.used_pct, cfg))
            },
            field: Field::ContextBar,
        });
    }
    out
}

/// A field of the pill, for the hint that names it under the pointer.
///
/// The pill is eight or nine bare figures in a row; the panel below spells them
/// out but only all at once. This answers the narrower question a newcomer
/// actually has, which is "that number there — what is it?".
///
/// It serialises in `snake_case` — `five_hour_pct` — because a field is an
/// identity rather than a label: `field_name` gives the wording, which is
/// allowed to be reworded, and this is what a contract can be keyed on (A21).
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    FiveHourPct,
    FiveHourPace,
    FiveHourLeft,
    FiveHourClock,
    FiveHourBar,
    SevenDayPct,
    SevenDayPace,
    SevenDayDays,
    SevenDayAllowance,
    SevenDayBar,
    Context,
    ContextPct,
    ContextTokens,
    ContextBar,
}


/// What to call a field, in the language the pill is speaking.
pub fn field_name(field: Field, cfg: &Config) -> String {
    let ru = cfg.is_ru();
    // The percentages follow `pct_mode`, so their names have to as well: the
    // hint is there to remove doubt, not to add a second reading of it.
    let (spent, left) = if ru {
        ("потрачено", "осталось")
    } else {
        ("spent", "left")
    };
    let side = match cfg.pct_mode {
        crate::config::PctMode::Spent => spent,
        crate::config::PctMode::Left => left,
    };
    // `ctx` reads by its own pair of directions, not `pct_mode` — see
    // `ctx_dir_linked`.
    let ctx_pct_side = match cfg.ctx_pct_dir {
        crate::config::PctMode::Spent => spent,
        crate::config::PctMode::Left => left,
    };
    let ctx_tok_side = match cfg.ctx_tokens_dir() {
        crate::config::PctMode::Spent => spent,
        crate::config::PctMode::Left => left,
    };
    match (field, ru) {
        (Field::FiveHourPct, true) => format!("5-часовой лимит: {side}"),
        (Field::FiveHourPct, false) => format!("5-hour limit: {side}"),
        (Field::FiveHourPace, true) => "темп: расход против часов".into(),
        (Field::FiveHourPace, false) => "pace: spending against the clock".into(),
        (Field::FiveHourLeft, true) => "до сброса 5-часового лимита".into(),
        (Field::FiveHourLeft, false) => "until the 5-hour window resets".into(),
        (Field::FiveHourClock, true) => "время сброса".into(),
        (Field::FiveHourClock, false) => "clock time of the reset".into(),
        (Field::FiveHourBar, true) => "полоса 5 часов: то же число".into(),
        (Field::FiveHourBar, false) => "5-hour bar: the same figure".into(),
        (Field::SevenDayPct, true) => format!("недельный лимит: {side}"),
        (Field::SevenDayPct, false) => format!("weekly limit: {side}"),
        (Field::SevenDayPace, true) => "темп недели: расход против дней".into(),
        (Field::SevenDayPace, false) => "weekly pace: spending against the days".into(),
        (Field::SevenDayDays, true) => "дней до сброса недели".into(),
        (Field::SevenDayDays, false) => "days until the weekly reset".into(),
        (Field::SevenDayAllowance, true) => "дневная норма до сброса".into(),
        (Field::SevenDayAllowance, false) => "daily allowance until the reset".into(),
        (Field::SevenDayBar, true) => "полоса недели: то же число".into(),
        (Field::SevenDayBar, false) => "weekly bar: the same figure".into(),
        (Field::Context, true) => "контекст сессии".into(),
        (Field::Context, false) => "session context window".into(),
        (Field::ContextPct, true) => format!("контекст: доля окна, {ctx_pct_side}"),
        (Field::ContextPct, false) => format!("context: share of the window, {ctx_pct_side}"),
        (Field::ContextTokens, true) => format!("контекст: токенов, {ctx_tok_side}"),
        (Field::ContextTokens, false) => format!("context: tokens, {ctx_tok_side}"),
        (Field::ContextBar, true) => "полоса контекста: та же доля".into(),
        (Field::ContextBar, false) => "context bar: the same share".into(),
    }
}


/// How far spending has run ahead of the clock, in points. `None` without a
/// reset time: there is no clock to be ahead of.
pub fn pace_delta(w: Window, span: i64, now: i64) -> Option<f64> {
    let target = pace::pace_target(w.resets_at?, span, now);
    Some(w.used_pct - target)
}


/// How the API is doing, in words a person can act on. A rate limit is not
/// a failure to report and forget — it says when to look again.
pub fn api_state_text(cfg: &Config, rt: &Runtime, now: i64) -> Option<String> {
    if !cfg.api_enabled {
        return None;
    }
    Some(match api_notice(cfg, rt, now) {
        Some(notice) => notice,
        None => match rt.api_last_ok {
            Some(ok) => format!("API {}", pace::fmt_duration(now - ok)),
            None => "API —".to_string(),
        },
    })
}

/// The part of the above that the age of a reading cannot say: a poll in
/// flight, a rate limit, a queued request, an error.
pub fn api_notice(cfg: &Config, rt: &Runtime, now: i64) -> Option<String> {
    if !cfg.api_enabled {
        return None;
    }
    let ru = cfg.is_ru();
    Some(if let Some(until) = rt.api_retry_until.filter(|u| *u > now) {
        let mins = ((until - now) + 59) / 60;
        if ru {
            format!("лимит запросов, ещё {mins} мин")
        } else {
            format!("rate limited, {mins} min left")
        }
    } else if let Some(until) = rt.api_wait_until.filter(|u| *u > now) {
        // Not a refusal — the request is queued behind our own budget and
        // goes out by itself, so the wording says "in", not "wait".
        let secs = until - now;
        if ru {
            format!("запрос через {secs} с")
        } else {
            format!("request in {secs} s")
        }
    } else if rt.api_polling {
        if ru { "опрос…".to_string() } else { "polling…".to_string() }
    } else if let Some(err) = &rt.api_status {
        format!("API: {err}")
    } else {
        return None;
    })
}

/// The observed rate and what it means for the reset, as the tail of a
/// window's line. Empty while the log has too little to say: a forecast
/// from two readings a minute apart would be theatre.
pub fn rate_text(cfg: &Config, rt: &Runtime, w: Window, span: i64, resets_at: i64, now: i64) -> String {
    if !cfg.pace_rate {
        return String::new();
    }
    let Some(rate) = rt.burn_rate(w, span, now) else {
        return String::new();
    };
    let ru = cfg.is_ru();
    let daily = span != pace::FIVE_HOUR_SECS;
    let unit = match (daily, ru) {
        (false, true) => "%/ч",
        (false, false) => "%/h",
        (true, true) => "%/день",
        (true, false) => "%/day",
    };
    let mut out = format!(
        " · {}{unit}",
        pace::fmt_rate(if daily { rate * 24.0 } else { rate })
    );

    match history::runs_out_at(w.used_pct, rate, now) {
        Some(gone) if gone < resets_at => {
            let at = if daily {
                pace::fmt_reset_full(gone, &cfg.lang)
            } else {
                pace::fmt_clock(gone)
            };
            let gap = resets_at - gone;
            let early = if gap >= 86400 {
                let d = gap / 86400;
                if ru { format!("{d}д") } else { format!("{d}d") }
            } else {
                pace::fmt_remaining(gap)
            };
            out.push_str(&if ru {
                format!(" · кончится {at}, за {early} до сброса")
            } else {
                format!(" · runs out {at}, {early} before the reset")
            });
        }
        // Either nothing is being spent or the pace makes it to the end;
        // both are the same news to the reader.
        _ => out.push_str(if ru {
            " · хватит до сброса"
        } else {
            " · lasts to the reset"
        }),
    }
    out
}

/// The detail panel's contents as (label, value) pairs, built once so the
/// panel can be measured before it is painted.
pub fn detail_lines(cfg: &Config, rt: &Runtime, snap: &Snapshot, stale: bool, now: i64) -> Vec<(String, String)> {
    let ru = cfg.is_ru();
    let mut out: Vec<(String, String)> = Vec::new();
    let no_data = || {
        if ru {
            "нет данных".to_string()
        } else {
            "no data".to_string()
        }
    };

    // Each window names its own source: with three of them feeding the
    // pill, "where did this number come from" has to be answerable without
    // guessing. How old each source is belongs to the source, not to the
    // window, so it is said once on the `src` line.
    // The panel is where the convention is spelled out: both numbers, the
    // one that is on the pill first, so the eye lands on the same figure it
    // just left and the other reading is there without a second thought.
    let mode = cfg.pct_mode;
    let both = |used: f64| {
        format!(
            "{:.0}% {} · {:.0}% {}",
            mode.shown(used),
            mode.label(ru),
            mode.other().shown(used),
            mode.other().label(ru),
        )
    };

    let describe = |w: Window, span: i64| -> String {
        let mut text = both(w.used_pct);
        match w.resets_at {
            Some(r) => {
                let target = pace::pace_target(r, span, now);
                let verdict = pace::pace_of(w.used_pct, target, cfg.pace_tolerance)
                    .describe(&cfg.lang);
                // The figure and the word together: the pill has room for
                // only the figure, so this is where the arrow is explained.
                text.push_str(&format!(
                    " · {} {} · {} {}",
                    if ru { "сброс" } else { "resets" },
                    pace::fmt_reset_full(r, &cfg.lang),
                    // Both numbers here whatever the pill was asked for: the
                    // panel is where everything is spelled out.
                    pace::pace_text(cfg.pace_view.both(), w.used_pct, target),
                    verdict
                ));
                text.push_str(&rate_text(cfg, rt, w, span, r, now));
            }
            None => text.push_str(&format!(
                " · {}",
                if ru {
                    "время сброса неизвестно"
                } else {
                    "reset time unknown"
                }
            )),
        }
        // Only the name here. The age of each source is on the `src`
        // line below, once: with both windows coming from one source it
        // was printed three times over, all three the same number.
        text.push_str(&format!(" · {}", w.source.label()));
        text
    };

    out.push((
        "5h".to_string(),
        match snap.five_hour {
            Some(w) => describe(w, pace::FIVE_HOUR_SECS),
            None => no_data(),
        },
    ));
    out.push((
        "7d".to_string(),
        match snap.seven_day {
            Some(w) => {
                let mut text = describe(w, pace::SEVEN_DAY_SECS);
                if let Some(r) = w.resets_at {
                    // Said in words, because the rate above it is also a
                    // figure per day and the two mean opposite things: one
                    // is what is happening, this is what is allowed.
                    let allowed = pace::daily_allowance(
                        w.used_pct,
                        pace::days_until(r, now, cfg.day_count()),
                    );
                    text.push_str(&if ru {
                        format!(" · можно {allowed:.0}%/день")
                    } else {
                        format!(" · {allowed:.0}%/day allowed")
                    });
                }
                text
            }
            None => no_data(),
        },
    ));

    if let Some(c) = &snap.context {
        let ctx_mode = cfg.ctx_pct_dir;
        let mut parts =
            vec![format!("{:.0}% {}", ctx_mode.shown(c.used_pct), ctx_mode.label(ru))];
        // The division, not only its answer: a denominator that is wrong by
        // five times is invisible in the percentage alone.
        if let (Some(t), Some(l)) = (c.used_tokens, c.limit_tokens) {
            let guess = match (c.limit_stated, ru) {
                (true, _) => "",
                (false, true) => " (оценка)",
                (false, false) => " (guessed)",
            };
            parts.push(format!(
                "{} {} {}{guess}",
                pace::fmt_tokens(t),
                if ru { "из" } else { "of" },
                pace::fmt_tokens(l)
            ));
        }
        if let Some(model) = &c.model {
            parts.push(model.trim_start_matches("claude-").to_string());
        }
        parts.push(match &c.label {
            Some(l) if !l.is_empty() => l.clone(),
            _ => c.session_id[..c.session_id.len().min(8)].to_string(),
        });
        parts.push(format!(
            "{} {}",
            c.source.label(),
            pace::fmt_duration(now - c.observed_at)
        ));
        out.push(("ctx".to_string(), parts.join(" · ")));
    }

    if cfg.detail_all_sources {
        // `--sources` without leaving the screen: what each source says on
        // its own, next to the merged figures above.
        for kind in SourceKind::ALL {
            if !cfg.enabled(kind) {
                continue;
            }
            let snap = match kind {
                SourceKind::Bridge => &rt.bridge,
                SourceKind::Api => &rt.api,
                SourceKind::Desktop => &rt.desktop,
                SourceKind::Session => &rt.session,
            };
            let pct = |w: Option<Window>| match w {
                Some(w) => format!("{:.0}%", mode.shown(w.used_pct)),
                None => "—".to_string(),
            };
            // What this source says about the context, so two sources
            // disagreeing about one session is visible rather than merged
            // away. That disagreement is the diagnosis, not a fault.
            let ctx = snap.context.as_ref().map(|c| {
                let mut s = format!("ctx {:.0}%", mode.shown(c.used_pct));
                if let Some(t) = c.used_tokens {
                    s.push_str(&format!(" {}", pace::fmt_tokens(t)));
                }
                s
            });
            let age = snap
                .five_hour
                .or(snap.seven_day)
                .map(|w| w.observed_at)
                .or(snap.context.as_ref().map(|c| c.observed_at))
                .map(|at| pace::fmt_duration(now - at));
            let windows = match (snap.five_hour, snap.seven_day) {
                (None, None) => None,
                _ => Some(format!("{} / {}", pct(snap.five_hour), pct(snap.seven_day))),
            };
            let body = match (windows, ctx) {
                (Some(w), Some(c)) => Some(format!("{w} · {c}")),
                (Some(w), None) => Some(w),
                (None, Some(c)) => Some(c),
                (None, None) => None,
            };
            let text = match (body, age) {
                (Some(body), Some(age)) => format!("{body} · {age}"),
                (Some(body), None) => body,
                (None, _) => if ru { "нет данных" } else { "no data" }.to_string(),
            };
            out.push((kind.short().to_string(), text));
        }
    }

    // A line per source already spells the ages out, so they are not
    // repeated here when that is switched on.
    let with_ages = !cfg.detail_all_sources;
    let names: Vec<String> = SourceKind::ALL
        .iter()
        .filter(|k| cfg.enabled(**k))
        .map(|k| {
            let from = match k {
                SourceKind::Bridge => &rt.bridge,
                SourceKind::Api => &rt.api,
                SourceKind::Desktop => &rt.desktop,
                SourceKind::Session => &rt.session,
            };
            let age = from
                .five_hour
                .or(from.seven_day)
                .map(|w| w.observed_at)
                .or(from.context.as_ref().map(|c| c.observed_at))
                .map(|at| pace::fmt_duration(now - at));
            match (with_ages, age) {
                (true, Some(age)) => format!("{} {age}", k.short()),
                // Enabled but silent so far — better said than left blank.
                (true, None) => format!("{} —", k.short()),
                (false, _) => k.short().to_string(),
            }
        })
        .collect();
    let mut tail = format!(
        "{}: {}",
        if ru { "источники" } else { "sources" },
        if cfg.demo {
            if ru { "демо".to_string() } else { "demo".to_string() }
        } else if names.is_empty() {
            if ru { "нет".to_string() } else { "none".to_string() }
        } else {
            names.join(" + ")
        }
    );
    // Only what the age does not already say: polling, a rate limit, a
    // queued request, an error.
    if let Some(api) = api_notice(cfg, rt, now) {
        tail.push_str(&format!(" · {api}"));
    }
    if stale {
        tail.push_str(if ru { " · устарело" } else { " · stale" });
    }
    out.push((if ru { "ист" } else { "src" }.to_string(), tail));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PctMode;
    use crate::model::{ContextInfo, Source};
    use chrono::{Local, TimeZone};

    /// A fixed moment, built in **local** time so the expected strings below
    /// hold whatever timezone the machine runs in — the same trick `pace.rs`
    /// uses for its clock tests. Friday, 11 September 2026, midday, a date
    /// deliberately clear of any daylight-saving switch, so "three hours and
    /// thirty-seven minutes from now" really does read 15:37.
    fn now() -> i64 {
        Local
            .with_ymd_and_hms(2026, 9, 11, 12, 0, 0)
            .single()
            .expect("a real local moment")
            .timestamp()
    }

    /// The defaults, in English. Half of these tests assert on wording, and a
    /// failure message full of Cyrillic is harder to read a diff out of.
    fn en() -> Config {
        Config { lang: "en".to_string(), ..Config::default() }
    }

    /// …with the observed rate left out, so that a test about the panel is not
    /// also a test of the log behind it. `rate_text` has its own tests above.
    fn quiet(cfg: Config) -> Config {
        Config { pace_rate: false, ..cfg }
    }

    fn window(now: i64, used_pct: f64, resets_in: i64, age: i64, source: Source) -> Window {
        Window {
            used_pct,
            resets_at: Some(now + resets_in),
            observed_at: now - age,
            source,
        }
    }

    fn context(now: i64) -> ContextInfo {
        ContextInfo {
            used_tokens: Some(382_000),
            limit_tokens: Some(1_000_000),
            used_pct: 38.0,
            // Guessed on purpose: a denominator nobody stated is the case the
            // panel has to mark, and the marker is what A18 is about.
            limit_stated: false,
            model: Some("claude-opus-5".into()),
            label: None,
            session_id: "abcdef0123456789".into(),
            observed_at: now - 90,
            source: Source::Session,
        }
    }

    /// The two quotas from the endpoint, the context from a transcript — the
    /// ordinary arrangement, with the runtime holding the same readings the
    /// merged snapshot does so the `src` line has ages to print.
    fn fixture(now: i64) -> (Snapshot, Runtime) {
        let five = window(now, 24.0, 3 * 3600 + 37 * 60, 12, Source::Api);
        let seven = window(now, 54.0, 3 * 86400 + 5 * 3600, 12, Source::Api);
        let ctx = context(now);
        let snap = Snapshot {
            five_hour: Some(five),
            seven_day: Some(seven),
            context: Some(ctx.clone()),
        };
        let mut rt = Runtime::blank();
        rt.api = Snapshot {
            five_hour: Some(five),
            seven_day: Some(seven),
            context: None,
        };
        rt.session = Snapshot {
            five_hour: None,
            seven_day: None,
            context: Some(ctx),
        };
        (snap, rt)
    }

    fn line<'a>(lines: &'a [(String, String)], label: &str) -> &'a str {
        lines
            .iter()
            .find(|(l, _)| l == label)
            .map(|(_, v)| v.as_str())
            .unwrap_or_else(|| panic!("no `{label}` line in {lines:?}"))
    }

    /// The cell list written out one line per cell, so a golden test reads as
    /// a picture of the pill rather than as a wall of struct literals.
    fn sketch(cells: &[Cell]) -> String {
        let mut out = String::new();
        for c in cells {
            let paint = match c.paint {
                Paint::Plain => "plain".to_string(),
                Paint::Dim => "dim".to_string(),
                Paint::Tone(t) if t.from == t.to => format!("{:?}", t.from).to_lowercase(),
                Paint::Tone(t) => format!("{:?}->{:?}@{:.2}", t.from, t.to, t.t).to_lowercase(),
            };
            out.push_str(&format!(
                "g{} {:?} {:?} {:?} {} {:?} {}\n",
                c.group,
                c.role,
                c.gap,
                c.side,
                paint,
                c.text,
                c.field.map(|f| format!("{f:?}")).unwrap_or_else(|| "-".into()),
            ));
        }
        out
    }

    #[test]
    fn the_pill_is_made_of_exactly_this() {
        // The golden test for the pill, beside the golden test for the panel.
        // It is the insurance for the move to Slint: the drawing layer is
        // replaced wholesale, and a difference here says the *figures* moved —
        // the one kind of difference that must not happen quietly.
        let now = now();
        let (snap, _) = fixture(now);
        assert_eq!(
            sketch(&cells(&en(), &snap, false, false, now)),
            "\
g0 Pct Cell Centre green \"24%\" FiveHourPct
g0 Time Cell Centre green \"▼1.1×\" FiveHourPace
g0 Time Cell Centre plain \"3:37\" FiveHourLeft
g0 Time Group Centre dim \"15:37\" FiveHourClock
g1 Pct Cell Centre green \"54%\" SevenDayPct
g1 Time Cell Centre green \"1.0×\" SevenDayPace
g1 Time Cell Centre dim \"3d\" SevenDayDays
g1 Time Group Centre plain \"15%\" SevenDayAllowance
g2 Small Cell Centre dim \"ctx\" Context
g2 Time Cell Centre plain \"38%\" ContextPct
"
        );
    }

    #[test]
    fn a_window_nobody_has_reported_is_a_row_of_dashes() {
        // Not an absent group: a group that simply is not there looks exactly
        // like a source somebody switched off, and those are different facts.
        let now = now();
        let cells = cells(&en(), &Snapshot::default(), false, false, now);
        assert!(
            cells.iter().all(|c| matches!(c.paint, Paint::Dim)),
            "nothing is known, so nothing is coloured"
        );
        let texts: Vec<&str> = cells.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            texts,
            ["--%", "--", "--:--", "--:--", "--%", "--", "--", "--%", "ctx", "--%"]
        );
        // The context group is still announced, because `session_enabled` is
        // on and "found nothing yet" is worth saying.
        assert_eq!(cells.last().map(|c| c.group), Some(2));
    }

    #[test]
    fn stale_figures_go_grey_rather_than_staying_colourful() {
        // A confident colour on a reading from an hour ago is a claim about
        // the present that nothing supports.
        let now = now();
        let (snap, _) = fixture(now);
        let cells = cells(&en(), &snap, true, true, now);
        assert!(cells.iter().all(|c| matches!(c.paint, Paint::Dim)), "{}", sketch(&cells));
    }

    #[test]
    fn the_bars_carry_the_spent_figure_and_the_colour_of_spending() {
        let now = now();
        let (snap, _) = fixture(now);
        let spent = bars(&en(), &snap, false, now);
        let groups: Vec<usize> = spent.iter().map(|b| b.group).collect();
        assert_eq!(groups, [0, 1, 2], "one bar per group, in the pill's order");
        assert!((spent[0].frac - 0.24).abs() < 1e-6);
        assert!((spent[1].frac - 0.54).abs() < 1e-6);

        // Reading the quotas the other way round drains the bar — and leaves
        // the colour alone. A short red bar is a window with room left and no
        // time to use it (A15).
        let left = Config { pct_mode: PctMode::Left, ..en() };
        let drained = bars(&left, &snap, false, now);
        assert!((drained[0].frac - 0.76).abs() < 1e-6, "the length follows the figure");
        assert_eq!(
            format!("{:?}", drained[0].paint),
            format!("{:?}", spent[0].paint),
            "the colour does not"
        );
    }

    #[test]
    fn a_piped_pace_is_three_cells_answering_to_one_name() {
        // The separator is a cell of its own only so that it can stay grey
        // while the two numbers either side carry the pace colour. It is
        // punctuation — but it still answers to the field's name, or crossing
        // it would blink the hint off and on again.
        let now = now();
        let (snap, _) = fixture(now);
        let cfg = Config { pace_view: PaceView::Piped, ..en() };
        let cells = cells(&cfg, &snap, false, false, now);
        let pace: Vec<&Cell> =
            cells.iter().filter(|c| c.field == Some(Field::FiveHourPace)).collect();
        assert_eq!(pace.len(), 3);
        assert_eq!(pace[1].text, "|");
        assert!(matches!(pace[1].paint, Paint::Dim), "the pipe is punctuation");
        assert_eq!(pace[0].side, Side::Right, "the halves lean on the separator");
        assert_eq!(pace[2].side, Side::Left);
        assert!(
            pace[0].gap == Gap::Tight && pace[1].gap == Gap::Tight,
            "and nothing sits between them"
        );
    }

    #[test]
    fn every_field_has_a_name_in_both_languages() {
        let mut cfg = Config::default();
        for field in [
            Field::FiveHourPct,
            Field::FiveHourPace,
            Field::FiveHourLeft,
            Field::FiveHourClock,
            Field::FiveHourBar,
            Field::SevenDayPct,
            Field::SevenDayPace,
            Field::SevenDayDays,
            Field::SevenDayAllowance,
            Field::SevenDayBar,
            Field::Context,
            Field::ContextPct,
            Field::ContextTokens,
            Field::ContextBar,
        ] {
            for lang in ["ru", "en"] {
                cfg.lang = lang.to_string();
                let name = field_name(field, &cfg);
                assert!(!name.is_empty(), "{lang}: a field with no name");
                assert!(name.chars().count() < 45, "{lang}: {name} is a sentence");
            }
        }
    }

    #[test]
    fn the_percentage_is_named_the_way_it_is_read() {
        let mut cfg = Config { lang: "ru".to_string(), ..Config::default() };

        cfg.pct_mode = PctMode::Spent;
        assert!(field_name(Field::FiveHourPct, &cfg).ends_with("потрачено"));
        cfg.pct_mode = PctMode::Left;
        assert!(field_name(Field::FiveHourPct, &cfg).ends_with("осталось"));

        cfg.lang = "en".to_string();
        assert!(field_name(Field::SevenDayPct, &cfg).ends_with("left"));
        cfg.pct_mode = PctMode::Spent;
        assert!(field_name(Field::SevenDayPct, &cfg).ends_with("spent"));
    }

    #[test]
    fn ctx_names_follow_their_own_direction_not_pct_mode() {
        let mut cfg = Config {
            pct_mode: PctMode::Left,
            ctx_pct_dir: PctMode::Spent,
            ..en()
        };
        // The quotas read "left" here, but ctx must not follow them.
        assert!(field_name(Field::ContextPct, &cfg).ends_with("spent"));
        assert!(field_name(Field::FiveHourPct, &cfg).ends_with("left"));

        cfg.ctx_dir_linked = false;
        cfg.ctx_tokens_dir = PctMode::Left;
        assert!(field_name(Field::ContextTokens, &cfg).ends_with("left"));
        assert!(field_name(Field::ContextPct, &cfg).ends_with("spent"), "split, not moved together");
    }

    // ---- colour as meaning -------------------------------------------------

    #[test]
    fn usage_is_flat_green_below_the_warning() {
        let cfg = Config::default();
        for pct in [0.0, 30.0, 59.9] {
            let t = usage_tone(pct, &cfg);
            assert_eq!((t.from, t.to), (Ink::Green, Ink::Green), "at {pct}%");
        }
    }

    #[test]
    fn usage_ramps_green_to_amber_then_amber_to_red() {
        let cfg = Config::default(); // warn 60, danger 85
        let t = usage_tone(60.0, &cfg);
        assert_eq!((t.from, t.to), (Ink::Green, Ink::Amber));
        assert!(t.t.abs() < 1e-6, "the ramp has not started at the line");

        let t = usage_tone(72.5, &cfg); // half way from 60 to 85
        assert!((t.t - 0.5).abs() < 1e-6, "got {}", t.t);

        let t = usage_tone(85.0, &cfg);
        assert_eq!((t.from, t.to), (Ink::Amber, Ink::Red));
        assert!(t.t.abs() < 1e-6);
        assert!((usage_tone(100.0, &cfg).t - 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_tone_reads_spending_whatever_the_pill_shows() {
        // A15: colour is fed `used_pct` and never the shown figure, so the
        // display convention must not reach it at all. "10% left" coming up
        // green is the exact reading this rule exists to prevent.
        let mut cfg = Config { pct_mode: PctMode::Spent, ..Config::default() };
        let spent = usage_tone(90.0, &cfg);
        cfg.pct_mode = PctMode::Left;
        let left = usage_tone(90.0, &cfg);

        assert_eq!((spent.from, spent.to), (left.from, left.to));
        assert!((spent.t - left.t).abs() < 1e-6);
        assert_eq!(spent.from, Ink::Amber, "90% spent is not a green figure");
    }

    #[test]
    fn pace_is_green_to_the_line_and_orange_the_moment_it_is_over() {
        // A16: green lives entirely below the line. A ramp that started from
        // green above it made a two-point overspend look like headroom.
        let cfg = Config::default();
        assert_eq!(pace_tone(-20.0, &cfg).from, Ink::Green);
        assert_eq!(pace_tone(0.0, &cfg).from, Ink::Green, "on the line is still green");

        let over = pace_tone(0.1, &cfg);
        assert_eq!((over.from, over.to), (Ink::Orange, Ink::Red));
    }

    #[test]
    fn pace_reaches_red_two_tolerances_over() {
        let cfg = Config::default(); // tolerance 7
        let t = pace_tone(7.0, &cfg);
        assert!((t.t - 0.5).abs() < 1e-6, "one tolerance over is half way, got {}", t.t);
        assert!((pace_tone(14.0, &cfg).t - 1.0).abs() < 1e-6);
    }

    // ---- the pace difference ----------------------------------------------

    #[test]
    fn the_pace_delta_is_spending_minus_the_clock() {
        let now = now();
        // Half a five-hour window gone, a quarter of the quota spent.
        let w = window(now, 25.0, pace::FIVE_HOUR_SECS / 2, 0, Source::Api);
        let d = pace_delta(w, pace::FIVE_HOUR_SECS, now).expect("a reset time is present");
        assert!((d - -25.0).abs() < 1e-9, "got {d}");

        let w = window(now, 80.0, pace::FIVE_HOUR_SECS / 2, 0, Source::Api);
        let d = pace_delta(w, pace::FIVE_HOUR_SECS, now).unwrap();
        assert!((d - 30.0).abs() < 1e-9, "over the line is positive, got {d}");
    }

    #[test]
    fn without_a_reset_there_is_no_clock_to_be_ahead_of() {
        let now = now();
        let mut w = window(now, 25.0, 0, 0, Source::Desktop);
        w.resets_at = None;
        assert!(pace_delta(w, pace::FIVE_HOUR_SECS, now).is_none());
    }

    // ---- what the endpoint is doing ---------------------------------------

    #[test]
    fn the_api_notice_says_what_the_age_of_a_reading_cannot() {
        let now = now();
        let mut cfg = en();
        let mut rt = Runtime::blank();

        assert_eq!(api_notice(&cfg, &rt, now), None, "nothing to report is silence");

        rt.api_polling = true;
        assert_eq!(api_notice(&cfg, &rt, now).unwrap(), "polling…");

        // Our own budget: the request is queued and goes out by itself, so the
        // wording says "in", not "wait" (A13).
        rt.api_wait_until = Some(now + 6);
        assert_eq!(api_notice(&cfg, &rt, now).unwrap(), "request in 6 s");

        // A real refusal outranks it — it is the one thing nothing may act
        // against.
        rt.api_retry_until = Some(now + 61);
        assert_eq!(api_notice(&cfg, &rt, now).unwrap(), "rate limited, 2 min left");

        // Both deadlines past, an error left over.
        rt.api_retry_until = Some(now - 1);
        rt.api_wait_until = Some(now - 1);
        rt.api_polling = false;
        rt.api_status = Some("HTTP 500".to_string());
        assert_eq!(api_notice(&cfg, &rt, now).unwrap(), "API: HTTP 500");

        cfg.api_enabled = false;
        assert_eq!(api_notice(&cfg, &rt, now), None, "a source switched off says nothing");
    }

    #[test]
    fn the_notice_speaks_the_pill_s_language() {
        let now = now();
        let cfg = Config::default(); // ru
        let mut rt = Runtime::blank();
        rt.api_retry_until = Some(now + 120);
        assert_eq!(api_notice(&cfg, &rt, now).unwrap(), "лимит запросов, ещё 2 мин");
    }

    #[test]
    fn the_api_line_falls_back_to_the_age_of_the_last_answer() {
        let now = now();
        let mut cfg = en();
        let mut rt = Runtime::blank();

        assert_eq!(
            api_state_text(&cfg, &rt, now).unwrap(),
            "API —",
            "switched on and never answered is not the same as switched off"
        );
        rt.api_last_ok = Some(now - 75);
        assert_eq!(api_state_text(&cfg, &rt, now).unwrap(), "API 1:15");

        // A notice outranks the age: "polling…" is news, "1:15 ago" is not.
        rt.api_polling = true;
        assert_eq!(api_state_text(&cfg, &rt, now).unwrap(), "polling…");

        cfg.api_enabled = false;
        assert_eq!(api_state_text(&cfg, &rt, now), None);
    }

    // ---- the observed rate -------------------------------------------------

    fn log_of(points: &[(i64, f64)], five_hour: bool) -> history::Log {
        history::Log {
            samples: points
                .iter()
                .map(|&(t, v)| history::Sample {
                    t,
                    fh: if five_hour { Some(v) } else { None },
                    sd: if five_hour { None } else { Some(v) },
                })
                .collect(),
            ..history::Log::default()
        }
    }

    #[test]
    fn the_rate_says_when_the_quota_runs_out_before_the_reset() {
        let now = now();
        let cfg = en();
        let mut rt = Runtime::blank();
        // Nothing to 40% over the last hour: 40 %/h, and 60% left at that pace
        // is an hour and a half — half an hour short of the reset.
        rt.history = log_of(&[(now - 3600, 0.0), (now, 40.0)], true);
        let w = window(now, 40.0, 2 * 3600, 0, Source::Api);

        assert_eq!(
            rate_text(&cfg, &rt, w, pace::FIVE_HOUR_SECS, now + 2 * 3600, now),
            " · 40%/h · runs out 13:30, 0:30 before the reset"
        );
    }

    #[test]
    fn a_pace_that_makes_it_to_the_reset_is_said_as_much() {
        let now = now();
        let cfg = en();
        let mut rt = Runtime::blank();
        rt.history = log_of(&[(now - 3600, 20.0), (now, 40.0)], true);
        let w = window(now, 40.0, 2 * 3600, 0, Source::Api);

        assert_eq!(
            rate_text(&cfg, &rt, w, pace::FIVE_HOUR_SECS, now + 2 * 3600, now),
            " · 20%/h · lasts to the reset"
        );
    }

    #[test]
    fn the_weekly_rate_is_said_per_day() {
        let now = now();
        let cfg = en();
        let mut rt = Runtime::blank();
        rt.history = log_of(&[(now - 86400, 10.0), (now, 20.0)], false);
        let w = window(now, 20.0, 2 * 86400, 0, Source::Api);

        assert_eq!(
            rate_text(&cfg, &rt, w, pace::SEVEN_DAY_SECS, now + 2 * 86400, now),
            " · 10%/day · lasts to the reset"
        );
    }

    #[test]
    fn too_little_history_produces_no_rate_rather_than_a_bad_one() {
        let now = now();
        let cfg = Config::default();
        let mut rt = Runtime::blank();
        let w = window(now, 40.0, 2 * 3600, 0, Source::Api);

        // Nothing logged at all.
        assert_eq!(rate_text(&cfg, &rt, w, pace::FIVE_HOUR_SECS, now + 2 * 3600, now), "");

        // Two readings a minute apart: a forecast off that would be theatre.
        rt.history = log_of(&[(now - 60, 39.0), (now, 40.0)], true);
        assert_eq!(rate_text(&cfg, &rt, w, pace::FIVE_HOUR_SECS, now + 2 * 3600, now), "");
    }

    #[test]
    fn the_rate_is_silent_when_it_is_switched_off() {
        let now = now();
        let cfg = quiet(Config::default());
        let mut rt = Runtime::blank();
        rt.history = log_of(&[(now - 3600, 0.0), (now, 40.0)], true);
        let w = window(now, 40.0, 2 * 3600, 0, Source::Api);

        assert_eq!(rate_text(&cfg, &rt, w, pace::FIVE_HOUR_SECS, now + 2 * 3600, now), "");
    }

    // ---- the detail panel, word for word -----------------------------------

    /// The golden snapshot: the whole panel on fixed data, as strings.
    ///
    /// This is the insurance for the move to another GUI. The front end
    /// changes; these four lines must not, and a diff here is the difference
    /// between a port and a rewrite.
    #[test]
    fn the_panel_reads_exactly_this() {
        let now = now();
        let cfg = quiet(Config::default());
        let (snap, rt) = fixture(now);

        let lines = detail_lines(&cfg, &rt, &snap, false, now);
        let got: Vec<(&str, &str)> =
            lines.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();

        assert_eq!(
            got,
            vec![
                (
                    "5h",
                    "24% потрачено · 76% осталось · сброс Пт 15:37 · ▼1.1× 4% \
                     по графику · usage API"
                ),
                (
                    "7d",
                    "54% потрачено · 46% осталось · сброс Пн 17:00 · 1.0× 0% \
                     по графику · usage API · можно 15%/день"
                ),
                (
                    "ctx",
                    "38% потрачено · 382k из 1.0M (оценка) · opus-5 · abcdef01 \
                     · transcript 1:30"
                ),
                ("ист", "источники: sl — + API 0:12 + app — + tr 1:30"),
            ]
        );
    }

    #[test]
    fn the_panel_says_both_readings_with_the_pill_s_one_first() {
        // A15: the pill has room for one convention, so this is where the two
        // are reconciled — and the one on the pill leads, so the eye lands on
        // the figure it just left.
        let now = now();
        let mut cfg = quiet(en());
        let (snap, rt) = fixture(now);

        cfg.pct_mode = PctMode::Spent;
        let spent = detail_lines(&cfg, &rt, &snap, false, now);
        assert!(line(&spent, "5h").starts_with("24% spent · 76% left"));

        cfg.pct_mode = PctMode::Left;
        let left = detail_lines(&cfg, &rt, &snap, false, now);
        assert!(line(&left, "5h").starts_with("76% left · 24% spent"));
    }

    #[test]
    fn a_guessed_window_size_is_marked_and_a_stated_one_is_not() {
        // A18: a denominator wrong by five times is invisible in a percentage
        // and obvious in `382k of 1.0M`, so the panel prints the division —
        // and says when the divisor was a guess.
        let now = now();
        let cfg = quiet(Config::default());
        let (mut snap, rt) = fixture(now);

        assert!(line(&detail_lines(&cfg, &rt, &snap, false, now), "ctx").contains("(оценка)"));

        snap.context.as_mut().unwrap().limit_stated = true;
        assert!(!line(&detail_lines(&cfg, &rt, &snap, false, now), "ctx").contains("(оценка)"));
    }

    #[test]
    fn a_silent_source_is_said_rather_than_left_blank() {
        let now = now();
        let cfg = en();
        let rt = Runtime::blank();
        let snap = Snapshot::default();

        let lines = detail_lines(&cfg, &rt, &snap, false, now);
        assert_eq!(line(&lines, "5h"), "no data");
        assert_eq!(line(&lines, "7d"), "no data");
        // No context at all means no line, rather than a line saying nothing.
        assert!(lines.iter().all(|(l, _)| l != "ctx"));
        assert_eq!(line(&lines, "src"), "sources: sl — + API — + app — + tr —");
    }

    #[test]
    fn a_line_per_source_replaces_the_ages_on_the_src_line() {
        // `--sources` without leaving the screen. The ages move into those
        // lines, so printing them twice is exactly what this switch avoids.
        let now = now();
        let cfg = Config { detail_all_sources: true, ..quiet(en()) };
        let (snap, rt) = fixture(now);

        let lines = detail_lines(&cfg, &rt, &snap, false, now);
        assert_eq!(line(&lines, "API"), "24% / 54% · 0:12");
        assert_eq!(line(&lines, "tr"), "ctx 38% 382k · 1:30");
        assert_eq!(line(&lines, "sl"), "no data");
        assert_eq!(line(&lines, "src"), "sources: sl + API + app + tr");
    }

    #[test]
    fn a_stale_reading_says_so_on_the_src_line() {
        let now = now();
        let cfg = quiet(Config::default());
        let (snap, rt) = fixture(now);

        assert!(line(&detail_lines(&cfg, &rt, &snap, true, now), "ист").ends_with(" · устарело"));
        assert!(!line(&detail_lines(&cfg, &rt, &snap, false, now), "ист").ends_with(" · устарело"));
    }

    #[test]
    fn a_window_without_a_reset_time_says_that_instead_of_guessing() {
        // The desktop app's history reports percentages and nothing else
        // (A08): no countdown, no pace verdict, and the panel has to be honest
        // about it rather than inventing a clock.
        let now = now();
        let cfg = en();
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

        assert_eq!(
            line(&detail_lines(&cfg, &rt, &snap, false, now), "5h"),
            "24% spent · 76% left · reset time unknown · desktop app"
        );
    }

    #[test]
    fn demo_names_itself_rather_than_its_sources() {
        let now = now();
        let cfg = Config { demo: true, ..quiet(Config::default()) };
        let (snap, rt) = fixture(now);

        assert_eq!(line(&detail_lines(&cfg, &rt, &snap, false, now), "ист"), "источники: демо");
    }
}
