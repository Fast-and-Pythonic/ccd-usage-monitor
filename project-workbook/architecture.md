# Architecture

- **A01** · One executable, two modes
- **A02** · A state file, not IPC
- **A03** · Four sources, merged field by field
- **A04** · The pill is painted, not composed from widgets
- **A05** · Click-through requires the tray
- **A06** · the panel flips at the screen edge, and the pill holds its place
- **A07** · figures expire
- **A08** · The desktop app's own history as a source
- **A09** · Responsiveness comes from wake-ups, not from polling harder
- **A10** · a frameless window on Windows does not come out frameless
- **A11** · the settings are a second native window
- **A12** · the detail panel is sized to its text
- **A13** · a request budget, and a rate limit as a schedule
- **A14** · sources are switches, each with its own idea of "stale"
- **A15** · the display convention is a view, not a change to the model
- **A16** · the pace is one comparison, said in three units and two places
- **A17** · the widget keeps its own history, seeded from the app's
- **A18** · the context figure is read from the transcript, not asked for
- **A19** · three layers, and only one of them is about drawing
- **A20** · the GUI is Slint, and the core stayed put
- **A21** · the snapshot is published as JSON, and that JSON is a contract
- **A22** · the settings are three files, and one list describes them all
- **A23** · the config is re-read while the program runs
- **A24** · the theme contract — what the program sends, what the theme declares
- **A25** · the pill and its panel are two windows
- **A26** · one indent width, three spaces, enforced rather than remembered
- **A27** · the poller remembers itself across a restart
- **A28** · `data_dir` moves every file this program writes, in one hop (revised by A36)
- **A29** · the weekly days are counted by the calendar or by the clock
- **A30** · figures worked out from the time step on the minute
- **A31** · a hint names the figure, then says what it is now
- **A32** · a commit has to pass the checks, and a hook says so
- **A33** · the settings window is pages of cards, and the schema says where each setting sits
- **A34** · a cell is as wide as its text, and one edge of the pill stays put
- **A35** · an expired token is handed back to the CLI to renew, never renewed here
- **A36** · our files live beside the executable, never in `%APPDATA%`
- **A37** · in the desktop app a Claude Code mod stands in for the status line
- **A38** · `turn-meter`: a turn's share of the limits is its dollars over the price of a point

## A01: One executable, two modes
`ccd-usage-monitor.exe` is both the widget and the status-line command (`--bridge`).

**Why:** the status line runs on every conversation tick, so the command must
start in milliseconds — a Rust binary does, a script interpreter plus JSON
parser does not. Shipping one file also removes the class of bug where the
widget and the bridge disagree about the state-file schema.

**Consequence:** the release binary is built with
`windows_subsystem = "windows"`, so `--bridge` and `--print` produce no console
window when Claude Code spawns them, and their stdout still reaches the caller.

## A02: A state file, not IPC
The bridge writes `state.json` into the data folder (A36); the widget polls its mtime
every 250 ms.

**Why:** the bridge is a short-lived process spawned by someone else, several
times over, possibly concurrently from several sessions. A socket or pipe would
need the widget to be running and listening, and would lose data whenever it is
not. A file survives the widget being closed, restarted, or never started.

**How the races are handled:** writes take a lock file (stolen after 3 s, in
case a process died holding it), then write to a temporary file and rename it
over the target, so a reader never observes a partial document. The lock is
best-effort: if it cannot be taken in 40 tries the write proceeds anyway,
because stalling the user's status line is worse than a rare interleaved write.

## A03: Four sources, merged field by field
Every `Window` carries `observed_at` and its `Source`. `Window::merge` takes the
percentage from whichever source saw it last, and the reset time from whichever
source has one at all, preferring the newer. `ContextInfo::merge` (A18) sits
beside it in the same step and follows the same shape for the context figure.

**Why per field, not per window:** the two halves of a window age differently.
A percentage is only true for the moment it was read. A reset time is an
absolute moment — read once, it stays true until it passes. So a source that
reports percentages and nothing else (the desktop app's history, A08) can carry
the figures while an hour-old API reading still supplies the countdown, and
neither has to be discarded for lacking the other's half.

**Why four sources:** none of them dominates. The bridge is official and
instant, but only while a terminal session runs. The endpoint is exact and
works with no session at all, but is undocumented and its token expires. The
desktop history needs no credentials and never fails, but lags fifteen minutes
and carries no reset times. The transcripts (A18) are the only source of a
context figure that works in the desktop app, but they say nothing about quotas
and have to infer the window size they divide by.

Absent fields never overwrite present ones: a status-line tick without
`rate_limits` (which is every tick before the session's first API response)
leaves the stored windows alone.

## A08: The desktop app's own history as a source
`%APPDATA%\Claude\plan-usage-history.json` is a log the Claude desktop app
appends to about every fifteen minutes: `{"t": <epoch ms>, "org": "<uuid>",
"u": {"fh": 8, "sd": 19}}`, where `fh` and `sd` are the five-hour and seven-day
percentages.

**Why it matters:** it is the only source that asks nothing of the user. No
token, no session, no status line — the file is simply there, and it is written
by the same app whose figures the widget is trying to mirror. It is what keeps
the pill alive when the OAuth token lapses.

**Why it is not enough on its own:** percentages only, and a quarter-hour of
lag. Countdowns and the pace tick need a reset time, which only the endpoint and
the bridge provide.

## A09: Responsiveness comes from wake-ups, not from polling harder
The endpoint is undocumented, so the background interval is deliberately slow
(180 s). What makes the widget feel immediate is `api::spawn_poller` waiting on
a channel via `recv_timeout` instead of sleeping: a send on that channel makes
the next poll happen now.

Wake-ups are sent on startup, on switching to a mode that uses the endpoint,
when the detail panel opens, from the menu's "refresh now", and when the desktop
history file changes — a new sample means the figures moved, and a percentage
that *fell* means a window rolled over and the stored reset times are wrong.
Everything except the explicit ones is rate limited to one wake per 30 s.

## A04: The pill is painted, not composed from widgets
Everything in the pill's own window is drawn with `Painter` primitives. The only
ordinary egui widgets in the project live in the settings window (A11), which is
a window of its own — right-clicking the pill opens that, not a panel.

**Why:** the target is a ~34-pixel-tall translucent bar where the exact spacing
and the hairline tracks matter. Laying that out with egui widgets would mean
fighting the default spacing model for every pixel.

**Consequence (until A34):** each figure was drawn in a fixed-width slot, sized
to its widest possible value (`100%`, `00:00`, `00d`), so the pill did not
jitter as digits changed. A34 dropped the slots: a cell is as wide as its text
and the spaces between figures are fixed instead.

**Sizes come from `Metrics`, not from constants.** Every distance and font size
is resolved once per frame from the config, so the menu can offer them as
controls. The base numbers stay as constants at scale 1.0 and are multiplied by
`Config::scale`; positions inside the pill are shares of its height rather than
fixed pixels, so a taller pill spreads its contents instead of pooling them in
the middle. `MARGIN` is the one exception and stays constant: the anchor and the
panel offset are bookkept in those units, and a margin that moved with the scale
would make the window jump on every nudge (A06).

## A05: Click-through requires the tray
The click-through toggle is only offered when the tray icon was created, because
a window that ignores the mouse cannot be right-clicked to turn the mode back
off. The tray is the escape hatch; without it the option is hidden entirely.

## A06: the panel flips at the screen edge, and the pill holds its place
The detail panel opens below the pill, or above it when the pill sits too close
to the bottom of the monitor — which is exactly where a status widget tends to
be dragged. The same applies sideways: the panel is sized to its text and is
usually wider than the pill, so near the right edge the window grows *leftwards*
and the panel ends flush with the pill instead of pushing the whole widget
inwards. `App::applied_offset` carries both shifts.

**Why the extra machinery:** the window is sized to its content, so a panel above
means the window has to grow *and move up*, and moving it must not drag the pill
along. `App::anchor` holds the pill's top-left in screen coordinates and is set
only twice — once at startup and again when a drag ends — always from
`ViewportInfo::outer_rect`.

**The trap:** recomputing the anchor every frame from `outer_rect` looks
equivalent and is not. `outer_rect` lags a frame behind the move command, so the
anchor picks up the panel offset twice and the window walks off the top of the
screen a panel-height at a time. Seeding the anchor from `Ui::max_rect()` fails
for a related reason: that rectangle is window-local, so the first frame places
the window at roughly (0, −panel_height).

## A07: figures expire
Two thresholds. The first is per source — `bridge_stale_secs`, `api_stale_secs`,
`desktop_stale_secs`, `session_stale_secs`, 15 minutes except the desktop
history's 30 (A14) — and past it the numbers grey out and the pill shows an
amber dot. The second is shared: past `drop_after_secs` (1 h) a window is not
shown at all.

**Why:** the bridge only updates while a session is running, so the natural
failure mode is a widget confidently displaying a percentage from hours ago. A
five-hour window read an hour ago is not an approximation of the present, it is
a different window. `--%` is worth more than a wrong number that looks right.

**The context figure greys but is never dropped**, because it describes a
conversation rather than a moving window. The reasoning is in A18.

## A10: a frameless window on Windows does not come out frameless
The widget removes its own window frame through Win32 (`src/win.rs`) right after
eframe creates the window, and again whenever visibility or click-through
changes.

**Why:** `egui-winit` turns winit's undecorated-shadow mode on for every window
built with `with_decorations(false)`. winit implements that by keeping
`WS_CAPTION | WS_THICKFRAME` and collapsing the non-client area from
`WM_NCCALCSIZE`, which is exactly what makes DWM paint its rectangular drop
shadow — plus, in winit's own comment, "a small black 1px border on the top". On
a translucent pill both are plainly visible, and the shadow is rectangular while
the pill is not.

**Why Win32 and not a viewport command:** the only `ViewportCommand` that
touches the shadow is `Decorations(true)`, which brings the title bar back with
it. There is no way to ask egui for a frameless window without the shadow.

**What `strip` does:** `DWMWA_NCRENDERING_POLICY = DWMNCRP_DISABLED` stops DWM
drawing the non-client area, and clearing `WS_CAPTION | WS_THICKFRAME` removes
the frame the shadow hangs off.

**Why that was not enough:** the styles are winit's *output*, not its state. Its
`WM_NCCALCSIZE` handler keeps adding a pixel to the top of the client rectangle
while its own undecorated-shadow flag is set, and the system paints that
one-pixel strip in the frame colour — a white line across the top of a window
that is supposed to be invisible. The flag is internal to winit and cannot be
reached, so `Frame::subclass` takes over the window procedure and answers
`WM_NCCALCSIZE` with "the client area is the whole window" before winit sees
it. Everything else is passed on with `CallWindowProcW`.

**Three more messages, for the same reason.** `WM_NCPAINT` answered with 0 and
`WM_ERASEBKGND` with 1 stop the system painting a frame that does not exist and
erasing the client area with the class brush — both showed as a white flash.
`WM_NCACTIVATE` is *forwarded* with `lParam = -1` instead: that is the
documented way to say "do everything except repaint the caption".

**Do not answer `WM_NCACTIVATE` yourself.** Returning 1 without letting the
default handling run also removes the white flash, and it silently breaks
dragging: `ViewportCommand::StartDrag` posts `WM_NCLBUTTONDOWN` with
`HTCAPTION`, whose modal move loop depends on that default activation
handling. The pill simply stopped following the mouse, with nothing in the
logs (G12).

**Why it repeats:** winit rebuilds the window style from its own flags whenever
it changes visibility or mouse passthrough, which restores the shadow. Both
commands are sent from `apply_window_state`, which sets a flag and re-strips on
the *following* frame — a viewport command is only applied once the current
frame ends, so stripping in the same frame would happen too early.

## A11: the settings are a second native window
The settings live in their own OS window with a system title bar, opened from
`App::ui` with `Context::show_viewport_immediate` while `settings_open` is set.

**Why not a panel under the pill:** the pill is 30-odd pixels tall and as wide
as five figures. Hanging a settings panel off it meant the indicator grew to
five times its height, four stepper buttons had to share a strip narrower than
their labels, and every row was placed by hand at a computed rectangle. A real
window has width, a scrollbar, and a close button the user already knows.

**Why immediate and not deferred:** a deferred viewport's callback must be
`Fn + Send + Sync + 'static`, which cannot touch `&mut App`. The immediate
variant is called synchronously inside the parent's pass and takes `FnMut`, so
the settings can read and write the live config. The cost is that both windows
repaint together, which for two windows updating once a second is nothing.

**Consequences worth knowing:** the child window paints its own background,
because `App::clear_color` is transparent for the pill's sake and applies to
every viewport; "Quit" must be sent with `send_viewport_cmd_to(ViewportId::ROOT,
…)`, since a plain `Close` from inside the child only shuts the settings; and
the window's position and size are read back from `ViewportInfo` and stored in
the config, throttled to one write every two seconds so dragging it does not
rewrite the file every frame.

## A12: the detail panel is sized to its text
`detail_lines` builds the panel's contents as (label, value) pairs before
anything is drawn, so the panel can be measured with `measure()` and made as
wide as its longest line — wider than the pill when the line needs it.

**Why it matters:** a line reads `22% · resets Fri 20:20 · over pace · usage API
29 s`. Sized to the pill it was cut off mid-word, which is worse than useless:
the part that gets cut is the source and the age, exactly what the panel exists
to answer.

## A13: a request budget, and a rate limit as a schedule
The poller keeps two things: a budget of requests per rolling minute
(`api_budget_per_min`, four by default) and a `penalty_until` set when the
endpoint refuses.

**The budget comes first, and it delays rather than refuses.** When it is spent,
the poller sends `ApiEvent::Throttled { until }` and waits — then makes the
request itself, without being asked again. A handful of manual refreshes in a
row therefore costs a few seconds, not a five-minute penalty. That asymmetry is
the point: hitting the server's limit is expensive and opaque, hitting our own
is cheap and visible.

**The penalty is a ladder.** `Retry-After` if the answer carries one; otherwise
sixty seconds, doubling on each consecutive refusal to a ceiling of five
minutes, reset by any success (`api::next_penalty`). The previous fixed guess of
five minutes was the widget punishing the user far beyond anything the endpoint
had actually asked for.

**While a penalty is on, nothing goes out** — not on the interval, not on a
wake-up, not on "refresh now": the one thing that certainly makes a rate limit
worse is asking again. The figures do not disappear meanwhile; they age normally
under A07, and the desktop history (A08) keeps feeding percentages that need no
token at all.

**The poller's settings are shared, not copied.** `spawn_poller` takes an
`Arc<PollerSettings>` and re-reads the interval and the budget on every lap.
They used to be arguments, which meant changing one had to drop the thread and
start another — and a thread starts by making a request, so editing the request
budget spent from that very budget (G14). Changing a setting now sends
`Wake::Recheck`, which only makes the poller look at the clock again;
`Wake::Now` is the one that means "go".

**The policy is a pure function.** `api::decide` returns `Request`, `Wait` or
`Throttled` from (now, last request, interval, forced, penalty, history,
budget), in that order of precedence — a penalty outranks even "refresh now",
the schedule outranks the budget. The loop has no decisions left of its own,
and the rules are tested rather than argued about.

**Reading the status at all** requires `http_status_as_error(false)` on the
request. With ureq's default the response is converted into
`Error::StatusCode(429)`, whose `to_string()` is "http status: 429" — which is
how that text ended up in the widget's own status line.

## A14: sources are switches, each with its own idea of "stale"
Every source has an `*_enabled` flag rather than all of them sharing one "mode"
setting, and every source carries its own `*_stale_secs`.

**Why switches:** the old single choice could say "only the API" or "all of
them" but not "the app's history and the API, without the status line". Four
independent flags say all of it, and the settings window puts each source's own
knobs directly under its switch, where they mean something — which is where the
transcript source's session picker and window-size strategy live (A18).

**Why per-source staleness:** they age at completely different rates. The
endpoint answers in seconds, so a fifteen-minute-old reading from it means
something is wrong; the desktop app writes its history about every fifteen
minutes, so the same threshold marks it stale almost always — blaming a source
for how it works. Bridge, API and the transcripts keep 15 minutes, the desktop
history gets 30.

**Migration:** a config written before this carries `source = "auto"` or
similar. `Config::migrate` translates it into the flags once, drops the key and
rewrites the file, so an old config keeps meaning what it meant instead of
silently reverting to defaults.

## A15: the display convention is a view, not a change to the model
`pct_mode` switches the pill between "48% spent" and "52% left", and the bars
follow the figure they sit under. Everything else keeps to spending.

**`pct_mode` governs the two quotas, not the whole pill.** The `ctx` group reads
by its own pair of directions — `ctx_pct_dir` and `ctx_tokens_dir` — because a
context window fills where a quota drains; the argument is in A18. Everything
below still holds for both, since both go through `PctMode::shown`.

**One number stored, two ways to read it.** A reading is `used_pct` throughout —
merging, staleness, the state file, `--sources`. Only the last step before the
eye passes it through `PctMode::shown`, so nothing downstream has to know which
convention is on, and no source has to report two figures.

**Colour stays on spending.** `usage_color`, `warn_pct`/`danger_pct` and the
pace verdict are fed `used_pct` directly, never the shown figure. Otherwise
"10% left" would come up green at exactly the moment it should be shouting, and
a bar would go red as it drained to nothing. What the eye gets in "left" mode is
a bar that shrinks *and* reddens — two signals agreeing, not one inverted.

**The daily allowance is not inverted.** `20%/day` already answers "how much may
I spend", so flipping it would produce a number with no meaning. It reads the
same in both modes.

**The panel says both.** The pill has room for one figure, so the convention is
ambiguous at a glance; the detail panel prints `62% spent · 38% left`, the
current mode first. That is where the two conventions are reconciled, which is
why the panel is also where the pace verdict and the reset time live.

## A16: the pace is one comparison, said in three units and two places
Usage against the share of the window already elapsed — `used_pct - pace_target`
— is the whole of it. Everything about pace is a rendering of that difference.

**Two units, one decision.** The gap in points (`▲14%`, as claude-pace says it)
and the ratio (`▲2.0×` from `pace_ratio`: how many times slower the rest has to
go, the form that says what to *do*) are the same comparison twice. `PaceView`
is every arrangement of them — either alone, the points in time alone, both in either order, both with a
rule between, both run together behind a written pipe, or none — in one list,
because which of them helps is taste and both at once is a legitimate answer. It replaced an on/off switch plus a
two-way unit toggle: three states of two settings that could not express "both".
The arrow, the verdict word and every colour come from the points whatever the
form on screen, so no two of them can disagree. `pace_ratio` is clamped at ±9.9:
a window with nothing left divides by zero, and `inf` is not a reading.

**Time is the points again, at the window's rate.** `pace_secs` turns the gap
into a share of the window's length — 1% of five hours is three minutes, so nine
points ahead is `▲0:27`; 1% of a week is 100.8 minutes. The grain follows the
window rather than the value: `h:mm` for five hours, like the countdown beside
it, and days with whole hours for the week (`▲3д2ч`), because `73:55` is a sum to
do and minutes are noise at that scale. `pace_form = "short"` writes the week
as `▲3'2` instead, the way `days_form` shortens the day count and with the same
separator and zeros (A29): on the pill and the status line only, never in the
panel. A setting of its own rather than a
follower of `days_form`, because that one only exists when days are counted by
time and a pace in time form does not care how they are counted. It is one form on its own, not paired:
pairs with it would double a list of seven for no reading the panel does not
already give. The cell is as wide as the text on show (A34); before that it
reserved the widest week, and a centred `▼9ч` sat in a hole three glyphs wide
that read as broken spacing. The panel and `--serve`'s `text` always carry all
three units (`pace_detail`) and the JSON has `delta_secs` beside `delta_pct`.

**The pace figure sits second, against the percentage.** It qualifies that
number and nothing else — how much is left and how fast it is going are one
reading — while the two clocks after it answer a different question. At the end
of the group it read as a fourth clock and was a whole group away from the
figure it was about. The bridge line follows the pill here, as it does
everywhere else.

**One arrow, and the panel says everything.** Only the first half of a combined
figure carries the arrow — the direction is shared, and a second one is noise on
a pill. The detail panel ignores the choice and prints both numbers in the
chosen order, the same way it prints both of `pct_mode`'s readings: the pill is
where taste is honoured, the panel is where things are spelled out.

**The arrow is a smaller cell, not a smaller glyph.** `▲`/`▼` read a size too
large next to the digits beside them, and Segoe UI's small-triangle variants
(`▴`/`▾`) turned out to be missing from the font — tofu boxes on the live pill,
found only once shipped, not in a screenshot taken from the source. `view.rs`'s
`split_arrow` peels the leading arrow off the assembled text and draws it as
its own cell at `Role::Arrow` — its own theme variable
(`font_arrow`), apart from `Role::Small`'s (`font_small`, the `ctx` label's)
even though both started at the same size, because the arrow now sits inline
beside a number where `Small` never used to — ahead of the number at its usual
size, same colour, joined with `Gap::Tight` so the pair still reads as one
figure. A figure with no arrow (an even burn) is one cell: nothing is held
open for the arrow, and the pill narrows by its width (A34).

**A cell centres on its own box only, not on a taller sibling's.** Mixing
`Role::Arrow` inline with `Role::Time` exposed a `pill.slint` gap that had
sat unnoticed: `Cell`'s `shown` Text had no `y` of its own, so when the
surrounding layout stretched a cell taller than its text (to match a taller
sibling in the same row), the text stayed pinned at the stretched box's top
edge — `vertical-alignment: center` only centres within a Text's *own* box,
which defaults to exactly its content's height, so it had nothing to do
before two different sizes ever shared a row. `Role::Small` alone (the `ctx`
tag) never triggered it: it always stood in a row by itself. Both `shown` and
the invisible `ghost` now carry `y: (parent.height - self.height) / 2`,
centring against the cell's actual, post-stretch height instead.

**Green lives below the line only.** The first version ramped green→amber across
the tolerance, so a two-point overspend came out a tired green — indistinguishable
at a glance from fourteen points of headroom next to it. Colour is the fastest
channel this widget has and it was spending it on a lie. Now `pace_color` is
green at or under the line and starts at orange (`E8994C`) the moment it is over,
deepening to red across twice the tolerance. `pace_of` moved with it: over the
line is `Behind`, and the tolerance only separates `OnTrack` from `Ahead`
underneath. A margin is worth having below the line; above it, over is over.

**A cell asks for its own trailing gap.** What separates two figures is a
`Gap` on the cell before it — a plain space, nothing at all, a faint rule, or
the divider between two limits — rather than lists of indices the layout and the
painter both have to consult. The width of the pill is then literally the
sum the painting walk adds up, which is what the two lists had stopped agreeing
on: the rule inside a `split` figure was painted a gap and a half wide and
budgeted one. It also made the `piped` form possible without a fourth list: it
is three cells with no gaps between them, the middle one holding the separator
alone so that it can keep the grey of the clock while the numbers carry the pace
colour. One glyph cannot have two colours; two cells can.

**A hint that names one field, without a second window.** The pill is ten
figures with no labels, and the detail panel answers all of them at once rather
than the narrower question — that number there, what is it? So each cell
carries a name, and pointing at it draws a small box in the same band the panel
uses, aligned under the field. Not a real tooltip window: the widget's window is
its own size plus a margin for the shadow, so an egui tooltip would be clipped
by it, and a viewport of its own is the thing that flashes white on Windows
until it has painted a frame (which is why the settings window is born hidden).
The name is discovered at the end of a frame and shown on the next one, because
the window is measured and resized before anything is painted; one frame is not
visible. When the panel is open the name becomes its first line instead — two
surfaces answering one hover is one too many.

**Colour is a separate axis from length.** A bar's length follows `pct_mode`,
its colour follows `bar_colour`. `usage_color` answers "how much is gone",
`pace_color` answers "is it going too fast"; both are on the same green-amber-red
scale, and the second is scaled by `pace_tolerance` rather than by
`warn_pct`/`danger_pct`. A short red bar is the case worth having: quota left,
no time to spend it in.

**The observed rate is a different thing again, and needs a memory.** The pace
above compares against the clock; the rate in the panel is what actually
happened over the last hour, out of `history`. Because it needs samples it can
be silent, and it is: no rate rather than a rate from two readings a minute
apart.

## A17: the widget keeps its own history, seeded from the app's
No source reports a rate — each says only where usage stands now — so the
widget remembers. `history::Log` is a rolling file of `(time, 5h%, 7d%)` next to
the state file, written whenever a reading moves and at least every five minutes
so a quiet hour reads as "nothing spent" rather than as "nothing known".

**Runs are kept as their two ends.** The value is a step function; the middle of
a run of identical readings carries nothing, so `record` moves the far end of a
run instead of appending to it. An idle night costs two samples, not three
hundred.

**Rates are bounded by the window.** Percentages fall to zero at a reset, so
`rate` never looks earlier than `resets_at - window`, and cuts the range again
at any fall inside it — a reset we did not witness. The start of the range is
anchored by the last reading before it, held: that is what makes an idle hour
read as `0`/h instead of as silence.

**The seed is the desktop app's file, and is not the desktop *source*.** Reading
`plan-usage-history.json` at startup gives a month of samples at a quarter-hour
cadence, so a fresh install can answer "how fast" immediately. Tying that import
to the `desktop_enabled` switch would have conflated two different questions:
whose figures to show now, and what is known about the past.

## A18: the context figure is read from the transcript, not asked for
The status line is the documented way to learn how full a session's context
window is, and it is a terminal feature: the desktop app never runs it (G06), so
for anyone working in the app the field simply never appeared. The conversation
transcripts under `~/.claude/projects` are written whatever the client is, and
the last assistant line of one carries the token counters. So the figure is read
from the file rather than waited for.

**Why the formula is quoted rather than derived.** `input_tokens +
cache_creation_input_tokens + cache_read_input_tokens`, rounded and clamped, is
what Claude Code's own status-line code does with the same numbers. Deriving a
plausible sum instead — adding the output tokens, say — would produce a figure
that is close enough to look right and wrong enough to mislead, and there would
be nothing to check it against.

**Why the numerator and the denominator are carried separately.** The count is
exact. The window it is counted against is not on disk at all: it comes from a
model catalogue the client fetches, and the caches that would hold it are empty
on a machine that has only ever run the desktop app. So `ContextInfo` carries
both halves and a flag saying whether the size was stated or guessed, the panel
prints the division rather than only its answer, and `ctx_view` can put the raw
count on the pill. A denominator wrong by five times is invisible in a
percentage and obvious in `309k of 1.0M`.

**Why a size survives a newer reading.** `ContextInfo::merge` follows
`Window::merge`: the fill comes from whichever source saw it last, the size from
whichever source was told it, because a window size is an attribute of a model
rather than of a moment. That is the whole mechanism by which one terminal
session teaches the widget a size that every later transcript read then uses.
The size does not travel between models, for the same reason a reset time does
not travel between windows.

**Why the session is chosen rather than guessed.** Eighty-odd transcripts sit
under `~/.claude/projects` on a working machine, and all but the newest few are
finished conversations that would each report a perfectly true figure about a
session nobody is in. `~/.claude/sessions/<pid>.json` names the ones Claude Code
currently lists as running, which is the default; the alternative is to pin one
by hand. "Running" here means "registered and not cleaned up" — a killed process
leaves its entry behind — so nothing claims more than that.

**Why context does not expire the way a window does.** A07 drops a five-hour
percentage after an hour because that percentage describes a window that has
moved on. A conversation's fill is a fact about that conversation and stays true
until someone types in it again, so it goes grey on its source's clock and is
never dropped. Blanking it after twenty quiet minutes would empty the field
exactly while it was being read.

**Why `ctx` does not follow `pct_mode`.** The first shipped version fed
`c.used_pct` through the pill's shared `pct_mode` like every other percentage
(A15) and left the token count untouched, on the reasoning in A15 that a count
"owes nothing to a denominator we may have guessed" and so should never be
inverted. That reasoning covered the token count; it did not cover the
percentage sitting right next to it. A pill with `pct_mode = left` — the normal
setting for someone who watches quotas drain — showed the two ctx figures
disagreeing: `384k` (spent) beside `62%` (left, i.e. 38% spent), each correct
on its own terms and contradictory side by side. A quota drains from a limit
that was already reached once; a context window fills toward one it has not
reached yet, so treating them as the same kind of percentage was the actual
mistake, not the token count being exempt. The fix is a second, `ctx`-only pair
of directions — `ctx_pct_dir` and `ctx_tokens_dir`, tied together by
`ctx_dir_linked` unless someone wants them to disagree on purpose — defaulting
to "spent" for both regardless of what `pct_mode` is set to, so an existing
config does not inherit a mismatch from a setting that was never about `ctx` in
the first place.

## A19: three layers, and only one of them is about drawing
The code that was once `ui.rs` is split by what survives a change of GUI:

* **`runtime.rs` — what the widget knows.** The four sources, their file
   watching, the poller's channel, the merge (A03), age filtering (A07, A14),
   the history log (A17).
* **`view.rs` — what the widget says.** Snapshot and runtime state turned into
   the text, figures and verdicts a person reads: `detail_lines`, `rate_text`,
   `api_notice`, `field_name`, the pace delta, colour *as meaning*, and — since
   the move to Slint — the composition of the pill itself (`cells`, `bars`).
* **`ui.rs` — how it is drawn.** Painting, layout, the settings window, the
   palette, the window itself.

**Why:** the pill was about to move to another toolkit, where the drawing layer
is replaced wholesale and the other two have to survive unchanged. Left as one
file they could not have been handed over, because both were locked inside an
`App` that needed an `egui::Context` to exist — and, being unreachable from a
test, both were unverifiable as well. The move happened (A20) and the split held:
`runtime.rs` and `view.rs` were not touched by it.

**The invariant is mechanical, so it can be checked:**
```
grep -E "slint::|slint_interpreter::|Color32|FontId|Painter"   src/runtime.rs src/view.rs src/tray.rs src/serve.rs | grep -v "//"
```
Empty output is the condition. Not a style rule — the point of the split. The
toolkit named in it has already changed once; what it is checking has not.
`serve.rs` joined the list when it was written, because a second consumer of
these layers is the thing that proves they are consumable (A21).

**The pill's composition is what it says, not how it is drawn.** `view::cells`
returns the figures in order — text, a font *role*, a `Paint`, which group it belongs to and which field it is — and
`view::bars` the hairline under each group. No widths and no fonts: measuring
text needs a font, and under Slint the process no longer owns one at all, since
the theme lays out its own text. Moving this out of `ui.rs` *before* the front
end was replaced is the same trick `--serve` played on A19 — the composition
became testable while the old GUI was still there to compare against, and the
proof that nothing shifted is two `--demo` screenshots identical to the byte.

**Colour is a meaning, not a value.** `view::usage_tone` and `view::pace_tone`
return `Tone { from: Ink, to: Ink, t }`: which two roles a figure sits between
and how far along. `ui.rs` resolves that against its palette; a web front end
maps the same tones onto CSS variables. Returning `Color32` would have dragged
egui into the one module that most needs to outlive it. The rule from A15 is
unchanged and now lives in `view`: the tone is fed `used_pct`, never the shown
figure.

**The config stays in `App`, and `Runtime` takes it as a parameter.** Moving it
would have rewritten every `self.cfg` in the settings window — a thousand-odd
lines that are themselves due to be cut. It is also the more honest
arrangement: the settings UI owns the config and edits it in place, so a
`Runtime` holding its own copy would need telling about every change.

**What is left of the old orchestration in `ui.rs`** is only what is inherently
egui's: `pump_tray`, `apply_window_state`, `logic` and `ui` — the dozen lines
where viewport commands and repaint requests actually live.

**The tray was never egui's.** `tray-icon` talks to the OS directly, so
`tray.rs` moved across untouched and reports `tray::Command`s instead of acting
on them. A web front end needs the same tray for the same reason (A05).

## A20: the GUI is Slint, and the core stayed put
The pill is drawn by Slint, with its appearance loaded from a `.slint` file at
runtime through `slint-interpreter`. `runtime.rs` and `view.rs` were untouched
by the change; `ui.rs` is gone, and `eframe` with it.

**Why not stay on egui:** every appearance setting was a branch in the drawing
code, which is why the config had grown to 51 fields and `settings_body` to
1339 lines. Nothing a user wanted to change could be changed without a rebuild.
Both numbers came down with the move: nine of those fields are now the theme's
business, and the settings window is a repeater over a list.

**Why Slint and not a web front end.** Measured on this machine, private
working set:

| | processes | private |
|---|---|---|
| Slint + interpreter | 1 | 15 MB |
| egui/wgpu | 1 | 63 MB |
| Tauri/WebView2 | 7 | 70 MB |

A quarter of what runs today and a fifth of the browser-based option, in one
process instead of seven. Startup is 143 ms against egui's 64 ms — irrelevant
for something launched once a day. DPI is per-monitor correct and hairlines
snap to whole device pixels, with a `phx` unit when a physical pixel must be
named outright; transparency works.

**Note that the case against the web is weaker than the first measurement
suggested.** Summing *working set* across a process tree counts shared pages
once per process and produced "310 MB against 121" — by private working set the
gap is about 10 %. Slint was chosen for being better, not for the web being
three times worse. Compare private working set across a process tree; working
set counts the same shared pages once per process and flatters anything
single-process.

**The figures above are both front ends of the real widget**, release builds,
`--demo`, window visible, pointer elsewhere — not the spike. Memory is the
decisive column: 21 MB private against 61, and 51 MB working set against 110.

**CPU at rest is a wash, and anything sharper would be invention.** The
finished Slint build measures 0.109 s per seventy seconds against egui's
0.156 s, but repeated runs of one binary have come out 0.156 s and 0.172 s — a
spread as wide as the gap between the two toolkits. The earlier "0.0 s over ten
seconds" was not a measurement but the method running out of resolution: these
are `TotalProcessorTime` deltas over 70 seconds, window visible, pointer
elsewhere, and a shorter window cannot resolve the difference. And the first
release build
measured 1.9 s per seventy, twelve times egui, because the model was pushed to
the theme on every tick and every push is a repaint — see [[G23]]. Worth
remembering before quoting any of this: the number measured what the code did,
not what the toolkit costs.

**The binary figure cannot be taken yet.** The real executable carries both
front ends and weighs 25.5 MB; what it will be with `eframe` gone is not
measurable until it is gone.

**Transparency works under Tauri and did not under bare wry/tao.** Eight
configurations of the latter — two version pairs, with and without the frame
surgery, with and without an explicit `DefaultBackgroundColor`, and with DWM's
blur-behind trick — all painted an opaque backdrop; the same page under Tauri
let the desktop through. So what was missing was configuration rather than
capability, and the transparency risk against a future web front end is closed.

**How much transparency weighs.** It is wanted, not required — the owner, asked
on 2026-09-25, rarely runs the pill translucent. A renderer or a front end that
lost it would cost the `opacity` setting and the see-through corners round the
pill's rounded ends, which would show as a dark rectangle behind them (G05's
symptom); that is a price to state when choosing, not a reason on its own to
rule the option out.

**What the drawing layer costs today**, for scale: `draw` 411 lines,
`build_cells` 141, `pace_cells` 47, `ctx_cells` 39, `draw_details` 28,
`draw_bar` 27, `draw_track` 22, `Metrics` 80 — 795 lines that exist because
immediate mode computes every coordinate by hand. The pill used for these
measurements took 25 lines of `.slint` for six fields, a background, a corner
radius and a bar.

**What Slint costs.**
- *A smaller pool of people who can write a theme.* `.slint` reads like QML and
   is not hard to pick up — it is declarative, it borrows CSS vocabulary, and a
   language model writes it competently — but the "everyone already knows CSS"
   argument still does not transfer. The people who can edit a theme are those
   willing to learn a language for it, which is fewer than the people who would
   edit a stylesheet, so a theme ecosystem of the Waybar kind is unlikely rather
   than impossible.
- *A narrow path.* `slint-interpreter` sees 12 k downloads a quarter against
   Slint's own 467 k — roughly one user in forty — so fewer bugs are trodden.
- *A licence that is no longer unconditional.* **Decided 2026-09-11: the
   royalty-free terms.** The project keeps its own licence rather than going
   GPLv3, and in exchange the application must visibly acknowledge that it uses
   Slint — an `AboutSlint` widget or the Slint badge. The pill is 34 pixels
   tall, so that acknowledgement belongs in the settings window, which is the
   one surface with room for it. The royalty-free terms cover desktop; they do
   not cover embedded, which this is not.

**Why the interpreter in production, not the compiler.** Compiling a `.slint`
file means rebuilding the application through `build.rs`, which needs a Rust
toolchain — so a compiled theme is not a theme a user can change. Measured, the
interpreter costs 0.5 MB of memory and no startup time; the whole price is on
disk, in the compiler it carries.

*How large that price is was overstated here until 2026-09-12.* The line used
to read "one doubled binary, 24 MB against 12 MB", which compared a Slint
binary built with cargo's default release profile against this project's, which
is `lto = true`, `codegen-units = 1`, `panic = "abort"` and `strip = true`. The
profile alone was worth seven of those twelve megabytes. Now that the move is
finished the comparison is between two builds of the same program: **17.9 MB
with Slint against 12.1 MB with egui**, and that 5.8 MB buys the interpreter,
the compiled settings window and `std-widgets`.

**Compiling it instead was reconsidered on 2026-09-11 and rejected.** The case
for compiling is real: the generated accessors are typed, so a renamed property
is a build error instead of a `SetPropertyError::NoSuchProperty` nobody sees,
and most of the theme machinery below — the tolerant contract, the fallback
theme, the compiler diagnostics in the tray — exists only because the theme is
read at runtime. What settles it is what compiling takes away. Numeric knobs
survive either way, as typed properties fed from the config; what dies is
rearranging the pill — moving an element, dropping one, changing a shape — and
that is the part no configuration flag can stand in for. The type risk is
answered by routing every write to the UI through one function rather than
scattering `set_property` calls, which is also what would make a later move to
compilation cheap: the markup is identical, only the Rust glue changes.
Compiling the pill, and a web front end beside it, stay open as later work.

**The low entry barrier comes back through another door.** `--serve` (A21)
publishes the snapshot as JSON over localhost, which lets anyone write a widget
in real CSS for a bar they already run — Zebar, Rainmeter, eww. Zebar accepts no
third-party providers, but its widgets are HTML/JS and can `fetch`. That is the
answer to Slint's barrier: those who want CSS get a door that costs no memory,
because their webview is already running. `view.rs` already produces the values,
so the mode is a serialisation layer rather than a subsystem (A19).

**Tauri stays on the table** as a possible later move. Nothing in this decision
forecloses it: the layer split means a second front end costs the drawing layer
only.

**Authoring a theme** is done with `slint-viewer --auto-reload`, which watches
the file and redraws on save. It shows the UI without the application's live
data; `--load-data <file>` seeds public properties from JSON once, so a snapshot
dumped by the widget makes the preview realistic. Swapping the theme of a
*running* instance is not covered by the stable API — the interpreter loads one
at startup and replacing it means recreating the window — so a "restart" button
in the settings window is the intended answer.

## A21: the snapshot is published as JSON, and that JSON is a contract
`--serve` binds `127.0.0.1:47317` and answers `GET /` (and `/snapshot.json`)
with everything the pill would draw: both quota windows, the context figure,
the detail panel already worded, and the display conventions in force.
`--port N` moves it; `--no-api` leaves the endpoint alone.

**Why it exists.** Writing a `.slint` theme means learning a language first
(A20), so most people who want a different look will not write one. This is the other door: Zebar, Rainmeter and
eww widgets are HTML/JS in a webview that is already running, so a CSS widget
against this endpoint costs its author no new language and its user no new
memory. What Slint takes away, this gives back through a different opening.

**Why it went in before the move rather than after.** Building it is a test of
A19. Every value it publishes had to already exist in `view.rs`; anything it
had to work out for itself would have been a figure still trapped in the
drawing layer, and the whole premise of the move is that no such figure
remains. Two gaps turned up and were closed where they belonged —
`Source::key` and `Pace::key`, machine-readable twins of `label` and
`describe`, because prose is allowed to be reworded and a contract is not.
Doing this after `ui.rs` was gone would have found the same gaps with nowhere
cheap to fix them.

**Why a DTO rather than `Serialize` on the model.** The JSON is read by other
people's widgets, so renaming a field in `model.rs` must not break somebody's
Rainmeter skin. `serve.rs` owns the wire format and maps onto it; `schema` is
bumped when a field changes meaning or leaves, and never for an addition,
since a consumer that reads what it knows keeps working. It is 2 since a cell
lost `template` and `side` (A34).

**`view::Ink` and `view::Tone` do serialise**, which is not an exception to the
above but the point of them: a role name is exactly what travels. The server
publishes `"green"`, a CSS widget turns that into `var(--green)`, and neither
has to agree with the other about an RGB value — the same trick that lets
`ui.rs` hold the palette (A19).

**One process, and it does not share the poller.** `--serve` runs headless with
a `Runtime` of its own rather than as a thread inside the widget, so somebody
who only wants a widget in their own bar pays nothing for a GUI. The cost is
that a widget and a server running together are two pollers against one
undocumented endpoint, and the request budget (A13) is per process — `--no-api`
is the switch for that case, and the file sources need no token anyway.

**What it is not.** Loopback only, no authentication, and
`Access-Control-Allow-Origin: *` — without which a widget in someone else's
webview could not read it at all. The trade is real and worth stating: any page
the browser happens to have open can also read it. What is in there is usage
percentages, a model name and a session id; no token ever goes over it.

**The server is ninety lines of `std::net`**, not a dependency. One resource,
one client, no keep-alive — a single-threaded loop that let a browser hold the
connection open would be a server that answers once. Read timeouts are on the
socket for the same reason.

## A22: the settings are three files, and one list describes them all
`config.toml` is what a person tells the widget. `window.json`, next to
`state.json`, is what the widget remembers about its own windows. `schema.rs`
is the list of every setting there is.

**Why the window rectangles left the config.** Three of the old fifty-one
fields — the pill's position and the settings window's position and size — are
written by dragging, which is to say several times a second. The whole file was
rewritten each time, so a comment in it survived until the first nudge of a
window. That is the reason nobody could hand-edit the config, and it was a
worse reason than it looked: every *other* problem with editing the file was
downstream of it.

So the split is by author, not by subject. A person owns `config.toml`; the
widget owns `window.json`. The second is JSON because nobody is meant to read
it, and it is written whole and renamed into place like the state file and the
history log — with no lock, because there is only ever one writer.

**Why the config is written with `toml_edit`.** `toml::to_string` produces a
correct file and throws away everything a person put in it. Now the existing
document is edited in place: a value is replaced but keeps its decoration, and
both halves of that are needed, because they hold different things — the
comment trailing a value belongs to the value, the comment standing above the
line belongs to the key. The one-time move of a flat file into sections carries
both across with the key, since that migration is precisely the moment when the
most comments would otherwise be lost.

**An `f32` is written back as an `f32`.** Serialised through `f64`,
`scale = 1.1` comes out as `1.0999999046325684` — the same number and an
unreadable file. Values the schema calls floats are shortened to the briefest
form that still round-trips.

**Why the schema is data and not code.** It has two consumers already and the
important one has not been written yet.

* *The file layout.* `Section` says which table a key is written into, so the
   file reads in groups rather than in the order the struct declares its fields.
   The key itself never changes — moving it into a table is a change of
   position, not of name, so no value can be mistranslated by the migration.
* *The settings form, at the next stage.* The window that exists today is 1300
   lines of egui with the label, the tooltip, the step size and the layout
   braided together. When it is rebuilt in Slint, a form walked out of this list
   is a repeater over three kinds of row; rebuilt by hand it would be the same
   1300 lines in a second language. The strings were harvested into the schema
   *before* that stage on purpose: they exist nowhere else, and that stage
   deletes the file they live in.

**Four tests hold the two ends together**, because the failure they prevent is
the expensive kind — a setting that quietly stops persisting, noticed weeks
later as "it forgot what I chose". Every `Config` field must appear in the
schema exactly once and every schema entry must name a real field; every stated
bound must survive the accessor that enforces it; every choice must be a
spelling the config can actually read; and a config with every setting moved
off its default — built *from* the schema, so a field added later is covered
without anyone remembering — must survive the trip into sections and back
unchanged.

**The reading side takes either shape.** A section is hoisted rather than
mapped: the keys inside it are spelled exactly as `Config` spells them, so
flattening the document and handing it to serde needs no table of names that
could fall out of date. A flat file is recognised as flat and rewritten once.

## A23: the config is re-read while the program runs
`config.toml` is watched by modification time on the quarter-second tick that
already watches the state file, and re-read when it moves. Both long-running
modes do it: the widget in `logic`, `--serve` in its pump loop.

**Why.** A22 made the file worth hand-editing — the rectangles that used to be
rewritten on every drag are gone, comments survive a click in the settings
window. A file worth editing whose edits only appear after a restart is a file
people edit once and then stop, and the restart also throws away the poller's
budget and the history the burn rate is computed from.

**The trap is the program's own writing.** The widget saves the config from
some forty places — every switch in the settings window, every tray toggle — so
a watcher that cannot tell those from a person's edit either loops or
overwrites the click that caused it. The answer is to remember the exact text
written (`config::LAST_WRITTEN`) and treat a file holding it as unchanged.
Remembering a modification time instead would not do: a rename is not
guaranteed to leave a timestamp this process can predict, and two writes inside
one clock tick share one on Windows. Comparing text also gets a second case
right for free — a person who saves the file without having changed anything.

**What a re-read config costs.** The sources' remembered modification times
describe answers to the question the *old* config asked: which files to read,
which conversation to follow, how large a context window is. So they are
dropped whole (`invalidate_files`) rather than reasoned about field by field,
which cannot miss a setting the way a hand-written list of dependencies would.
The poller is not restarted: it re-reads its interval and budget from the
shared `PollerSettings` on its next lap, because a fresh thread would begin
with a request and editing a setting must never cost one (G14).

**That last rule needed a second guard.** Forgetting the modification times
makes the desktop history look new, and the old code asked the endpoint to
refresh whenever that file appeared to move. It now compares the readings
rather than the timestamp, so a file re-read into identical figures says
nothing to the network. The rule is the same one G14 states: a change of
setting is not an event about the quota.

**`window.json` is not watched.** Only the widget writes it, and only about
itself.

## A24: the theme contract — what the program sends, what the theme declares
`themes/pill.slint` is the widget's whole appearance, compiled into the binary
and written to `theme.slint` in the data folder on first run (A36) — which
for a build inside this repository is its `settings/`, and wherever `data_dir`
points when that is set (A28). From then on the copy on disk is what loads, so a change
to the theme contract's *meaning* (not only its shape) has to be carried into
that copy as well as into the default.

**The division is by whose question it answers.** The theme owns the palette,
the type, the paddings, the corner radius and the hairline thickness. The
program owns the wording (it arrives already localised from `view.rs`), which
colour a figure gets, and everything about how the window behaves. The theme
declares `palette_green`/`amber`/`orange`/`red`/`ink`/`dim`; the program reads
them back, resolves a `Tone` against them and sends a finished brush. So
changing a colour in the theme changes it everywhere that colour *means*
something, and nowhere else — the same split as A19, made concrete.

**The theme reports its own natural size.** `wanted_width` and
`wanted_height` are `out property <length>` bound to the layout's preferred
size. This is not a convenience: nothing in the process can measure a string
any more, because the theme lays out its own text, so a content-sized window is
only possible if the number comes back out. **The value is a frame behind** —
an `out` property bound to a layout is recomputed when the theme next lays
itself out, which is after the callback that changed the text returns — so the
window is sized from the previous tick's layout. A quarter-second of lag is
invisible; a window sized to the previous sentence is not.

**No window policy in the theme.** Not `no-frame`, not `always-on-top`. All of
it is applied from Rust over the HWND (`win.rs`), so a broken or hostile theme
cannot produce a window that cannot be closed, cannot be dragged, or sits below
everything. The theme also cannot turn click-through on.

**Tolerance has two halves, and only one of them is optional.** A property the
theme does not declare is simply not set, so a theme written against an older
version keeps working with fewer values. A **struct** is different: Slint
matches it field by field and refuses the whole assignment if it has drifted,
which leaves the pill blank. So `theme_api` is checked before the theme is used
at all — the theme states which contract it was written against, and a mismatch
means the built-in copy draws instead, with the two version numbers on stderr.
`THEME_API` is bumped whenever `PillCell`, `PillGroup` or `PanelLine` changes
shape; it is 3 since `PillCell` lost `template` and `side` (A34). Failures to
set a property are reported once each rather than dropped (G22).

**A broken theme is never a dead application.** It falls back to the built-in
copy and says so. The file is not overwritten — it is somebody's work — so the
recovery is to delete it, and the message says as much.

## A25: the pill and its panel are two windows
The detail panel and the field hint are a window of their own, placed under the
pill by the program, rather than part of the pill's window as they were under
egui.

**Why.** With one window, opening the panel grew the window and — near the
bottom of the screen — moved it. The pill's rectangle therefore depended on
what the pointer was over, and that is a feedback loop: the pointer's position
is window-relative and only refreshed by real mouse events, so a still pointer
read as being somewhere it was not, the hint closed, the window shrank back and
it opened again. That is G16, and it cost a day. With two windows the pill's
rectangle never changes while the panel opens, so the loop has no way to start.

**What survives of A06.** The flip is still there — the panel opens downwards,
or upwards when the pill sits too near the bottom of the work area — and so is
the slide left when a panel wider than the pill would hang off the right edge
of the screen. Both are now one `SetWindowPos` on a second window instead of a
resize and a move of the first.

**The panel is never hidden.** It is parked at −30000, −30000 and moved back
when it is wanted, because `hide` followed by `show` restores the window styles
and takes `WS_EX_NOACTIVATE` with them — and the `show` that would put it back
is the same call that steals the focus (G21). A window that is never hidden has
nothing to restore.

**Hover is read from the system cursor**, not from either toolkit, and covers
both windows: the pointer moving from the pill onto the panel to read it must
not close the panel. The theme reports only which figure was *entered*, never
that one was left, so crossing the gap between two figures does not blink the
name off.

## A26: one indent width, three spaces, enforced rather than remembered
`rustfmt.toml` sets `tab_spaces = 3` and `.editorconfig` sets `indent_size = 3`
for every file. Every existing file — `.rs`, `.slint`, `Cargo.toml`, the
`.claude/settings.local.json` permissions list, and the hanging indents in
these docs and `CLAUDE.md` — was reformatted to match.

**Why.** Before this there was no convention at all, and it showed: `.rs`
files sat at the rustfmt default of 4 spaces while the two `.slint` files
disagreed with each other, `ui/settings.slint` at 3 and `themes/pill.slint`
at 4. A convention that is only ever a habit erodes the moment a second
person, or a second session, touches the code without having absorbed it. A
convention a tool enforces cannot drift.

**Why 3 and not 2 or 4.** No technical reason — it is a preference, applied
uniformly rather than argued for per language.

## A27: the poller remembers itself across a restart
`api_poll.json`, next to `state.json` and `window.json`, holds two things the
poller (`src/api.rs`) used to forget every time the process restarted: when
it last actually asked (`last_request`), and its last successful reading
(`five_hour`/`seven_day`, reusing `state::StoredWindow`'s shape). Written
whole and swapped in, one writer, no lock — the same convention as
`geometry.rs`'s `window.json` (A22's "the widget's own memory, not a
setting").

**Why `last_request` matters.** `decide()` (A13's request budget and
schedule) already refuses to ask more often than `api_interval_secs`, but
that schedule lived only in the poller thread's own memory. A restart is a
fresh thread with no memory, and `decide()` treats that the same as "never
polled" — go at once. During active development the widget restarts far
more often than the interval, so every restart asked again immediately,
making the schedule pointless exactly when it mattered most.

**Why the reading matters too.** Fixing the request meant the pill sat empty
until the next poll came in — sources are switches (A14), and with
`bridge`/`desktop` off, the API was the *only* source that had ever carried
the quota windows at all. Persisting the last answer means a restart shows
it immediately, aged honestly: `observed_at` is untouched, so the existing
staleness rules (A07, A14) grey it out and then drop it exactly as they
would a live reading nobody refreshed in a while. Nothing here invents a
new notion of freshness.

**Absent fields never overwrite present ones**, the same rule `Window::merge`
follows (A03): a reading that only renews one window must not blank out the
other one merely because it wasn't due yet.

## A28: `data_dir` moves every file this program writes, in one hop
> **Revised by A36:** the fixed location is now `settings` beside the
> executable, not `%APPDATA%`, and `data_dir` must name a folder that exists.
> The one hop and `relocate` stand; the paragraphs below describe the
> `%APPDATA%` years.

Some environments redirect a packaged program's usual per-user folder to a
private copy of it — edits made there through the normal, unpackaged route (a
text editor, another process) never reach what the packaged one reads, and
neither sees the other change. `config::base_dir()` gives a way around it: a
setting that, once pointed at an ordinary folder outside that redirected
place, becomes where `config.toml`, `theme.slint`, `state.json`,
`window.json` and `api_poll.json` all live from then on.

**One hop, not a chain.** `base_dir()` always starts at the fixed location
(`default_base_dir`, the same path `config_path` used to be) and reads
`data_dir` from whatever sits there. If it names a folder, that folder is
used — and its own `data_dir`, if it has one, is never read. Two configs
pointing at each other is a problem this only has to not create, not solve.

**A folder that cannot be made falls back to the fixed location**, the same
rule A24 applies to a theme that fails to compile: a setting typed wrong must
not be a widget that will not start.

**`relocate` runs once, on the way in.** The moment `data_dir` changes from
empty to something, `slint_ui::changed` copies every file this program owns
into the new folder — never overwriting one already there — and rewrites the
fixed location down to the one line that matters from here on, then restarts
the same way `Change::Restart` already does (state already loaded into this
process would otherwise sit at the old location until the next launch
regardless of what `config.toml` now says). Copies, not moves: a folder that
turns out to be wrong should cost nothing at the original. Changing the
setting a second time is not handled the same way — the new value saves into
whatever `config.toml` is currently in effect, which is the moved copy, not
the fixed location `base_dir` actually reads `data_dir` from, so a further
move has to be set by editing the file at the fixed location directly (the
tip in `schema.rs` says so).

**Two bases became one.** `state_dir` used to be `dirs::data_local_dir()`,
a different physical folder from `config_path`'s `dirs::config_dir()` — one
`data_dir` covers both now, since `state::state_dir` is `config::base_dir`
too. A person pointing this at somewhere they can reach need remember only
one folder, not two.

## A29: the weekly days are counted by the calendar or by the clock
The day figure beside the weekly limit (`5d`) and the daily allowance beside
that (`20%`) are one division: what is left over the days left. `days_by`
decides what a day is. `calendar`, the default, counts whole local dates and
leaves both ends of the run to `count_today` and `count_reset_day`, because
neither end is obviously a day. `time` counts nothing: the days are the
seconds to the reset over 86 400, written `4d7h` the way a weekly pace gap is,
and the two switches do nothing.

**The pace figure is not touched by this setting.** `pace_target` has always
measured the exact time to the reset against the whole week, in both modes.
By time the allowance becomes the same comparison as the pace ratio: an even
burn is 100% over seven days, about 14.3% a day, and the allowance is exactly
that over the ratio — 54% spent with 3 d 5 h left is `1.0×` (0.996) beside
`14%` (14.29 / 0.996). By the calendar the two drift apart by the rounding of
the divisor to whole days.

**The calendar stays the default.** A config written before `days_by` existed
reads the same figures it always did; the clock is something to switch to, not
a change that arrives with an update.

**By time there is no floor of one day.** The calendar count is whole, so its
only special case is zero — everything left is for now. By time the count goes
below one, and dividing by it anyway is what keeps the two cells agreeing: half
a day and 40% left is `12h 80%`, a rate that spends exactly the 40% by the
reset. Flooring at a day would print `40%` beside `12h`: the remainder, not a
rate per day, and a figure the cell beside it no longer explains. The one guard is a cap at 100%, so the
cell does not grow into four digits in the last minutes; one function,
`pace::daily_allowance`, serves both modes, since for whole days the cap and
the zero case are the old behaviour.

**The cell is as wide as the figure on show** (A34). Before that it reserved
for the shape on show, `0d0h` for `3d5h`, because a fixed `0d00h` left the
short forms floating in a hole.

**By time the count has a second, shorter form**, `days_form = "short"`:
`3'16` for `3d16h`. It is a separate setting rather than a third `days_by`,
because it changes how the figure reads and nothing it divides by; the
settings window hides it unless `days_by` is `time`. It applies to the figure
only — the pill, the status line and `days_text` — and the hover hint keeps
words. The weekly pace gap in time form has the same choice as a setting of
its own, `pace_form` (A16), since it does not depend on how the days are
counted.

**The short form's separator is `'`, not a colon.** It started as `3:16`, and
beside the five-hour `2:19` a colon says hours and minutes, which a weekly
figure is not. The apostrophe is as narrow, is in every font, and reads as
nothing but a divider. `weekly_sep` makes it a choice, shared by both weekly
figures: an empty one falls back to `'`, since `316` says nothing, and more
than three characters are cut, so a stray paste cannot stretch the pill. A
config written as `colon` still reads, as `short` (`serde(alias)`), and a
colon typed into `weekly_sep` gives the old look back.

**The hours are as short as they get unless asked**: `2'7`, not `2'07`.
`weekly_pad` pads them. The unpadded form costs a width change once a day, as
the hours cross ten; with the cell sized to the figure on show (A34) the pill
moves by a digit either way, and the shorter reading was chosen over the
steadier one.

**`days_left` in the JSON stays an integer.** A21 makes the type part of the
contract; by time it carries the whole days, and the pill's text travels
beside it as `days_text`, with `display.days_by` saying which count is on
and `display.days_form`, `display.weekly_sep` and `display.weekly_pad` how it
is written.
The exact seconds were already there in `resets_in_secs`.

## A30: figures worked out from the time step on the minute
The pill is rebuilt every 250 ms, and until this the time-derived figures were
read off that exact second. Each then changed whenever its own rounding
happened to cross: a pace in time form rounds to the nearest minute and so
turned over half a minute after the countdown beside it, which rounds up — a
pace that looked like it lagged the clock. Over pace, the colour's position
between orange and red slid on every tick, and a model that differs is a
repaint (G23), four a second.

**One clock for all of them: the start of the current minute.**
`pace::on_the_minute` is applied inside the functions that turn time into a
figure — `pace_target` (every form of the pace, its verdict and its colour),
`days_until` and `fmt_days_until` by time (A29), `history::runs_out_at` (the
forecast) — rather than by their callers, so the pill, the panel, `--bridge`
and `--serve` cannot disagree and no new caller can forget it. They all move
together, once, as the system clock turns over.

**Not everything moves to the minute.** The countdown already rounds up to it
and is left as Claude Code writes it (G10). The age of a reading and the time
to the next poll keep their seconds on purpose: a ticking second is how a live
reading is told from a stuck one. The percentages themselves change only when
a source reports, which no clock can help — a new reading still moves the pace
by three minutes a point on the five-hour window.

**By time the day count rounds up too**, like the countdown: with 4 d 6 h 20
min to go there is more than `4д6ч` left, and `0ч` means over.

## A31: a hint names the figure, then says what it is now
Pointing at a figure used to give its name and nothing else — `days until the
weekly reset`. With `detail_mode = never` that name was all anyone got, and
the question behind a pointed-at `5d` is rarely "what is this" for long: it is
"which day, what time". So a hint is now a name and a few lines under it,
worked out by `view::field_detail` from the same snapshot as the pill: the date
and hour of a reset and the run of days counted, the pace's verdict in words
and in every unit, the other reading of a percentage and its source's age.

**The theme contract does not change.** The theme is handed each cell's name
and hands the hovered one back; `Field::named` turns that name back into the
field by matching it against `field_name`, which is why no two fields may share
a name — a test walks all of them in both languages. Adding a key to the cell
struct would have been tidier and would have put every theme on disk one
contract version behind (G22), drawing nothing, for a lookup this cheap.

**Worked out every tick, handed over only when it changed.** A hint carries an
age that ticks by the second, so it cannot be built once when the pointer
arrives. The panel's lines are compared with what it was last given before they
go to the theme, for the reason G23 gives; before this they were pushed on
every tick while the panel was up.

**Names start with what they measure.** `Left of the 5-hour limit` rather than
`5-hour limit: left`, the verb first so the two readings of one figure are told
apart at the first word. The detail under it keeps the same order — `spent 24%`,
not `24% spent` — so the eye reads each line the same way.

## A32: a commit has to pass the checks, and a hook says so
`tools/hooks/pre-commit`, enabled with `git config core.hooksPath tools/hooks`,
refuses a commit when `cargo fmt --check` or `cargo clippy --all-targets -- -D
warnings` fails, and — for a commit touching `project-workbook/` — when the documentation
linter or the index check does.

**Why a hook and not a count.** Clippy sat at eleven findings for months, none
of them bugs, and the rule was to count them per file before a change and after.
That held only while someone remembered to count, and a count taken twice with
different flags reads nineteen against eleven with nothing having changed —
which is how a new finding slips in unnoticed. At zero there is nothing to count:
any finding at all stops the commit. The same argument as A26, applied to the
lints instead of the indent.

**Fixed, never allowed.** The findings were cleared by changing the code, not by
`#[allow]` or a lint table; an allowance is the old count with extra steps.

**What it does not cover.** The tests are not run — they are quick, but a hook
slow enough to be skipped is worse than one that is not. The checks read the
working tree rather than the staged snapshot, so a half-staged file is judged by
its other half. And `core.hooksPath` is local to a clone: a fresh checkout does
nothing until it is set, which README says.

**Where the standard is looked for.** The linter and `make_index.py` belong to
the documentation standard, a separate repository. The hook looks for it beside
the *main* checkout — the parent of `git rev-parse --git-common-dir`, then
`../0_project-workbook-method` — and `WORKBOOK_STANDARD` overrides that. Where it is
absent the hook says so and lets the commit through, since a contributor without
it has nothing to run. It used to look beside the working tree's own root, which
is the same place for the main checkout but not for a linked worktree under
`.claude/worktrees/<name>/`: every commit from a worktree printed "not found"
and skipped the docs check, a pass that looked like a check. The common git dir
is the main checkout's `.git` whichever tree commits, so both now resolve to one
place. A relative `WORKBOOK_STANDARD` is still read from the committing tree's
root.

---

## A33: the settings window is pages of cards, and the schema says where each setting sits
The window has a list of pages down its left side and, on each page, cards of
settings; under each setting's name stands the first sentence of its tip. A
setting names its card (`Group`, which belongs to a `Page`) in `schema.rs`, and,
where it only matters while another setting has some value, what it `Needs` —
with whether it is greyed out or hidden when that is not met. `settings_ui.rs`
walks that into a model; `ui/settings.slint` draws a row by the control it asks
for and never knows which setting it is.

**Why not a list.** The window before this was every setting in one scrolling
column, in the order of the file, with four arrow buttons on every number. It
read as a dump of the file, not as a place to change something: a source's
switch, its expiry and its polling were three sections apart, and nothing said
what a row did without hovering over it.

**Why not two columns.** Three layouts were mocked up on the real settings and
their real wording before any Slint was written: pages with cards, two columns
with everything on one screen, and tabs over a narrow window. Two columns did
not fit even as a mockup — the last card of one column ran under the footer —
and every setting added later would have meant balancing the columns again by
hand, which is layout work for a form that is supposed to be generated. Tabs
kept the bare list inside each tab. Pages of cards grow by a card or a row, and
a page longer than the window simply scrolls.

**Why a page is not a `Section`.** `Section` groups the file by what kind of
thing a key is, so someone editing it finds the thresholds together. A page
groups by what someone opening the window is trying to do. The two orders
disagree — a source's expiry is a `Sources` key and sits on the source's card,
and the days count is `Behaviour` and sits on the card of the day figure — and
neither is wrong, so both exist. Within a card the order is the order of `ALL`,
which does not reach the file: `config.rs` writes keys in the order `Config`
serialises them.

**A card is one thing on the pill, and holds all of it** (2026-09-27). The
first cut of the pages split by kind of setting: the switches that put a figure
on the pill on one page, how that figure is worked out and coloured on another
("Pace and thresholds"). Changing the day figure meant two pages, and the pace
three cards. Now the **Limits** page has a card per figure — percentages (which
way they count, and the amber and red thresholds), pace (its form, the weekly
form, whether each limit carries it, the tolerance), the five-hour reset, the
days to the weekly reset with the daily allowance that divides by them, and the
bars. A setting shared by both limits sits once, on its figure's card; the one
thing that differs per limit, whether the figure is shown, is a switch per limit
on that same card. The detail panel is a card on **Appearance**, and the one
expiry common to every source, `drop_after_secs`, the last card on **Sources**.
Only `Group`, `Page` and labels changed: no key moved in the file.

**What `Needs` is for.** A setting that does nothing at the moment is noise, but
the two cases differ. A switched-off source still has an expiry, and seeing it
greyed says "the source is off" rather than "the setting is gone": `Dim`. A
manual window size means nothing until "manual" is chosen: `Hide`. A tested rule
keeps a need on the same card, above the setting that has it, and one level deep,
so the form can indent the dependent row without being told to.

**The control follows from the kind.** A switch for a bool; buttons side by side
for a choice of up to three and a menu past that; a stepper for a duration or a
token count, whose range runs from half a minute to a day and would put every
value anyone uses in the first pixels of a track; a slider for everything else.
Nothing about drawing is written against a setting.

**Rows are rewritten where they stand.** Every click rewrites the rows in place
rather than handing the window a new model: a new model rebuilds the page under
the pointer, losing the scroll position and ending a drag. For that the number
of rows on a card never changes — a setting that does not apply is marked
hidden, not left out — and the value controls are drawn in the `.slint` file with
no state of their own. A stock `Switch` or `Slider` writes its own value when
used, which cuts its binding to the row, so from then on it shows what was
clicked rather than what was saved. The status line of the usage endpoint changes
every second and is rewritten on its own, and only when its words change (G23).

**A slider writes the file once.** While it is dragged the change goes to the
config in memory and the pill follows; the file is written on release, not a few
dozen times a second. Text is applied on Enter: applied per keystroke, the first
letter of a path typed into `data_dir` was already a folder to move every file
to, and a restart.

## A34: a cell is as wide as its text, and one edge of the pill stays put
Every cell of the pill is exactly as wide as the text it shows. The space
between two figures is the gap the theme names for that kind of join —
`cell_gap` between figures of one limit, `rule_gap` either side of the rule
inside a split pace, `group_gap` either side of the divider between limits,
nothing inside `Gap::Tight` — and nothing else. The pill changes width when a
figure gains or loses a character, and `anchor` (`left` or `right`) says which
of its edges stays where it was put.

**Why the slots went.** Each cell used to reserve the width of the widest value
it could ever hold (`template`) and centre its text in it. The visible space
between two figures was then that slack plus `cell_gap`, so it changed with the
value: `5%` in a `100%` slot had a hole on each side and `100%` had none, and
the pill read as unevenly spaced however the gaps were tuned. Reserving by the
shape on show (A16, A29) had already given up most of the stability the slots
were for — the pill already moved when a gap gained a digit — and Segoe UI's
figures are one width, so a cell as wide as its text holds its width while the
digits move and changes only with their count. What changed width before still
does; what it cost in spacing is gone.

**The price is a moving edge, and `anchor` picks which.** A pill parked at the
right of a screen that grows rightwards walks off it, and one that shrinks
leaves its right edge wandering. With `anchor = "right"` the right edge holds and
the pill grows leftwards. The default is `left`, which is what the pill always
did.

**The edge is held from a record, not from the window.** `window.json` keeps
the pill's top-left and, since this, its `width` at the moment the position was
written; the right edge is `x + width`. `resize` works out the new `x` from that
record (`geometry::anchored_x`) and writes the record back, on every width
change and at the end of every drag. Reading the window's rectangle back each
time looks equivalent and is the trap A06 already recorded: the rectangle lags
the command that moved it. The rectangle is asked once, for a file written
before `width` existed. Because both anchors keep the record current, switching
`anchor` moves nothing.

**Moved and resized in one call.** For a right anchor the new position and size
go to the window in one `SetWindowPos` (`Frame::set_rect`), then Slint is told
the size it now has; a size and then a move would show one frame grown the
wrong way. If Slint's own idea of the size nudges the width, the right edge is
put back.

**The arrow cell is gone when there is no arrow.** It used to be drawn empty at
an even burn, holding its sliver open; with nothing reserved there is nothing to
hold, and the figure is one cell.

**Two contract versions moved with it.** `PillCell` lost `template` and `side`,
so the theme contract is 3 (A24) and a theme written for 2 falls back to the
built-in copy with the two numbers on stderr (G22). `view::Cell` travels in the
`--serve` JSON as it is, so its `schema` is 2 (A21).

## A35: an expired token is handed back to the CLI to renew, never renewed here
When the poller finds the token expired and `api_revive_token` is on (the
default), it runs `claude -p /usage --no-session-persistence --safe-mode
--output-format json` with no window, from the temp directory, waits up to 60 s,
reads the token again and asks the endpoint in the same lap. A failed attempt
keeps the next one away for 30 minutes. `--revive-token` does the same by hand
and prints the expiry before and after.

**Why the CLI and not us.** The refresh token is single-use: a renewal made
here and written into Claude Code's credentials file would leave the CLI holding
a dead one, and writing to another program's file is a trade the project had
already declined. Asking the CLI keeps the file its own.

**Why `/usage`.** `claude auth status` does not renew anything: it reports
`loggedIn: true` over an expired token and leaves the file as it was. A normal
prompt would renew it but spend quota and open a five-hour window. `/usage` is
a built-in command that asks the same endpoint the widget does, so the token is
renewed with zero turns, zero tokens and zero cost — measured on the JSON result
of a real run.

**Why those flags.** `--safe-mode` keeps the user's hooks and plugins out of the
run; `--no-session-persistence` keeps it out of the transcripts the session
source reads (A18); the temp directory, because `data_dir` (A28) can sit inside
a project and the CLI would treat the run as that project's. 6–10 s per run.

**Why the verdict is read from the result.** A run whose sign-in fails still
exits with 0 (G32), so success is the result's `is_error` being false, not the
exit code. A refusal of the refresh token itself is not something a revival can
mend: the CLI then writes an expiry of zero, the poller stops starting it, and
the panel says to run `claude auth login` once.

**Why 30 minutes.** A CLI that could not renew once — signed out, offline, not
installed — will not do better a lap later, and each attempt is a process start
of several seconds. The last attempt is kept in memory only, so a restart may
try once more.

## A36: our files live beside the executable, never in `%APPDATA%`
Every file this program keeps — `config.toml`, `theme.slint`, `state.json`,
`window.json`, `api_poll.json`, `samples.json` — sits in a `settings` folder
beside the executable. A build inside this repository's `target\<profile>\`
(two levels up, a `Cargo.toml` naming `ccd-usage-monitor`) uses the
repository's own `settings/` instead. `data_dir` still moves everything in one
hop (A28), but only to a folder that already exists; anything else is ignored.
`config::default_base_dir` is the one place this is decided.

**Why not `%APPDATA%`.** A program started by a packaged one — the Claude
desktop app, or a Claude Code session running inside it — sees a private copy
of `%APPDATA%`, not the real folder (G33). That is what A28 was built around,
and on 2026-10-02 it bit again: a widget restarted from a session read a copy
of the config with demo figures on and a theme one contract version old. The
redirection covers AppData only, so a folder beside the executable is the same
folder whoever starts the process: Explorer, a session, or the CLI running
`--bridge` under the desktop app.

**Why the repository rule.** Here the widget runs from `target\quick` and the
status line calls the bridge from `target\release`. Two folders beside two
executables would split them — the bridge writing a `state.json` the widget
never reads — and a full `cargo clean` would take the settings with it. The
repository's `settings/` was already the live data folder (through `data_dir`)
and is ignored by git except for the theme.

**Why only an existing folder.** A mistyped `data_dir` used to be created, and
so became a second, empty set of settings that the widget then quietly used.
Now it is not taken, and the settings window clears it.

**No migration.** Nothing is read from the old `%APPDATA%\ccd-usage-monitor`
any more; where it holds settings worth keeping, they are copied by hand. The
desktop app's own history (`%APPDATA%\Claude`, A14) is another program's file
and is still read from there.

**Where the folder cannot be made** — an executable installed somewhere it may
not write — the temp folder is used and the fact goes to stderr: a widget with
settings that do not survive a cleanup beats one that will not start.

## A37: in the desktop app a Claude Code mod stands in for the status line
`mod/` is a Claude Code plugin of function hooks (a "mod"). It does two jobs, and
for both of them the executable decides what anything says:

- **It feeds the bridge.** On every `session.measure`, which the engine raises
  after each main-thread turn and whenever a rate-limit window moves a whole
  point, the mod builds the status line's own JSON (`StatusLinePayload`) out of
  the engine's figures and pipes it into `ccd-usage-monitor --bridge`. The only
  translation is `resetsAt`, an ISO 8601 string, into epoch seconds.
- **It pins the pill's line under the prompt.** `$.ui.status` takes the stdout of
  `ccd-usage-monitor --line --session <id>`: `view::cells` joined by
  `view::plain_line`, built the way `--serve` builds its snapshot, with that
  conversation's context and no network. It runs on `session.start`, after each
  measurement and every 60 seconds, so the countdowns move while the session is
  idle.

It acts only where something other than a terminal draws. A terminal runs the
real status line, and a second copy of the line there would be noise.

**Why.** The desktop app never runs the status line (G06), so desktop sessions
reached the widget only through the history file and the transcripts. The
engine itself has the figures: `$.session.usage()` and `session.measure` carry
`five_hour` and `seven_day` with their reset times, the context window's stated
size, its fill, and the model's full id. A mod is the one place in a desktop
session where code of ours can read them. That closes the last gap the status
line was needed for: the window's size is now stated by the engine in every
session where the mod is loaded, not learned from one terminal run.

**Why `--line` and not `--print`.** `--print` is the status line's own
rendering (`bridge::render`). It carries a daily allowance the pill does not
show and lacks the time of day the pill does show, and it reads only the state
file and a transcript. The line under the prompt is meant to be the pill as the
person set it up.

**Why the timer never feeds the bridge.** `state.json` prefers the session that
reported last. An idle session feeding the bridge once a minute would become that
session every minute and take the pill's context figure away from the
conversation actually in use. Only a measurement, which follows real work,
feeds it.

**Rejected:** the mod writing `state.json` itself. Locking and merging belong to
`state.rs`, and two writers of one format would drift.

**Finding the executable.** First the plugin option `exe`, stored in
`~/.claude/settings.json` under `pluginConfigs["ccd-usage-monitor"].options`.
Otherwise the executable in a `statusLine` command that ends in `--bridge`.
Whichever it is must be a build that knows `--line`. An older one would take
`--line` for no flag at all and start the widget.

**Loading.** `CLAUDE_CODE_PLUGIN_DIRS` in the `env` block of
`~/.claude/settings.json` names `mod/`, so every session the desktop app starts
loads it from the repository as it stands.

**The cost of the choice.** The mod API is early access (Claude Code 2.1.293
when this was written) and moves between releases. The mod is kept thin on
purpose, and its contract with the executable is the status-line JSON, which
`state.rs` already tests.

## A38: `turn-meter`: a turn's share of the limits is its dollars over the price of a point
`turn-meter/` is a second Claude Code mod, separate from `mod/`. Under each
reply it draws dim rows (`ui.render` on `AssistantMessage`, matched to the
reply's last text block the way a third-party mod, Turn Usage, does it). The
first row holds the context fill (`ctx 21% · 212k`), the turn's share of the
five-hour and weekly windows and its dollars. The second holds the turn's tokens
by kind and a group for the subagents that finished during the turn. The third
holds its duration, request count and model. A row with nothing to show is left
out; one long row wrapped at the model's name in a narrow window.
Each part is a boolean `userConfig` switch, stored in `~/.claude/settings.json`
under `pluginConfigs["turn-meter"].options`. The window percentages themselves
are not shown: the pill shows them.

**The turn's dollars** are the session ledger (`$.session.usage().cost.usd`) at
`turn.complete` minus its value at `turn.start`. The ledger is the engine's own,
at API list prices, over every request of the session. So the turn's subagents,
side calls and other mods' forks are in it with no price table of ours.

**The share** is those dollars over the price of one point of the window:
`usdPerPct5h` and `usdPerPctWeek`, with starting values 0.39 and 2.85. Those
come from fitting about 22,500 requests in this machine's transcripts against
the desktop app's history of the windows, 2026-09-23 to 2026-10-12. Measured
against the windows, list-price dollars spread less (IQR/median 0.12) than raw
token sums (0.4–0.6). The usual Turn Usage line is the difference of two whole
percentages, which shows most turns as `<1%` or `+1%`. A price per point has no
such step, and other sessions' spend does not leak into it.

**Learning the price.** Each session keeps a journal in
`turn-meter/data/<session id>.json` (ignored by git). On every `session.measure`
it records the ledger when it grew, and each window's reading when it moved. In
each window, the moments a reading first shows a new value are where it sits
closest to a whole point. The spend of all sessions from the window's start to
each such moment, against the value, is a line. Its least-squares slope is that
window's price of a point. A window counts with a rise of 8 points (five hours)
or 3 (week), R² ≥ 0.9, and a slope within 2× of the configured price. The price
in use is the median of the last ten windows that count. Below three windows it
is blended with the configured one, and the line marks the shares with `≈`.
`/turn-meter` prints the fits, and `/turn-meter reset` forgets them.

**Why files and not `$.store`.** `$.store` is one JSON file per plugin under
`~/.claude/plugins/store/`. Parallel sessions writing it would race on the whole
file. One journal file per session has one writer. The learned price is never
stored: it is a pure function of the journals, computed again when a window
moves.

**Known bias.** Spend this machine never sees (claude.ai, the phone, a session
without the mod) moves the windows without moving the journals. The price then
reads low. The 2× bound and `autoCalibrate: false` with a hand-set price are the
guards.

**Rejected:** the share as a difference of whole percentages, which is Turn
Usage's way and is mostly noise. A price table of our own, while the ledger
exists. Writing learned prices into `settings.json`, which is the person's file.

**Loading.** `CLAUDE_CODE_PLUGIN_DIRS` names `turn-meter/`, as it does for the
other mods. Run `claude plugin validate turn-meter` and `claude plugin test
turn-meter` after editing it.
