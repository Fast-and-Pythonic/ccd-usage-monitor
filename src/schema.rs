//! Every setting described once: what kind of value it is, where it lives in
//! the file, what to call it and what it does.
//!
//! Two jobs, and they are the reason this exists as data rather than as code.
//!
//! **It lays out `config.toml`.** `Section` is the table a key is written
//! into, so the file reads in groups instead of as forty-five lines in the
//! order a struct happens to declare them. Nothing else knows that mapping;
//! `config::save` and `config::load` ask here.
//!
//! **It will generate the settings form.** The window that exists today is
//! 1300 lines of egui in which the label, the tooltip, the step size and the
//! layout are braided together, and every one of those settings is described
//! four times over. When the GUI moves to Slint (A20) that window is rebuilt
//! from nothing, and rebuilding it by hand would reproduce the same 1300 lines
//! in a second language. A form walked out of this list is a repeater over
//! three kinds of row.
//!
//! **The bounds here must agree with the accessors in `config.rs`**, which are
//! what actually enforce them — a hand-edited file is clamped when it is read,
//! not when it is written. A test checks that every bound in this list
//! survives its own accessor, so the two cannot drift apart silently.

// The form that reads the labels, tips and steps below arrives with the Slint
// settings window; the file layout already uses `key` and `section`. Harvesting
// the rest now is deliberate: these strings exist only inside `settings_body`,
// which that same stage deletes.
#![allow(dead_code)]

use crate::config::Config;

/// A table in `config.toml`. The order is the order the sections are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    /// How the pill is drawn. Dies with the move to Slint (A20): a theme file
    /// takes it over, and only `scale` and `opacity` stay behind as the two
    /// knobs worth having without learning `.slint`.
    Style,
    /// What the widget says rather than how it looks — the display
    /// conventions. `view.rs` reads these, so they survive any front end and
    /// `--serve` publishes them (A21).
    Display,
    /// Which sources are on, and how quickly each one goes stale (A14).
    Sources,
    /// The numbers that decide a colour or an expiry (A07, A15, A16).
    Thresholds,
    /// Everything else the widget does rather than shows.
    Behaviour,
}

impl Section {
    pub const ALL: [Section; 5] = [
        Section::Sources,
        Section::Behaviour,
        Section::Display,
        Section::Thresholds,
        Section::Style,
    ];

    /// The table name in the file.
    pub fn key(self) -> &'static str {
        match self {
            Section::Style => "style",
            Section::Display => "display",
            Section::Sources => "sources",
            Section::Thresholds => "thresholds",
            Section::Behaviour => "behaviour",
        }
    }

    pub fn title(self, ru: bool) -> &'static str {
        match (self, ru) {
            (Section::Style, true) => "Вид",
            (Section::Style, false) => "Appearance",
            (Section::Display, true) => "Что показывать",
            (Section::Display, false) => "What to show",
            (Section::Sources, true) => "Источники",
            (Section::Sources, false) => "Sources",
            (Section::Thresholds, true) => "Пороги",
            (Section::Thresholds, false) => "Thresholds",
            (Section::Behaviour, true) => "Поведение",
            (Section::Behaviour, false) => "Behaviour",
        }
    }
}

/// How a value is written down, once it has been read.
///
/// Separate from its type: `opacity` is a float from 0 to 1 and reads as a
/// percentage, `bridge_stale_secs` is a count of seconds and reads as `15:00`.
/// A form that did not know this would show either the raw number or nothing
/// useful.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Show {
    Plain,
    /// × 100, with a `%`.
    Percent,
    /// Points of a percentage — already a percentage, so no multiplication.
    Points,
    /// `15:00`, through `pace::fmt_duration`.
    Duration,
    /// `200k`, through `pace::fmt_tokens`.
    Tokens,
}

#[derive(Clone, Copy, Debug)]
pub enum Kind {
    Bool,
    Int {
        min: i64,
        max: i64,
        /// A nudge, and a jump. Two sizes because the gestures differ: tune a
        /// threshold by one, hunt for a size in fours.
        step: i64,
        big: i64,
        show: Show,
    },
    Float {
        min: f32,
        max: f32,
        step: f32,
        big: f32,
        show: Show,
    },
    /// The options, in the order a menu should offer them.
    Choice(&'static [Choice]),
    Text,
}

/// One option of a `Choice`: the spelling the file uses, and what to call it
/// in a menu.
///
/// The two live together because they are one fact. They used to be apart —
/// the spellings here, the wording on `label(ru)` methods in `config.rs` that
/// only the old settings window called — and a form generated from this list
/// would have had to reach across for half of what it needed.
#[derive(Clone, Copy, Debug)]
pub struct Choice {
    pub value: &'static str,
    pub label: Words,
}

const fn ch(value: &'static str, ru: &'static str, en: &'static str) -> Choice {
    Choice { value, label: Words { ru, en } }
}

/// Russian first, because that is the default language of the widget.
#[derive(Clone, Copy, Debug)]
pub struct Words {
    pub ru: &'static str,
    pub en: &'static str,
}

impl Words {
    pub fn get(self, ru: bool) -> &'static str {
        if ru { self.ru } else { self.en }
    }
}

pub struct Setting {
    /// The key as it is spelled in the file and in `Config`. The two are the
    /// same on purpose: moving a key into a table is a change of position, not
    /// of name, so no value can be lost in translation.
    pub key: &'static str,
    pub section: Section,
    pub kind: Kind,
    pub label: Words,
    /// What it actually does. The rows are as short as a stepper leaves them,
    /// so the label can only name a setting.
    pub tip: Words,
    /// Whether a generated form should carry a row for it. `false` for the two
    /// that something else owns: the session id belongs to the picker, and the
    /// demo switch is a flag for working on the layout.
    pub form: bool,
}

const fn w(ru: &'static str, en: &'static str) -> Words {
    Words { ru, en }
}

const SPENT_LEFT: &[Choice] = &[
    ch("spent", "потрачено", "spent"),
    ch("left", "осталось", "left"),
];

pub const ALL: &[Setting] = &[
    // ------------------------------------------------------------- sources ---
    Setting {
        key: "bridge_enabled",
        section: Section::Sources,
        kind: Kind::Bool,
        label: w("statusLine", "statusLine"),
        tip: w(
            "Данные от команды statusLine самого Claude Code. Идут, только пока \
             открыта сессия в терминале; зато размер окна контекста называет \
             только он.",
            "Figures from Claude Code's own status-line command. Only while a \
             terminal session is running — but it is the only source that states \
             the size of the context window.",
        ),
        form: true,
    },
    Setting {
        key: "api_enabled",
        section: Section::Sources,
        kind: Kind::Bool,
        label: w("usage API", "usage API"),
        tip: w(
            "Прямой запрос к серверу Anthropic токеном Claude Code. Единственный \
             источник, который знает время сброса.",
            "A direct request to Anthropic with Claude Code's token. The only \
             source that knows the reset times.",
        ),
        form: true,
    },
    Setting {
        key: "desktop_enabled",
        section: Section::Sources,
        kind: Kind::Bool,
        label: w("Приложение", "Desktop app"),
        tip: w(
            "Файл, который ведёт само приложение Claude. Ничего не требует, но \
             пишется раз в ~15 минут и без времени сброса.",
            "The history file the Claude desktop app keeps. Asks for nothing, but \
             is written every ~15 min and carries no reset times.",
        ),
        form: true,
    },
    Setting {
        key: "session_enabled",
        section: Section::Sources,
        kind: Kind::Bool,
        label: w("Транскрипт сессии", "Session transcript"),
        tip: w(
            "Читает файлы разговоров, которые Claude Code пишет сам, в \
             ~/.claude/projects. Работает и в приложении, где строка состояния не \
             запускается. Число токенов точное, а вот размер окна приходится \
             выводить по модели.",
            "Reads the conversation files Claude Code writes for itself under \
             ~/.claude/projects. Works in the desktop app, where the status line \
             never runs. The token count is exact; the window size has to be \
             inferred from the model.",
        ),
        form: true,
    },
    Setting {
        key: "bridge_stale_secs",
        section: Section::Sources,
        kind: Kind::Int { min: 30, max: 86_400, step: 60, big: 300, show: Show::Duration },
        label: w("statusLine устаревает через", "statusLine stale after"),
        tip: STALE_TIP,
        form: true,
    },
    Setting {
        key: "api_stale_secs",
        section: Section::Sources,
        kind: Kind::Int { min: 30, max: 86_400, step: 60, big: 300, show: Show::Duration },
        label: w("usage API устаревает через", "usage API stale after"),
        tip: STALE_TIP,
        form: true,
    },
    Setting {
        key: "desktop_stale_secs",
        section: Section::Sources,
        kind: Kind::Int { min: 30, max: 86_400, step: 60, big: 300, show: Show::Duration },
        label: w("Приложение устаревает через", "Desktop app stale after"),
        tip: STALE_TIP,
        form: true,
    },
    Setting {
        key: "session_stale_secs",
        section: Section::Sources,
        kind: Kind::Int { min: 30, max: 86_400, step: 60, big: 300, show: Show::Duration },
        label: w("Транскрипт устаревает через", "Transcript stale after"),
        tip: STALE_TIP,
        form: true,
    },
    Setting {
        key: "api_interval_secs",
        section: Section::Sources,
        kind: Kind::Int { min: 30, max: 3_600, step: 10, big: 60, show: Show::Duration },
        label: w("Опрашивать каждые", "Poll every"),
        tip: w(
            "Как часто виджет сам ходит за цифрами в фоне. Реже — меньше запросов, \
             но цифры дольше остаются старыми.",
            "How often the widget fetches in the background. Rarer means fewer \
             requests but older figures.",
        ),
        form: true,
    },
    Setting {
        key: "api_budget_per_min",
        section: Section::Sources,
        kind: Kind::Int { min: 1, max: 30, step: 1, big: 5, show: Show::Plain },
        label: w("Запросов в минуту", "Requests per min"),
        tip: w(
            "Потолок, который виджет держит сам, чтобы сервер не ответил отказом. \
             Сверх него запрос не отменяется, а откладывается на несколько секунд.",
            "A cap the widget keeps for itself so the server never has to refuse. \
             Past it a request is delayed by a few seconds, not dropped.",
        ),
        form: true,
    },
    // ----------------------------------------------------------- behaviour ---
    Setting {
        key: "session_pick",
        section: Section::Behaviour,
        kind: Kind::Choice(&[
            ch("live", "запущенная", "running"),
            ch("freshest", "свежайшая", "freshest"),
            ch("pinned", "выбранная", "pinned"),
        ]),
        label: w("Какая сессия", "Which session"),
        tip: w(
            "Запущенная — из тех, что Claude Code сейчас считает живыми; если таких \
             нет, поле молчит, а не подставляет старую. Свежайшая — тот разговор, в \
             который писали последним, где бы он ни был. Выбранная — та, что \
             указана ниже.",
            "Running — one Claude Code currently lists as alive; with none the \
             field goes quiet rather than falling back to an old one. Freshest — \
             whichever transcript was written to last, wherever it is. Pinned — \
             the one chosen below.",
        ),
        form: true,
    },
    Setting {
        key: "session_id",
        section: Section::Behaviour,
        kind: Kind::Text,
        label: w("Разговор", "Conversation"),
        tip: w(
            "Запоминается id разговора. Если его файл исчезнет, поле замолчит — \
             виджет не станет молча показывать соседний.",
            "The conversation's id is remembered. If its file goes, the field goes \
             quiet — the widget will not quietly show a neighbour instead.",
        ),
        // The picker owns this one: eighty-odd transcripts is a list to choose
        // from, not a uuid to type.
        form: false,
    },
    Setting {
        key: "session_scan_secs",
        section: Section::Behaviour,
        kind: Kind::Int { min: 5, max: 600, step: 10, big: 60, show: Show::Duration },
        label: w("Пересматривать список", "Rescan the list"),
        tip: w(
            "Как часто виджет заново перечитывает папку разговоров. Между \
             просмотрами он смотрит только на один выбранный файл, так что это \
             почти ничего не стоит.",
            "How often the widget enumerates the transcripts again. Between scans \
             it stats one chosen file, so this costs next to nothing either way.",
        ),
        form: true,
    },
    Setting {
        key: "ctx_limit",
        section: Section::Behaviour,
        kind: Kind::Choice(&[
            ch("learned", "из statusLine", "as stated"),
            ch("table", "по таблице", "from the table"),
            ch("fixed", "вручную", "by hand"),
        ]),
        label: w("Размер окна", "Window size"),
        tip: w(
            "На что делится число токенов. У моделей пятой серии окно — миллион, у \
             прежних — двести тысяч, и ошибка здесь это разница между 9% и 44% для \
             одного и того же разговора. statusLine называет размер сам: запустите \
             Claude Code в терминале хотя бы раз, и он запомнится.",
            "What the token count is divided by. The 5-series has a million-token \
             window and everything before it two hundred thousand, so getting this \
             wrong is the difference between 9% and 44% for one conversation. The \
             status line states the size itself: run Claude Code in a terminal once \
             and it is remembered.",
        ),
        form: true,
    },
    Setting {
        key: "ctx_limit_tokens",
        section: Section::Behaviour,
        kind: Kind::Int {
            min: 8_192,
            max: 1_000_000,
            step: 8_000,
            big: 200_000,
            show: Show::Tokens,
        },
        label: w("Окно вручную", "Window by hand"),
        tip: w(
            "Знаменатель вручную. Нужен там, где модель держат в меньшем окне \
             переменной среды: на диске это никак не отражается, и угадать неоткуда.",
            "The denominator by hand. Needed when a model is held to a smaller \
             window by an environment variable: nothing on disk says so.",
        ),
        form: true,
    },
    Setting {
        key: "count_today",
        section: Section::Behaviour,
        kind: Kind::Bool,
        label: w("Считать сегодня", "Count today"),
        tip: w(
            "Входит ли текущий день в счётчик дней и в дневную норму. Сегодня \
             обычно уже наполовину прожито, поэтому по умолчанию не считается.",
            "Whether the current day counts towards the day figure and the daily \
             allowance. Today is usually half spent by the time you look, so it \
             does not by default.",
        ),
        form: true,
    },
    Setting {
        key: "count_reset_day",
        section: Section::Behaviour,
        kind: Kind::Bool,
        label: w("Считать день сброса", "Count reset day"),
        tip: w(
            "Входит ли день, в который квота обновляется. Он стоит только тех \
             часов, что до сброса: если сброс в 7 утра, этот день лучше выключить.",
            "Whether the day the quota resets counts. It is worth only the hours \
             before the reset: with a reset at 07:00 that day is better left out.",
        ),
        form: true,
    },
    Setting {
        key: "click_through",
        section: Section::Behaviour,
        kind: Kind::Bool,
        label: w("Клики насквозь", "Click-through"),
        tip: w(
            "Пилюля перестаёт ловить мышь: клики уходят в окно под ней. Вернуть — \
             из меню в трее.",
            "The pill stops taking the mouse; clicks go to whatever is underneath. \
             Turn it back from the tray menu.",
        ),
        form: true,
    },
    Setting {
        key: "lang",
        section: Section::Behaviour,
        kind: Kind::Choice(&[ch("ru", "Русский", "Russian"), ch("en", "Английский", "English")]),
        label: w("Язык", "Language"),
        tip: w(
            "Язык подписей и панели подробностей: русский или английский.",
            "Language of the labels and the detail panel: Russian or English.",
        ),
        form: true,
    },
    Setting {
        key: "demo",
        section: Section::Behaviour,
        kind: Kind::Bool,
        label: w("Демо-данные", "Demo figures"),
        tip: w(
            "Синтетические цифры, которые стоят на месте, — чтобы работать над \
             раскладкой. То же даёт запуск с `--demo`.",
            "Synthetic figures that sit still, for working on the layout. The same \
             as starting with `--demo`.",
        ),
        // A flag for working on the widget, not a setting for using it.
        form: false,
    },
    // ------------------------------------------------------------- display ---
    Setting {
        key: "pct_mode",
        section: Section::Display,
        kind: Kind::Choice(SPENT_LEFT),
        label: w("Проценты", "Percentages"),
        tip: w(
            "Что показывают большие проценты и полоски под ними: сколько квоты уже \
             потрачено или сколько её осталось. Цвет в обоих случаях считается по \
             расходу, поэтому «10% осталось» красное.",
            "What the big percentages and the bars under them show: how much of the \
             quota is spent, or how much is left. The colour is keyed on spending \
             either way, so 10% left shows red.",
        ),
        form: true,
    },
    Setting {
        key: "pace_view",
        section: Section::Display,
        // Shown as themselves: the label *is* the thing, so a name for it
        // would be a second, worse description of what the eye can read.
        kind: Kind::Choice(&[
            ch("ratio", "▲2.0×", "▲2.0×"),
            ch("points", "▲9%", "▲9%"),
            ch("ratio_points", "▲2.0× 9%", "▲2.0× 9%"),
            ch("points_ratio", "▲9% 2.0×", "▲9% 2.0×"),
            ch("split", "▲9% | 2.0×", "▲9% | 2.0×"),
            ch("piped", "▲9%|2.0×", "▲9%|2.0×"),
            ch("off", "выкл", "off"),
        ]),
        label: w("Темп", "Pace"),
        tip: w(
            "Что стоит рядом с каждым лимитом на пилюле. Пункты — на сколько расход \
             обогнал часы; кратность — во сколько раз придётся замедлиться.",
            "What stands beside each limit on the pill. Points are how far spending \
             has run ahead of the clock; the ratio is how many times slower the \
             rest has to go.",
        ),
        form: true,
    },
    Setting {
        key: "bar_colour",
        section: Section::Display,
        kind: Kind::Choice(&[
            ch("fill", "по расходу", "by fill"),
            ch("pace", "по темпу", "by pace"),
        ]),
        label: w("Цвет баров", "Bar colour"),
        tip: w(
            "По расходу — краснеет к концу квоты. По темпу — краснеет, когда \
             тратишь быстрее часов, даже если потрачено мало. Короткий красный бар \
             значит: квота есть, времени нет.",
            "By fill it reddens as the window runs out. By pace it reddens when \
             spending outruns the clock, however little is gone. A short red bar \
             means quota left and no time to use it.",
        ),
        form: true,
    },
    Setting {
        key: "pace_rate",
        section: Section::Display,
        kind: Kind::Bool,
        label: w("Скорость и прогноз", "Rate and forecast"),
        tip: w(
            "В панели: сколько процентов в час уходит сейчас и когда при таком темпе \
             упрёмся в потолок. Считается по журналу замеров, который виджет ведёт \
             сам.",
            "In the panel: how many percent an hour are going now, and when that \
             runs into the ceiling. Read from the log of readings the widget keeps.",
        ),
        form: true,
    },
    Setting {
        key: "detail_mode",
        section: Section::Display,
        kind: Kind::Choice(&[
            ch("with_settings", "с настройками", "with settings"),
            ch("hover", "при наведении", "on hover"),
            ch("never", "никогда", "never"),
        ]),
        label: w("Панель подробностей", "Detail panel"),
        tip: w(
            "Когда показывать панель под пилюлей: пока открыто окно настроек, при \
             наведении мыши на пилюлю, или никогда.",
            "When the panel under the pill appears: while the settings window is \
             open, on hovering the pill, or never.",
        ),
        form: true,
    },
    Setting {
        key: "detail_all_sources",
        section: Section::Display,
        kind: Kind::Bool,
        label: w("Строка на источник", "Line per source"),
        tip: w(
            "Добавляет в панель по строке на каждый включённый источник — то же, что \
             печатает `ccd-usage-monitor --sources`.",
            "Adds a line per enabled source to the panel — the same thing \
             `ccd-usage-monitor --sources` prints.",
        ),
        form: true,
    },
    Setting {
        key: "ctx_view",
        section: Section::Display,
        kind: Kind::Choice(&[
            ch("pct", "14%", "14%"),
            ch("tokens", "142k", "142k"),
            ch("pct_tokens", "14% 142k", "14% 142k"),
            ch("tokens_pct", "142k 14%", "142k 14%"),
            ch("off", "выкл", "off"),
        ]),
        label: w("Контекст", "Context"),
        tip: w(
            "Что стоит в группе ctx: доля окна, число токенов или оба подряд. Токены \
             не зависят от угаданного размера окна, поэтому их стоит держать рядом с \
             процентом, пока нет доверия к знаменателю.",
            "What the ctx group shows: the share of the window, the token count, or \
             both. The count owes nothing to a guessed window size, so it is worth \
             keeping beside the percentage until the denominator is trusted.",
        ),
        form: true,
    },
    Setting {
        key: "ctx_dir_linked",
        section: Section::Display,
        kind: Kind::Bool,
        label: w("Направление ctx вместе", "Ctx direction together"),
        tip: w(
            "Включено — процент и токены ctx читаются в одну сторону. Выключено — у \
             каждого своя настройка: например, проценты остатка при токенах \
             заполнения.",
            "On, the ctx percentage and token count read the same direction. Off, \
             each gets its own: e.g. a remaining percentage alongside a spent token \
             count.",
        ),
        form: true,
    },
    Setting {
        key: "ctx_pct_dir",
        section: Section::Display,
        kind: Kind::Choice(SPENT_LEFT),
        label: w("Ctx проценты", "Ctx percent"),
        tip: CTX_DIR_TIP,
        form: true,
    },
    Setting {
        key: "ctx_tokens_dir",
        section: Section::Display,
        kind: Kind::Choice(SPENT_LEFT),
        label: w("Ctx токены", "Ctx tokens"),
        tip: CTX_DIR_TIP,
        form: true,
    },
    // ---------------------------------------------------------- thresholds ---
    Setting {
        key: "warn_pct",
        section: Section::Thresholds,
        kind: Kind::Float { min: 0.0, max: 100.0, step: 1.0, big: 5.0, show: Show::Points },
        label: w("Янтарный с", "Amber from"),
        tip: w(
            "С какого расхода цифра начинает уходить из зелёного в янтарный. \
             Считается всегда по потраченному, даже когда на пилюле стоит остаток.",
            "The figure at which a reading starts moving from green to amber. Read \
             off spending always, even when the pill is showing what is left.",
        ),
        form: true,
    },
    Setting {
        key: "danger_pct",
        section: Section::Thresholds,
        kind: Kind::Float { min: 0.0, max: 100.0, step: 1.0, big: 5.0, show: Show::Points },
        label: w("Красный с", "Red from"),
        tip: w(
            "С какого расхода цифра идёт из янтарного в красный, добираясь до \
             чистого красного к сотне.",
            "The figure at which a reading moves from amber to red, reaching full \
             red at a hundred.",
        ),
        form: true,
    },
    Setting {
        key: "pace_tolerance",
        section: Section::Thresholds,
        kind: Kind::Float { min: 0.0, max: 50.0, step: 1.0, big: 5.0, show: Show::Points },
        label: w("Допуск темпа", "Pace tolerance"),
        tip: w(
            "Насколько можно отставать от часов и всё ещё считаться идущим по \
             графику. Работает только ниже линии: перерасход есть перерасход, и \
             цвет за линией начинается сразу с оранжевого.",
            "How far under the clock still counts as on pace. It applies below the \
             line only: over is over, and the colour past the line starts at orange \
             rather than at green.",
        ),
        form: true,
    },
    Setting {
        key: "drop_after_secs",
        section: Section::Thresholds,
        kind: Kind::Int { min: 60, max: 86_400, step: 300, big: 3_600, show: Show::Duration },
        label: w("Прятать показание через", "Drop a reading after"),
        tip: w(
            "Через сколько показание перестаёт показываться вовсе, а не просто \
             сереет. Пятичасовой процент часовой давности — это не приблизительное \
             значение, это другое окно.",
            "After this a reading is not shown at all, rather than merely greyed. A \
             five-hour percentage from an hour ago is not an approximation of the \
             present, it is a different window.",
        ),
        form: true,
    },
    // --------------------------------------------------------------- style ---
    Setting {
        key: "scale",
        section: Section::Style,
        kind: Kind::Float { min: 0.6, max: 3.0, step: 0.01, big: 0.05, show: Show::Percent },
        label: w("Общий масштаб", "Overall scale"),
        tip: w(
            "Множитель для всего сразу: шрифтов, высоты, отступов. Первое, что стоит \
             крутить на другом мониторе.",
            "One multiplier for the fonts, the height and the paddings alike. The \
             knob to reach for on a different monitor.",
        ),
        form: true,
    },
    Setting {
        key: "opacity",
        section: Section::Style,
        kind: Kind::Float { min: 0.3, max: 1.0, step: 0.01, big: 0.05, show: Show::Percent },
        label: w("Непрозрачность", "Opacity"),
        tip: w(
            "Насколько плотная подложка у пилюли и панели. Текст и цифры не бледнеют.",
            "How solid the background of the pill and the panel is. The text and \
             the figures stay as they are.",
        ),
        form: true,
    },
    Setting {
        key: "font_pct",
        section: Section::Style,
        kind: Kind::Float { min: 6.0, max: 48.0, step: 0.5, big: 2.0, show: Show::Plain },
        label: w("Кегль процентов", "Percent font"),
        tip: w(
            "Кегль крупных процентов на пилюле.",
            "Point size of the large percentages on the pill.",
        ),
        form: true,
    },
    Setting {
        key: "font_time",
        section: Section::Style,
        kind: Kind::Float { min: 6.0, max: 48.0, step: 0.5, big: 2.0, show: Show::Plain },
        label: w("Кегль времени", "Time font"),
        tip: w(
            "Кегль обратного отсчёта, времени сброса, числа дней и дневной нормы.",
            "Point size of the countdown, the reset clock, the day count and the \
             daily allowance.",
        ),
        form: true,
    },
    Setting {
        key: "pill_height",
        section: Section::Style,
        kind: Kind::Float { min: 18.0, max: 120.0, step: 1.0, big: 4.0, show: Show::Plain },
        label: w("Высота пилюли", "Pill height"),
        tip: w(
            "Высота самой пилюли. Если содержимому мало места, она вырастет сама.",
            "Height of the pill itself. It grows on its own when the contents need \
             more room.",
        ),
        form: true,
    },
    Setting {
        key: "track_height",
        section: Section::Style,
        kind: Kind::Float { min: 0.0, max: 16.0, step: 0.5, big: 2.0, show: Show::Plain },
        label: w("Толщина полосок", "Bar thickness"),
        tip: w(
            "Толщина полосок расхода под цифрами. 0 убирает их совсем.",
            "Thickness of the usage bars under the figures. Zero hides them.",
        ),
        form: true,
    },
    Setting {
        key: "pad_top",
        section: Section::Style,
        kind: Kind::Float { min: 0.0, max: 40.0, step: 0.5, big: 2.0, show: Show::Plain },
        label: w("Над цифрами", "Above figures"),
        tip: w(
            "Отступ между верхом пилюли и строкой цифр.",
            "Space between the top of the pill and the row of figures.",
        ),
        form: true,
    },
    Setting {
        key: "gap_text_bars",
        section: Section::Style,
        kind: Kind::Float { min: 0.0, max: 40.0, step: 0.5, big: 2.0, show: Show::Plain },
        label: w("Цифры и бары", "Figures to bars"),
        tip: w(
            "Просвет между цифрами и полосками расхода.",
            "The gap between the figures and the usage bars.",
        ),
        form: true,
    },
    Setting {
        key: "pad_bottom",
        section: Section::Style,
        kind: Kind::Float { min: 0.0, max: 40.0, step: 0.5, big: 2.0, show: Show::Plain },
        label: w("Под барами", "Below bars"),
        tip: w(
            "Отступ между полосками и низом пилюли.",
            "Space between the bars and the bottom of the pill.",
        ),
        form: true,
    },
    Setting {
        key: "gaps_x",
        section: Section::Style,
        kind: Kind::Float { min: 0.2, max: 3.0, step: 0.05, big: 0.2, show: Show::Percent },
        label: w("Отступы по горизонтали", "Horizontal gaps"),
        tip: w(
            "Все горизонтальные просветы сразу: между числами, вокруг разделителей \
             групп и от края пилюли.",
            "Every horizontal gap at once: between the figures, either side of the \
             group dividers, and from the edge of the pill.",
        ),
        form: true,
    },
    Setting {
        key: "corner_radius",
        section: Section::Style,
        kind: Kind::Float { min: 0.0, max: 60.0, step: 1.0, big: 4.0, show: Show::Plain },
        label: w("Скругление углов", "Corner radius"),
        tip: w(
            "Скругление пилюли, панели и подсказок. 0 даёт прямые углы, половина \
             высоты — обратно лозенгу.",
            "How round the pill, the panel and the hints are. Zero gives square \
             corners; half the height gives back the lozenge.",
        ),
        form: true,
    },
];

const STALE_TIP: Words = w(
    "После этого срока цифры этого источника сереют, а на пилюле загорается \
     янтарная точка. У каждого источника свой срок: они стареют с очень разной \
     скоростью.",
    "Past this the figures from this source go grey and the pill shows an amber \
     dot. Each source carries its own threshold: they age at very different \
     rates.",
);

const CTX_DIR_TIP: Words = w(
    "Заполненность окна растёт к пределу, а не убывает от него, как пятичасовая и \
     недельная квоты — поэтому у ctx свой выбор, а не общий «Проценты».",
    "The window fills toward its limit instead of draining from it like the \
     5-hour and weekly quotas — so ctx gets its own choice instead of following \
     the \"Percentages\" setting.",
);

/// Which table a key belongs in. `None` for anything not described here, which
/// is only ever a legacy key on its way out — those stay at the top level so a
/// migration can find them.
pub fn section_of(key: &str) -> Option<Section> {
    ALL.iter().find(|s| s.key == key).map(|s| s.section)
}

pub fn get(key: &str) -> Option<&'static Setting> {
    ALL.iter().find(|s| s.key == key)
}

/// One setting's current value, addressed by key.
///
/// Through serde rather than through a forty-five-arm `match`: the round trip
/// is the same one `config::save` already relies on, and a `match` is a list
/// that has to be remembered, which is exactly the kind of list this module
/// exists to abolish. The cost is serialising the whole config for one read,
/// which is nothing on the scale of a click in a form.
pub fn read(cfg: &Config, key: &str) -> Option<toml::Value> {
    let toml::Value::Table(table) = toml::Value::try_from(cfg).ok()? else {
        return None;
    };
    table.get(key).cloned()
}

/// Put one value back, by key. `false` when the key is unknown or the value is
/// the wrong shape for it — in which case the config is left as it was.
///
/// Deliberately tolerant of nothing: a form that wrote a string into a number
/// would corrupt the file on the next save, and the clamping in `config.rs`
/// only defends against values of the *right* type being out of range.
pub fn write(cfg: &mut Config, key: &str, value: toml::Value) -> bool {
    let Ok(toml::Value::Table(mut table)) = toml::Value::try_from(&*cfg) else {
        return false;
    };
    if !table.contains_key(key) {
        return false;
    }
    table.insert(key.to_string(), value);
    match toml::Value::Table(table).try_into::<Config>() {
        Ok(next) => {
            *cfg = next;
            true
        }
        Err(_) => false,
    }
}

/// The keys of a `Config` as serde writes them, which is what the file and
/// this list both have to agree with.
pub fn config_keys() -> Vec<String> {
    let value = toml::Value::try_from(Config::default()).expect("a config serialises");
    match value {
        toml::Value::Table(t) => t.keys().cloned().collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_in_the_config_is_described_exactly_once() {
        // The hole this closes: a field added to `Config` and forgotten here
        // would be written to the top level of the file instead of its
        // section, and would be missing from any generated form — silently,
        // and only noticed by whoever could not find the setting.
        let mut missing = Vec::new();
        for key in config_keys() {
            // Legacy keys are read and dropped, never written; they have no
            // section and no row.
            if key == "source" || key == "pace_cell" || key == "pace_unit" {
                continue;
            }
            if get(&key).is_none() {
                missing.push(key);
            }
        }
        assert!(missing.is_empty(), "not described in the schema: {missing:?}");

        let mut seen = std::collections::BTreeSet::new();
        for s in ALL {
            assert!(seen.insert(s.key), "{} is described twice", s.key);
        }
    }

    #[test]
    fn the_schema_describes_nothing_the_config_does_not_have() {
        // The other direction: a renamed field leaves an entry here pointing
        // at a key that no longer exists, and a form would offer a setting
        // that goes nowhere.
        let keys = config_keys();
        for s in ALL {
            assert!(
                keys.iter().any(|k| k == s.key),
                "{} is in the schema but not in Config",
                s.key
            );
        }
    }

    #[test]
    fn every_bound_survives_the_accessor_that_enforces_it() {
        // The schema says what a form may offer; `config.rs` clamps what is
        // read. If they disagree, a value set from the form comes back as a
        // different number and the row appears to ignore the click.
        let mut cfg = Config::default();
        for s in ALL {
            let Kind::Float { min, max, .. } = s.kind else {
                continue;
            };
            for v in [min, max] {
                let got = match s.key {
                    "scale" => { cfg.scale = v; cfg.scale() }
                    "font_pct" => { cfg.font_pct = v; cfg.font_pct() }
                    "font_time" => { cfg.font_time = v; cfg.font_time() }
                    "pill_height" => { cfg.pill_height = v; cfg.pill_height() }
                    "track_height" => { cfg.track_height = v; cfg.track_height() }
                    "pad_top" => { cfg.pad_top = v; cfg.pad_top() }
                    "gap_text_bars" => { cfg.gap_text_bars = v; cfg.gap_text_bars() }
                    "pad_bottom" => { cfg.pad_bottom = v; cfg.pad_bottom() }
                    "gaps_x" => { cfg.gaps_x = v; cfg.gaps_x() }
                    "corner_radius" => { cfg.corner_radius = v; cfg.corner_radius() }
                    // `opacity` and the thresholds are clamped where they are
                    // used rather than by an accessor.
                    _ => continue,
                };
                assert_eq!(got, v, "{} clamps its own stated bound {v}", s.key);
            }
        }
    }

    #[test]
    fn every_choice_is_a_spelling_the_config_can_read() {
        // A choice list is what a form offers and what lands in the file, so
        // each entry has to deserialise into the field it belongs to.
        for s in ALL {
            let Kind::Choice(options) = s.kind else {
                continue;
            };
            assert!(!options.is_empty(), "{} offers no choices", s.key);
            for opt in options {
                assert!(
                    !opt.label.ru.is_empty() && !opt.label.en.is_empty(),
                    "{}: `{}` has no name in a menu",
                    s.key,
                    opt.value
                );
                let line = format!("{} = \"{}\"", s.key, opt.value);
                let parsed: Result<Config, _> = toml::from_str(&line);
                assert!(
                    parsed.is_ok(),
                    "{}: `{}` is not a spelling the config accepts",
                    s.key,
                    opt.value
                );
            }
        }
    }

    #[test]
    fn every_setting_says_something_in_both_languages() {
        for s in ALL {
            for ru in [true, false] {
                assert!(!s.label.get(ru).is_empty(), "{} has no label", s.key);
                assert!(!s.tip.get(ru).is_empty(), "{} has no tip", s.key);
                // The label names a setting; the tip is what it does. A label
                // long enough to be a sentence means they were confused.
                assert!(
                    s.label.get(ru).chars().count() <= 32,
                    "{}: the label is a sentence: {}",
                    s.key,
                    s.label.get(ru)
                );
            }
        }
    }

    #[test]
    fn a_run_of_spaces_in_a_tip_is_a_broken_line_continuation() {
        // Rust eats the newline *and* the indentation after a trailing `\`.
        // Without it the indentation lands in the string, and the tooltip
        // renders with a gulf in the middle of a sentence — which is how the
        // "Percentages" tip shipped.
        for s in ALL {
            for text in [s.label.ru, s.label.en, s.tip.ru, s.tip.en] {
                assert!(
                    !text.contains("  "),
                    "{}: a run of spaces, i.e. a missing `\\`: {text}",
                    s.key
                );
            }
        }
    }
}
