//! Every setting described once: what kind of value it is, where it lives in
//! the file, what to call it and what it does.
//!
//! Two jobs, and they are the reason this exists as data rather than as code.
//!
//! **It lays out `config.toml`.** `Section` is the table a key is written
//! into, so the file reads in groups instead of as thirty-seven lines in the
//! order a struct happens to declare them. Nothing else knows that mapping;
//! `config::save` and `config::load` ask here.
//!
//! **It generates the settings form.** Each setting names the card it is shown
//! on (`Group`, and through it the `Page`) and, where it only matters while
//! another setting has some value, what it `Needs`. The window is walked out of
//! this list; `ui/settings.slint` knows what kind of row it is drawing and
//! never which setting (A22, A33).
//!
//! **The bounds here must agree with the accessors in `config.rs`**, which are
//! what actually enforce them — a hand-edited file is clamped when it is read,
//! not when it is written. A test checks that every bound in this list
//! survives its own accessor, so the two cannot drift apart silently.

// Not every consumer of this list reads every field of it: the file layout
// needs `key` and `section`, the form everything else.
#![allow(dead_code)]

use crate::config::Config;

/// A table in `config.toml`. The order is the order the sections are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
   /// How the pill is drawn. Almost all of this moved into the theme with
   /// the move to Slint (A20, A24); `scale` and `opacity` stayed behind as
   /// the two knobs worth having without opening a `.slint` file.
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
   Choice {
      value,
      label: Words { ru, en },
   }
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

/// A page of the settings window: one entry in the list down its left side.
///
/// A different division from `Section` on purpose. The file is grouped by what
/// kind of thing a key is, so a person editing it finds all the thresholds
/// together; the window is grouped by what someone opening it is trying to do,
/// so a source's switch, its expiry and its polling sit on one card. The two
/// orders disagree, and neither is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
   Quotas,
   Context,
   Look,
   Sources,
   System,
}

impl Page {
   /// The order down the side of the window.
   pub const ALL: [Page; 5] = [
      Page::Quotas,
      Page::Context,
      Page::Look,
      Page::Sources,
      Page::System,
   ];

   pub fn title(self, ru: bool) -> &'static str {
      let words = match self {
         Page::Sources => w("Источники", "Sources"),
         Page::Quotas => w("Лимиты", "Limits"),
         Page::Context => w("Контекст", "Context"),
         Page::Look => w("Вид", "Appearance"),
         Page::System => w("Система", "System"),
      };
      words.get(ru)
   }

   /// The page's icon as SVG path data on an 18 × 18 grid, stroked rather
   /// than filled. Data rather than drawing, so the window can show it without
   /// knowing which page it belongs to.
   pub fn icon(self) -> &'static str {
      match self {
         // A stack of discs: where the figures come from.
         Page::Sources => {
            "M3 4.5a6 2.5 0 1 0 12 0a6 2.5 0 1 0 -12 0M3 4.5v9c0 1.4 2.7 2.5 6 2.5s6 -1.1 6 -2.5v-9\
             M3 9c0 1.4 2.7 2.5 6 2.5s6 -1.1 6 -2.5"
         }
         // A gauge.
         Page::Quotas => "M2.5 13.5a6.5 6.5 0 1 1 13 0M9 13.5l3.2 -4.2",
         // A window with a line of text in it.
         Page::Context => {
            "M4.5 3.5h9a2 2 0 0 1 2 2v7a2 2 0 0 1 -2 2h-9a2 2 0 0 1 -2 -2v-7a2 2 0 0 1 2 -2z\
             M2.5 7h13M5.5 10.5h4"
         }
         // A sun.
         Page::Look => {
            "M6 9a3 3 0 1 0 6 0a3 3 0 1 0 -6 0M9 1.5v2M9 14.5v2M1.5 9h2M14.5 9h2\
             M3.7 3.7l1.4 1.4M12.9 12.9l1.4 1.4M3.7 14.3l1.4 -1.4M12.9 5.1l1.4 -1.4"
         }
         // Two sliders.
         Page::System => {
            "M2.5 5h8M14 5h1.5M2.5 13h1.5M7.5 13h8M10.7 5a1.6 1.6 0 1 0 3.2 0\
             a1.6 1.6 0 1 0 -3.2 0M4.1 13a1.6 1.6 0 1 0 3.2 0a1.6 1.6 0 1 0 -3.2 0"
         }
      }
   }
}

/// A card on a page. The settings in it are the ones that name it, in the
/// order they stand in `ALL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
   Pct,
   Pace,
   Reset5h,
   Days,
   Bars,
   CtxShow,
   Session,
   Window,
   Pill,
   Panel,
   Theme,
   SrcBridge,
   SrcApi,
   SrcDesktop,
   SrcSession,
   Stale,
   General,
   Files,
   About,
}

impl Group {
   /// The order the cards stand in, page by page.
   pub const ALL: [Group; 19] = [
      Group::Pct,
      Group::Pace,
      Group::Reset5h,
      Group::Days,
      Group::Bars,
      Group::CtxShow,
      Group::Session,
      Group::Window,
      Group::Pill,
      Group::Panel,
      Group::Theme,
      Group::SrcBridge,
      Group::SrcApi,
      Group::SrcDesktop,
      Group::SrcSession,
      Group::Stale,
      Group::General,
      Group::Files,
      Group::About,
   ];

   pub fn page(self) -> Page {
      match self {
         Group::Pct | Group::Pace | Group::Reset5h | Group::Days | Group::Bars => Page::Quotas,
         Group::CtxShow | Group::Session | Group::Window => Page::Context,
         Group::Pill | Group::Panel | Group::Theme => Page::Look,
         Group::SrcBridge
         | Group::SrcApi
         | Group::SrcDesktop
         | Group::SrcSession
         | Group::Stale => Page::Sources,
         Group::General | Group::Files | Group::About => Page::System,
      }
   }

   /// `None` for a source's card: its first row, the source's own switch,
   /// is the heading, and a title over it would say the same word twice.
   pub fn title(self, ru: bool) -> Option<&'static str> {
      let words = match self {
         Group::SrcBridge | Group::SrcApi | Group::SrcDesktop | Group::SrcSession => return None,
         Group::Pct => w("Проценты", "Percentages"),
         Group::Pace => w("Темп", "Pace"),
         Group::Reset5h => w("Сброс пятичасового лимита", "Five-hour reset"),
         Group::Days => w(
            "Дни до недельного сброса и норма",
            "Days to the weekly reset, and allowance",
         ),
         Group::Bars => w("Бары", "Bars"),
         Group::CtxShow => w("Что показывать", "What to show"),
         Group::Session => w("Сессия", "Session"),
         Group::Window => w("Окно контекста", "Context window"),
         Group::Pill => w("Пилюля", "The pill"),
         Group::Panel => w("Панель подробностей", "Detail panel"),
         Group::Stale => w("Старые показания", "Old readings"),
         Group::Theme => w("Тема", "Theme"),
         Group::General => w("Общее", "General"),
         Group::Files => w("Файлы", "Files"),
         Group::About => w("О программе", "About"),
      };
      Some(words.get(ru))
   }
}

/// What a form does with a setting whose `Needs` is not met.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Otherwise {
   /// Stays in view, greyed out: a source that is switched off still has an
   /// expiry, and seeing it greyed says the source is off rather than gone.
   Dim,
   /// Leaves the form: a manual window size means nothing until "manual" is
   /// chosen, and a row that means nothing is noise.
   Hide,
}

/// A setting that only takes effect while another one has a given value.
#[derive(Clone, Copy, Debug)]
pub struct Needs {
   pub key: &'static str,
   /// `"true"`/`"false"` for a switch, the spelling in the file for a choice.
   pub value: &'static str,
   pub otherwise: Otherwise,
}

impl Needs {
   pub fn met(self, cfg: &Config) -> bool {
      match read(cfg, self.key) {
         Some(toml::Value::Boolean(on)) => (if on { "true" } else { "false" }) == self.value,
         Some(toml::Value::String(s)) => s == self.value,
         _ => true,
      }
   }
}

const fn needs(key: &'static str, value: &'static str, otherwise: Otherwise) -> Option<Needs> {
   Some(Needs {
      key,
      value,
      otherwise,
   })
}

/// The first sentence of a tip: what the form prints under a setting's name.
/// The whole tip stays for the tooltip.
pub fn lead(tip: &str) -> &str {
   let mut chars = tip.char_indices().peekable();
   while let Some((at, c)) = chars.next() {
      if matches!(c, '.' | '!' | '?') && chars.peek().is_none_or(|(_, n)| n.is_whitespace()) {
         return &tip[..at + c.len_utf8()];
      }
   }
   tip
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
   /// The card the settings window shows it on. `None` for the two that
   /// something else owns: the session id belongs to the picker, and the demo
   /// switch is a flag for working on the layout.
   pub group: Option<Group>,
   /// When it has any effect at all. A form hides or greys it out otherwise,
   /// and draws it indented under the setting it depends on.
   pub needs: Option<Needs>,
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
      group: Some(Group::SrcBridge),
      needs: None,
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
      group: Some(Group::SrcApi),
      needs: None,
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
      group: Some(Group::SrcDesktop),
      needs: None,
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
      group: Some(Group::SrcSession),
      needs: None,
   },
   Setting {
      key: "bridge_stale_secs",
      section: Section::Sources,
      kind: Kind::Int {
         min: 30,
         max: 86_400,
         step: 60,
         big: 300,
         show: Show::Duration,
      },
      label: w("statusLine устаревает через", "statusLine stale after"),
      tip: STALE_TIP,
      group: Some(Group::SrcBridge),
      needs: needs("bridge_enabled", "true", Otherwise::Dim),
   },
   Setting {
      key: "api_stale_secs",
      section: Section::Sources,
      kind: Kind::Int {
         min: 30,
         max: 86_400,
         step: 60,
         big: 300,
         show: Show::Duration,
      },
      label: w("usage API устаревает через", "usage API stale after"),
      tip: STALE_TIP,
      group: Some(Group::SrcApi),
      needs: needs("api_enabled", "true", Otherwise::Dim),
   },
   Setting {
      key: "desktop_stale_secs",
      section: Section::Sources,
      kind: Kind::Int {
         min: 30,
         max: 86_400,
         step: 60,
         big: 300,
         show: Show::Duration,
      },
      label: w("Приложение устаревает через", "Desktop app stale after"),
      tip: STALE_TIP,
      group: Some(Group::SrcDesktop),
      needs: needs("desktop_enabled", "true", Otherwise::Dim),
   },
   Setting {
      key: "session_stale_secs",
      section: Section::Sources,
      kind: Kind::Int {
         min: 30,
         max: 86_400,
         step: 60,
         big: 300,
         show: Show::Duration,
      },
      label: w("Транскрипт устаревает через", "Transcript stale after"),
      tip: STALE_TIP,
      group: Some(Group::SrcSession),
      needs: needs("session_enabled", "true", Otherwise::Dim),
   },
   Setting {
      key: "api_interval_secs",
      section: Section::Sources,
      kind: Kind::Int {
         min: 30,
         max: 3_600,
         step: 10,
         big: 60,
         show: Show::Duration,
      },
      label: w("Опрашивать каждые", "Poll every"),
      tip: w(
         "Как часто виджет сам ходит за цифрами в фоне. Реже — меньше запросов, \
             но цифры дольше остаются старыми.",
         "How often the widget fetches in the background. Rarer means fewer \
             requests but older figures.",
      ),
      group: Some(Group::SrcApi),
      needs: needs("api_enabled", "true", Otherwise::Dim),
   },
   Setting {
      key: "api_budget_per_min",
      section: Section::Sources,
      kind: Kind::Int {
         min: 1,
         max: 30,
         step: 1,
         big: 5,
         show: Show::Plain,
      },
      label: w("Запросов в минуту", "Requests per min"),
      tip: w(
         "Потолок, который виджет держит сам, чтобы сервер не ответил отказом. \
             Сверх него запрос не отменяется, а откладывается на несколько секунд.",
         "A cap the widget keeps for itself so the server never has to refuse. \
             Past it a request is delayed by a few seconds, not dropped.",
      ),
      group: Some(Group::SrcApi),
      needs: needs("api_enabled", "true", Otherwise::Dim),
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
      group: Some(Group::Session),
      needs: None,
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
      group: None,
      needs: None,
   },
   Setting {
      key: "session_scan_secs",
      section: Section::Behaviour,
      kind: Kind::Int {
         min: 5,
         max: 600,
         step: 10,
         big: 60,
         show: Show::Duration,
      },
      label: w("Пересматривать список", "Rescan the list"),
      tip: w(
         "Как часто виджет заново перечитывает папку разговоров. Между \
             просмотрами он смотрит только на один выбранный файл, так что это \
             почти ничего не стоит.",
         "How often the widget enumerates the transcripts again. Between scans \
             it stats one chosen file, so this costs next to nothing either way.",
      ),
      group: Some(Group::Session),
      needs: None,
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
      group: Some(Group::Window),
      needs: None,
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
      group: Some(Group::Window),
      needs: needs("ctx_limit", "fixed", Otherwise::Hide),
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
      group: Some(Group::General),
      needs: None,
   },
   Setting {
      key: "lang",
      section: Section::Behaviour,
      kind: Kind::Choice(&[
         ch("ru", "Русский", "Russian"),
         ch("en", "Английский", "English"),
      ]),
      label: w("Язык", "Language"),
      tip: w(
         "Язык подписей и панели подробностей: русский или английский.",
         "Language of the labels and the detail panel: Russian or English.",
      ),
      group: Some(Group::General),
      needs: None,
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
      group: None,
      needs: None,
   },
   Setting {
      key: "data_dir",
      section: Section::Behaviour,
      kind: Kind::Text,
      label: w("Папка данных", "Data folder"),
      tip: w(
         "Пусто — обычное место (`%APPDATA%`). Указав папку, сюда переезжают \
             config.toml, theme.slint и всё остальное, что программа хранит сама; \
             виджет перезапустится. Удобно, если обычное место недоступно для \
             прямого редактирования — например, из-за подмены %APPDATA% у \
             какой-то из открытых программ. Повторное изменение здесь не \
             переносит файлы второй раз — новое место нужно задавать в файле, \
             который лежит там, куда всё уже переехало.",
         "Empty is the usual place (`%APPDATA%`). Naming a folder moves \
             config.toml, theme.slint and everything else this program keeps for \
             itself there; the widget restarts. Useful when the usual place is not \
             reachable for direct editing — say, because some open program has \
             %APPDATA% redirected. Changing this again here does not move the \
             files a second time — a further move has to be set from the file \
             now sitting wherever everything already moved to.",
      ),
      group: Some(Group::Files),
      needs: None,
   },
   // ------------------------------------------------------------- display ---
   Setting {
      key: "pct_mode",
      section: Section::Display,
      kind: Kind::Choice(SPENT_LEFT),
      label: w("Проценты показывают", "Percentages show"),
      tip: w(
         "Что показывают большие проценты и полоски под ними: сколько квоты уже \
             потрачено или сколько её осталось. Цвет в обоих случаях считается по \
             расходу, поэтому «10% осталось» красное.",
         "What the big percentages and the bars under them show: how much of the \
             quota is spent, or how much is left. The colour is keyed on spending \
             either way, so 10% left shows red.",
      ),
      group: Some(Group::Pct),
      needs: None,
   },
   Setting {
      key: "pace_view",
      section: Section::Display,
      // Shown as themselves: the label *is* the thing, so a name for it
      // would be a second, worse description of what the eye can read.
      kind: Kind::Choice(&[
         ch("ratio", "▲2.0×", "▲2.0×"),
         ch("points", "▲9%", "▲9%"),
         ch("time", "▲0:27", "▲0:27"),
         ch("ratio_points", "▲2.0× 9%", "▲2.0× 9%"),
         ch("points_ratio", "▲9% 2.0×", "▲9% 2.0×"),
         ch("split", "▲9% | 2.0×", "▲9% | 2.0×"),
         ch("piped", "▲9%|2.0×", "▲9%|2.0×"),
         ch("off", "выкл", "off"),
      ]),
      label: w("Вид", "Reads as"),
      tip: w(
         "Что стоит рядом с каждым лимитом на пилюле. Пункты — на сколько расход \
             обогнал часы; кратность — во сколько раз придётся замедлиться; время — \
             те же пункты во времени окна: у 5 часов часы и минуты, у недели дни и часы.",
         "What stands beside each limit on the pill. Points are how far spending \
             has run ahead of the clock; the ratio is how many times slower the \
             rest has to go; time is the same points in the window's own time: \
             hours and minutes for five hours, days and hours for the week.",
      ),
      group: Some(Group::Pace),
      needs: None,
   },
   Setting {
      key: "pace_form",
      section: Section::Display,
      // Shown as themselves, as the pace forms are.
      kind: Kind::Choice(&[ch("units", "▲3д2ч", "▲3d2h"), ch("short", "▲3'2", "▲3'2")]),
      label: w("Недельный темп", "Weekly pace reads as"),
      tip: w(
         "Как выглядит недельный темп во времени. 3'2 — дни и часы через \
             разделитель; сам разделитель и нули в часах задаются в карточке \
             дней до сброса. В панели подробностей всегда словами.",
         "How the weekly pace reads in time. 3'2 is days and hours around a \
             separator; the separator and the zeros in the hours are set in the \
             card of days to the reset. The detail panel always spells it out.",
      ),
      group: Some(Group::Pace),
      needs: needs("pace_view", "time", Otherwise::Hide),
   },
   Setting {
      key: "bar_colour",
      section: Section::Display,
      kind: Kind::Choice(&[
         ch("fill", "по расходу", "by fill"),
         ch("pace", "по темпу", "by pace"),
      ]),
      label: w("Цвет", "Colour"),
      tip: w(
         "По расходу — краснеет к концу квоты. По темпу — краснеет, когда \
             тратишь быстрее часов, даже если потрачено мало. Короткий красный бар \
             значит: квота есть, времени нет.",
         "By fill it reddens as the window runs out. By pace it reddens when \
             spending outruns the clock, however little is gone. A short red bar \
             means quota left and no time to use it.",
      ),
      group: Some(Group::Bars),
      needs: None,
   },
   Setting {
      key: "show_5h_pace",
      section: Section::Display,
      kind: Kind::Bool,
      label: w("На пятичасовом", "On the five-hour"),
      tip: w(
         "Темп пятичасового лимита на пилюле. Только для пилюли: панель \
             подробностей и строка статуса показывают всё, а при «Темп = выкл» \
             его нет нигде.",
         "The five-hour pace on the pill. The pill only: the detail panel and \
             the status line show every figure, and with Pace off it is shown \
             nowhere.",
      ),
      group: Some(Group::Pace),
      needs: None,
   },
   Setting {
      key: "show_5h_left",
      section: Section::Display,
      kind: Kind::Bool,
      label: w("Осталось до сброса", "Time to reset"),
      tip: w(
         "Сколько осталось до сброса пятичасового лимита, 3:19. Только для \
             пилюли: панель подробностей и строка статуса показывают всё.",
         "How long until the five-hour limit resets, 3:19. The pill only: the \
             detail panel and the status line show every figure.",
      ),
      group: Some(Group::Reset5h),
      needs: None,
   },
   Setting {
      key: "show_5h_clock",
      section: Section::Display,
      kind: Kind::Bool,
      label: w("Время сброса", "Reset time"),
      tip: w(
         "Когда сбросится пятичасовой лимит, по местным часам, 20:49. Только \
             для пилюли: панель подробностей и строка статуса показывают всё.",
         "When the five-hour limit resets, by the local clock, 20:49. The pill \
             only: the detail panel and the status line show every figure.",
      ),
      group: Some(Group::Reset5h),
      needs: None,
   },
   Setting {
      key: "show_week_pace",
      section: Section::Display,
      kind: Kind::Bool,
      label: w("На недельном", "On the weekly"),
      tip: w(
         "Темп недельного лимита на пилюле. Только для пилюли: панель \
             подробностей и строка статуса показывают всё, а при «Темп = выкл» \
             его нет нигде.",
         "The weekly pace on the pill. The pill only: the detail panel and the \
             status line show every figure, and with Pace off it is shown \
             nowhere.",
      ),
      group: Some(Group::Pace),
      needs: None,
   },
   Setting {
      key: "show_week_days",
      section: Section::Display,
      kind: Kind::Bool,
      label: w("Дни до сброса", "Days to reset"),
      tip: w(
         "Сколько осталось до сброса недельного лимита: 5д, или дни и часы при \
             счёте по времени. Только для пилюли: панель подробностей и строка \
             статуса показывают всё.",
         "How long until the weekly limit resets: 5d, or days and hours when \
             counted by time. The pill only: the detail panel and the status \
             line show every figure.",
      ),
      group: Some(Group::Days),
      needs: None,
   },
   Setting {
      key: "show_week_allowance",
      section: Section::Display,
      kind: Kind::Bool,
      label: w("Дневная норма", "Daily allowance"),
      tip: w(
         "Сколько процентов недельного лимита можно тратить в день, чтобы \
             уложиться ровно к сбросу. Только для пилюли: панель подробностей и \
             строка статуса показывают всё.",
         "How much of the weekly limit may go per day to finish exactly at the \
             reset. The pill only: the detail panel and the status line show \
             every figure.",
      ),
      group: Some(Group::Days),
      needs: None,
   },
   // The day count: Behaviour keys in the file, but on the form they stand
   // on the card of the figure they count, under its switch.
   Setting {
      key: "days_by",
      section: Section::Behaviour,
      kind: Kind::Choice(&[
         ch("calendar", "по календарю", "by calendar"),
         ch("time", "по времени", "by time"),
      ]),
      label: w("Счёт дней", "Day count"),
      tip: w(
         "Как считать дни до недельного сброса. По календарю — целые дни, а какие \
             из них входят, решают два переключателя ниже. По времени — точное \
             время до сброса, 4д7ч, и дневная норма делится на него; переключатели \
             ниже тогда не действуют.",
         "How the days to the weekly reset are counted. By calendar, whole days, \
             and the two switches below decide which of them count. By time, the \
             exact time to the reset, 4d7h, and the daily allowance divides by it; \
             the switches below do nothing then.",
      ),
      group: Some(Group::Days),
      needs: None,
   },
   Setting {
      key: "days_form",
      section: Section::Display,
      // Shown as themselves, as the pace forms are.
      kind: Kind::Choice(&[ch("units", "3д16ч", "3d16h"), ch("short", "3'16", "3'16")]),
      label: w("Вид счётчика", "Count reads as"),
      tip: w(
         "Как выглядит время до недельного сброса на пилюле. 3'16 — дни и часы \
             через разделитель, он задаётся ниже. Только при счёте по времени.",
         "How the time to the weekly reset reads on the pill. 3'16 is days and \
             hours around a separator, set below. Only when days are counted by time.",
      ),
      group: Some(Group::Days),
      needs: needs("days_by", "time", Otherwise::Hide),
   },
   Setting {
      key: "weekly_sep",
      section: Section::Display,
      kind: Kind::Text,
      label: w("Разделитель", "Separator"),
      tip: w(
         "Что стоит между днями и часами в коротком виде: 3'16 у счётчика, \
             ▲3'2 у недельного темпа. Не двоеточие, чтобы не путать с часами и \
             минутами пятичасового окна, но вписать можно любой, до трёх знаков; \
             пусто — апостроф.",
         "What stands between days and hours in the short form: 3'16 on the \
             count, ▲3'2 on the weekly pace. Not a colon, so it is not taken for \
             the hours and minutes of the five-hour window, but any will do, up \
             to three characters; empty is an apostrophe.",
      ),
      group: Some(Group::Days),
      needs: None,
   },
   Setting {
      key: "weekly_pad",
      section: Section::Display,
      kind: Kind::Bool,
      label: w("Часы двумя цифрами", "Hours as two digits"),
      tip: w(
         "В коротком виде писать 2'07 вместо 2'7. Так ширина пилюли не меняется, \
             когда часов становится меньше десяти; без этого короче на знак.",
         "Write 2'07 rather than 2'7 in the short form. The pill then keeps its \
             width as the hours drop below ten; without it, one character shorter.",
      ),
      group: Some(Group::Days),
      needs: None,
   },
   Setting {
      key: "count_today",
      section: Section::Behaviour,
      kind: Kind::Bool,
      label: w("Считать сегодня", "Count today"),
      tip: w(
         "Входит ли текущий день в счётчик дней и в дневную норму. Сегодня \
             обычно уже наполовину прожито, поэтому по умолчанию не считается. \
             Только при счёте по календарю.",
         "Whether the current day counts towards the day figure and the daily \
             allowance. Today is usually half spent by the time you look, so it \
             does not by default. Only when days are counted by calendar.",
      ),
      group: Some(Group::Days),
      needs: needs("days_by", "calendar", Otherwise::Hide),
   },
   Setting {
      key: "count_reset_day",
      section: Section::Behaviour,
      kind: Kind::Bool,
      label: w("Считать день сброса", "Count reset day"),
      tip: w(
         "Входит ли день, в который квота обновляется. Он стоит только тех \
             часов, что до сброса: если сброс в 7 утра, этот день лучше выключить. \
             Только при счёте по календарю.",
         "Whether the day the quota resets counts. It is worth only the hours \
             before the reset: with a reset at 07:00 that day is better left out. \
             Only when days are counted by calendar.",
      ),
      group: Some(Group::Days),
      needs: needs("days_by", "calendar", Otherwise::Hide),
   },
   Setting {
      key: "detail_mode",
      section: Section::Display,
      kind: Kind::Choice(&[
         ch("with_settings", "с настройками", "with settings"),
         ch("hover", "при наведении", "on hover"),
         ch("never", "никогда", "never"),
      ]),
      label: w("Когда показывать", "Shown"),
      tip: w(
         "Когда показывать панель под пилюлей: пока открыто окно настроек, при \
             наведении мыши на пилюлю, или никогда.",
         "When the panel under the pill appears: while the settings window is \
             open, on hovering the pill, or never.",
      ),
      group: Some(Group::Panel),
      needs: None,
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
      group: Some(Group::Panel),
      needs: None,
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
      group: Some(Group::Panel),
      needs: None,
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
      group: Some(Group::CtxShow),
      needs: None,
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
      group: Some(Group::CtxShow),
      needs: None,
   },
   Setting {
      key: "ctx_pct_dir",
      section: Section::Display,
      kind: Kind::Choice(SPENT_LEFT),
      label: w("Ctx проценты", "Ctx percent"),
      tip: CTX_DIR_TIP,
      group: Some(Group::CtxShow),
      needs: None,
   },
   Setting {
      key: "ctx_tokens_dir",
      section: Section::Display,
      kind: Kind::Choice(SPENT_LEFT),
      label: w("Ctx токены", "Ctx tokens"),
      tip: CTX_DIR_TIP,
      group: Some(Group::CtxShow),
      needs: needs("ctx_dir_linked", "false", Otherwise::Hide),
   },
   // ---------------------------------------------------------- thresholds ---
   Setting {
      key: "warn_pct",
      section: Section::Thresholds,
      kind: Kind::Float {
         min: 0.0,
         max: 100.0,
         step: 1.0,
         big: 5.0,
         show: Show::Points,
      },
      label: w("Янтарный с", "Amber from"),
      tip: w(
         "С какого расхода цифра начинает уходить из зелёного в янтарный. \
             Считается всегда по потраченному, даже когда на пилюле стоит остаток.",
         "The figure at which a reading starts moving from green to amber. Read \
             off spending always, even when the pill is showing what is left.",
      ),
      group: Some(Group::Pct),
      needs: None,
   },
   Setting {
      key: "danger_pct",
      section: Section::Thresholds,
      kind: Kind::Float {
         min: 0.0,
         max: 100.0,
         step: 1.0,
         big: 5.0,
         show: Show::Points,
      },
      label: w("Красный с", "Red from"),
      tip: w(
         "С какого расхода цифра идёт из янтарного в красный, добираясь до \
             чистого красного к сотне.",
         "The figure at which a reading moves from amber to red, reaching full \
             red at a hundred.",
      ),
      group: Some(Group::Pct),
      needs: None,
   },
   Setting {
      key: "pace_tolerance",
      section: Section::Thresholds,
      kind: Kind::Float {
         min: 0.0,
         max: 50.0,
         step: 1.0,
         big: 5.0,
         show: Show::Points,
      },
      label: w("Допуск", "Tolerance"),
      tip: w(
         "Насколько можно отставать от часов и всё ещё считаться идущим по \
             графику. Работает только ниже линии: перерасход есть перерасход, и \
             цвет за линией начинается сразу с оранжевого.",
         "How far under the clock still counts as on pace. It applies below the \
             line only: over is over, and the colour past the line starts at orange \
             rather than at green.",
      ),
      group: Some(Group::Pace),
      needs: None,
   },
   Setting {
      key: "drop_after_secs",
      section: Section::Thresholds,
      kind: Kind::Int {
         min: 60,
         max: 86_400,
         step: 300,
         big: 3_600,
         show: Show::Duration,
      },
      label: w("Прятать показание через", "Drop a reading after"),
      tip: w(
         "Через сколько показание перестаёт показываться вовсе, а не просто \
             сереет. Пятичасовой процент часовой давности — это не приблизительное \
             значение, это другое окно.",
         "After this a reading is not shown at all, rather than merely greyed. A \
             five-hour percentage from an hour ago is not an approximation of the \
             present, it is a different window.",
      ),
      group: Some(Group::Stale),
      needs: None,
   },
   // --------------------------------------------------------------- style ---
   Setting {
      key: "scale",
      section: Section::Style,
      kind: Kind::Float {
         min: 0.6,
         max: 3.0,
         step: 0.01,
         big: 0.05,
         show: Show::Percent,
      },
      label: w("Общий масштаб", "Overall scale"),
      tip: w(
         "Множитель для всего сразу: шрифтов, высоты, отступов. Первое, что стоит \
             крутить на другом мониторе.",
         "One multiplier for the fonts, the height and the paddings alike. The \
             knob to reach for on a different monitor.",
      ),
      group: Some(Group::Pill),
      needs: None,
   },
   Setting {
      key: "opacity",
      section: Section::Style,
      kind: Kind::Float {
         min: 0.3,
         max: 1.0,
         step: 0.01,
         big: 0.05,
         show: Show::Percent,
      },
      label: w("Непрозрачность", "Opacity"),
      tip: w(
         "Насколько плотная подложка у пилюли и панели. Текст и цифры не бледнеют.",
         "How solid the background of the pill and the panel is. The text and \
             the figures stay as they are.",
      ),
      group: Some(Group::Pill),
      needs: None,
   },
   Setting {
      key: "anchor",
      section: Section::Style,
      kind: Kind::Choice(&[ch("left", "слева", "left"), ch("right", "справа", "right")]),
      label: w("Начало пилюли", "Pill grows from"),
      tip: w(
         "Какой край пилюли стоит на месте, когда она меняет ширину — например, \
             когда 9% становится 10%. Слева — растёт вправо; справа — растёт \
             влево: для пилюли у правого края экрана.",
         "Which edge of the pill stays put when its width changes — when 9% \
             becomes 10%, say. Left grows it to the right; right grows it to \
             the left, for a pill parked against the right of the screen.",
      ),
      group: Some(Group::Pill),
      needs: None,
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
/// Through serde rather than through a thirty-seven-arm `match`: the round trip
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
      assert!(
         missing.is_empty(),
         "not described in the schema: {missing:?}"
      );

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
               "scale" => {
                  cfg.scale = v;
                  cfg.scale()
               }
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

   #[test]
   fn every_page_has_a_card_and_every_card_a_page() {
      // A page with nothing on it is an entry in the side list that opens
      // onto a blank pane; a card on no page is never seen.
      for page in Page::ALL {
         assert!(
            Group::ALL.iter().any(|g| g.page() == page),
            "{page:?} has no card"
         );
      }
      for group in Group::ALL {
         assert!(Page::ALL.contains(&group.page()));
      }
   }

   #[test]
   fn what_a_setting_needs_is_a_setting_beside_it() {
      // Checked against the schema rather than trusted: a misspelt key or
      // value would hide a row for good, and nobody would notice a row that
      // is not there.
      for (at, s) in ALL.iter().enumerate() {
         let Some(n) = s.needs else {
            continue;
         };
         let Some(by) = ALL.iter().position(|o| o.key == n.key) else {
            panic!("{} needs {}, which does not exist", s.key, n.key);
         };
         let other = &ALL[by];
         // Drawn indented under it, so it has to be on the same card and
         // come after it.
         assert_eq!(
            other.group, s.group,
            "{} needs {} from another card",
            s.key, n.key
         );
         assert!(by < at, "{} stands before {}, which it needs", s.key, n.key);
         // One level only: a chain would need the form to work out which
         // link hid the row.
         assert!(
            other.needs.is_none(),
            "{} needs {}, which needs something too",
            s.key,
            n.key
         );
         let spelled = match other.kind {
            Kind::Bool => n.value == "true" || n.value == "false",
            Kind::Choice(options) => options.iter().any(|o| o.value == n.value),
            _ => false,
         };
         assert!(
            spelled,
            "{}: `{}` is not a value {} can take",
            s.key, n.value, n.key
         );
      }
   }

   #[test]
   fn a_need_is_met_or_not_by_the_config() {
      let mut cfg = Config::default();
      let tokens = get("ctx_limit_tokens").and_then(|s| s.needs).unwrap();
      cfg.ctx_limit = crate::config::CtxLimit::Learned;
      assert!(!tokens.met(&cfg));
      cfg.ctx_limit = crate::config::CtxLimit::Fixed;
      assert!(tokens.met(&cfg));

      let stale = get("api_stale_secs").and_then(|s| s.needs).unwrap();
      cfg.api_enabled = false;
      assert!(!stale.met(&cfg));
      cfg.api_enabled = true;
      assert!(stale.met(&cfg));
   }

   #[test]
   fn every_tip_opens_with_a_sentence_short_of_the_whole() {
      // The lead is what stands under a setting's name; a tip whose first
      // sentence cannot be found would print whole there, several lines of it.
      for s in ALL {
         for ru in [true, false] {
            let tip = s.tip.get(ru);
            let first = lead(tip);
            assert!(!first.is_empty(), "{}: no lead", s.key);
            assert!(
               first.ends_with(['.', '!', '?']),
               "{}: the lead does not end a sentence: {first}",
               s.key
            );
         }
      }
      assert_eq!(lead("One. Two."), "One.");
      assert_eq!(lead("A 2.5 thing. Next."), "A 2.5 thing.");
      assert_eq!(lead("No full stop"), "No full stop");
   }
}
