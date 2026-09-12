# Architecture

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
The bridge writes `%LOCALAPPDATA%\ccd-usage-monitor\state.json`; the widget polls its mtime
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

**Consequence:** each figure is drawn in a fixed-width slot, sized to its widest
possible value (`100%`, `00:00`, `00d`), so the pill does not jitter as digits
change.

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

## A16: the pace is one comparison, said in two units and two places
Usage against the share of the window already elapsed — `used_pct - pace_target`
— is the whole of it. Everything about pace is a rendering of that difference.

**Two units, one decision.** The gap in points (`▲14%`, as claude-pace says it)
and the ratio (`▲2.0×` from `pace_ratio`: how many times slower the rest has to
go, the form that says what to *do*) are the same comparison twice. `PaceView`
is every arrangement of them — either alone, both in either order, both with a
rule between, both run together behind a written pipe, or none — in one list,
because which of them helps is taste and both at once is a legitimate answer. It replaced an on/off switch plus a
two-way unit toggle: three states of two settings that could not express "both".
The arrow, the verdict word and every colour come from the points whatever the
form on screen, so no two of them can disagree. `pace_ratio` is clamped at ±9.9:
a window with nothing left divides by zero, and `inf` is not a reading.

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
painter both have to consult. The width the pill reserves is then literally the
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

**Why:** the pill is to move to a web front end, where the drawing layer is
replaced wholesale and the other two must survive unchanged. Left as one file
they could not be handed over, because both were locked inside an `App` that
needed an `egui::Context` to exist — and, being unreachable from a test, both
were unverifiable as well.

**The invariant is mechanical, so it can be checked:**
```
grep -E "egui::|eframe::|Color32|FontId|Painter" src/runtime.rs src/view.rs src/tray.rs | grep -v "//"
```
Empty output is the condition. Not a style rule — the point of the split.

**The pill's composition is what it says, not how it is drawn.** `view::cells`
returns the figures in order — text, the widest string each will ever hold, a
font *role*, a `Paint`, which group it belongs to and which field it is — and
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

## A20: the GUI moves to Slint, and the core stays put
The pill will be drawn by Slint, with its UI loaded from a `.slint` file at
runtime through `slint-interpreter`. `runtime.rs` and `view.rs` are untouched by
the change; `ui.rs` is replaced.

**Why not stay on egui:** every appearance setting is a branch in the drawing
code, which is why the config grew to 51 fields and `settings_body` to 1339
lines. Nothing a user wants to change can be changed without a rebuild.

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
three times worse (`ai_docs_env` EG45).

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
is `lto = true`, `codegen-units = 1`, `panic = "abort"` and `strip = true`.
Built the same way, the Slint spike is **14.9 MB against the widget's 12.1 MB**
— the profile alone accounted for seven of the twelve megabytes the toolkit was
being charged for. The two are still different programs, so the remaining
difference is an indication rather than a measurement, but "doubled" was
wrong.

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
since a consumer that reads what it knows keeps working.

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
