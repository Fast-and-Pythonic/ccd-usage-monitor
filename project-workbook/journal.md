# Journal

Append-only, newest on top. Why the work went the way it did — the substance lives
in the registers and is not restated here. `git log` has what changed line by line.

## Index

- **2026-10-17** — `turn-meter` keeps finished windows, and refuses jumps its journals never paid for
- **2026-10-15** — prices per model in `turn-meter`, and `limit-test` to measure them
- **2026-10-15** — `turn-meter` draws each turn's spend under the reply
- **2026-10-10** — a Claude Code mod feeds the bridge in the desktop app and pins the pill's line
- **2026-10-03** — the pill can carry the time of day
- **2026-10-02** — the data folder moves beside the executable, out of `%APPDATA%`
- **2026-10-02** — the widget asks the CLI to renew its token, and says when the CLI cannot
- **2026-09-27** — a polite `taskkill` ends the widget, and a start clears old ghosts
- **2026-09-27** — the settings window gets a card per figure of the pill
- **2026-09-27** — the pill's cells lose their reserved width, and one edge holds
- **2026-09-25** — the settings window becomes pages of cards, chosen from three mockups
- **2026-09-23** — a setting's tip floats over the form instead of opening a line in it
- **2026-09-13** — egui is gone, and the settings window is a list
- **2026-09-12** — the Slint front end lands beside the egui one, not instead of it
- **2026-09-12** — two measurements A20 leaned on were wrong, both in its favour
- **2026-09-12** — the pill's composition moved into `view.rs` so the move is a port
- **2026-09-11** — the GUI decision settles on Slint, under the royalty-free terms

---

## 2026-10-17 — `turn-meter` keeps finished windows, and refuses jumps its journals never paid for

Done: [[A38]] extended. The first look at two days of live journals, by running
the mod's own fit over them, settled the five-hour price ($0.385 against the
configured $0.39) and turned up two faults that reading the code had only
suspected. The weekly price could never be learned: journals live eight days,
and trusting the median takes three weekly windows. One window was accepted at
$0.28 with R² 0.998, though its reading jumped 29 points over $9 and 4 over
nothing: jumps that lie on the line do not lower R². The jump test was chosen
over a bound on distance from the median because on these journals it split
clean windows (0) from the two dirty ones (13 %, 18 %) with nothing between,
while the median's neighbours spread ±7 % for reasons of their own, such as the
model mix. Holding a window to the learned median instead of the config came
with a way out, three agreeing windows as a new price, since otherwise a change
of limits would leave the mod stuck on the old median just as it would have been
on the config.

Later the same day the user said the size of the limits seems to change about
weekly, so waiting three weeks to trust the weekly price was the wrong shape
altogether: the week now stands on its latest window. The five-hour price was
left on its median of ten on purpose, as the question was about the week and a
weekly change moves that median within about two days.

---

## 2026-10-15 — prices per model in `turn-meter`, and `limit-test` to measure them

Done: [[A38]] extended, [[A39]] new. History showed Opus 5 at about $0.60 a
five-hour point and Opus 5.5 at $0.39, so one shared price misprices a turn on
the other model, and a turn whose subagents run on Haiku. The journal gained
`bymodel`, and a model's price comes from windows it had nearly to itself.
Such windows are rare, so a model needs a measured starting price. The first
design measured it inside turn-meter with `$.model.complete`; a probe showed
that those calls never reach the session's ledger, so the measurement moved to
subagents, and into a mod of its own that can be switched on only when wanted.
The same probe showed that a `$.ui.log` line cannot carry a table and that a mod
cannot run its own command, which is why the result waits behind
`/limit-test last`.

Checked: `claude plugin test` (22 in turn-meter, 8 in limit-test, the run
simulated on a mocked clock), `claude plugin validate` for both, `tsc` clean.
Not run live.

## 2026-10-15 — `turn-meter` draws each turn's spend under the reply

Done: [[A38]]. The paused note in `status.md` asked for the context figure under
the reply. A third-party mod, Turn Usage, already drew a line there, but its
per-turn figure is the difference of two whole percentages. A fit of this
machine's history showed that the windows follow list-price dollars closely. So
the turn's share became its dollars over a price per point, and the price is
learned live. Journals went to one file per session after reading that
`$.store` is a single shared JSON file.

## 2026-10-10 — a Claude Code mod feeds the bridge in the desktop app and pins the pill's line

Done: [[A37]]. `mod/` feeds `--bridge` on every `session.measure` and pins
`--line --session <id>` with `$.ui.status`. `--line` is new: `view::cells`
joined by `view::plain_line`. [[G06]] now names the mod as its fix.

Claude Code gained mods: plugins of function hooks that run inside a session,
the desktop app's sessions included. A throwaway probe mod in a desktop session
showed that the engine hands a mod everything the status line used to:
`five_hour` and `seven_day` with reset times, the context window's stated size
and fill, and the model's full id. It also showed that `$.process.run` works
there and that a status entry is drawn in the bottom bar, between the
permission mode and the model. The user liked that spot, and asked for the
pill's line there exactly as they have it set up. So the line comes from
`view::cells` and not from `bridge::render`: the two differ by the daily
allowance and the time of day.

Checked: `cargo test`; `--line` beside a screenshot of the pill, the same
quota figures and order, plain text; with `--session` the context is that
conversation's, and an unknown id gives `ctx --%`. `claude plugin validate mod`,
`claude plugin test mod` (5 tests: the payload against `SAMPLE` in `state.rs`,
the order bridge-then-line, silence in a terminal), `tsc` clean.

Checked live in a desktop session, from a copy of the mod hot-loaded into it:
the session wrote its own entry to `state.json` (quotas with reset times,
context, window size 1 000 000, model id); the line shows in the bottom bar and
its countdowns move while the session sits idle.

Status: Working — loading through `CLAUDE_CODE_PLUGIN_DIRS` in a newly started
desktop session is not yet seen.

---

## 2026-10-03 — the pill can carry the time of day

Done: `show_5h_now` — the local time, last in the five-hour group, off by
default; `view::Field::FiveHourNow`, whose hint gives the weekday and date.

The user keeps the taskbar hidden, so the clock in it is out of sight, and
asked for the time in the five-hour group. It goes last in that group, beside
the countdown it is read against, and is shown with or without the limit's
figures and never greyed, since it is this machine's clock and not a reading.
It is truncated to the minute it is in, where a reset is rounded to the nearest
one: a clock showing the next minute 40 seconds early is wrong. Stepping on the
minute with the other time figures (A30) keeps it from adding repaints (G23).
The card that held the two reset switches is renamed "Five-hour clocks". A new
`field` value in the `--serve` JSON is an addition, so its schema stays 2 (A21).

Checked: a test of its place, paint and truncation, with and without figures.

Status: Working

---

## 2026-10-02 — the data folder moves beside the executable, out of `%APPDATA%`

Done: [[A36]] — `settings` beside the executable, the repository's own
`settings/` for a build in its `target\`; `data_dir` only to a folder that
exists. [[G33]] — why `%APPDATA%` could not be trusted.

After the token check the widget was restarted from a Claude Code session in
the desktop app and came up with demo figures and a note about an old theme
contract. The session runs inside the app's package, so the widget read the
package's private copy of `%APPDATA%` — the redirection A28 had been built to
step around. First proposed: have the widget notice a foreign package and say
so. The user asked instead to stop using `%APPDATA%` at all, which removes the
trap rather than reporting it. Two choices were put to the user: a strict
"beside the executable" or the same with a rule for this repository, since the
widget runs from `target\quick` and the bridge from `target\release`; the
repository rule was taken. No migration from the old folder.

Checked: tests over the folder rule and the one hop; `--sources` from inside
the session, where `%APPDATA%` is still redirected, reads the repository's
`settings/`.

Decisions: [[A36]], [[A28]]
Gotchas: [[G33]]
Status: Working

---

## 2026-10-02 — the widget asks the CLI to renew its token, and says when the CLI cannot

Done: [[A35]] — an expired token is handed to `claude -p /usage`, with a switch
and a `--revive-token` flag; [[G32]] — the CLI's verdict is read from its result,
and a dropped session is named as one.

The renewal was built on 2026-09-27 to spare the user opening a terminal every
morning. On 2026-10-02 the pill showed dashes again, and a restart did not help.
The renewal had been running; it was the CLI that could not renew: its refresh
token had been refused the evening before and it had dropped the session, which
it marks with an expiry of zero. The widget had taken the CLI's exit code of 0
for success and gone on saying "token expired". Nothing here can mend a dropped
session, so the change is in what the widget says: the panel and both probes
now ask for `claude auth login`, and the CLI is no longer started for nothing.
Why the refresh token was refused is not known.

Checked: tests over a result trimmed from the real failing run; after a new
sign-in, `--probe-api` and `--sources` on the rebuilt binary read fresh
figures. The renewal of an expired but live token, unattended, is still
unconfirmed.

Decisions: [[A35]]
Gotchas: [[G32]], [[G03]]
Status: Working, one check open

---

## 2026-09-27 — a polite `taskkill` ends the widget, and a start clears old ghosts

Done: [[G31]] — a close request on the pill or the panel quits, the tray
dropped first; [[G25]] extended — a start sweeps the notification area of
icons whose owners are gone.

The user found that the widget could not be stopped from a script without
`/F`, and that `/F` leaves a ghost icon. Both were proposed as one change: the
close request as a real quit, and optionally a sweep at start-up for the ghosts
earlier kills left. Both were taken. The sweep is the part that talks to
another program's private window, so it is written to find nothing and do
nothing where Explorer is built differently, and every message it sends has a
short timeout.

A backstop came with it: `run` drops the tray after the event loop returns,
whatever ended it, since `App` is held in a cycle by the pill's own callbacks
and would never be dropped otherwise.

Checked on `--demo` by counting the taskbar toolbar's buttons before and after
each step, and by listing the process's windows: the kill, the control run
with no launch, and the settings window closed on its own are in G31 and G25.
The overflow flyout held no icons on this machine, so its half of the sweep
ran over an empty toolbar only.

Gotchas: [[G31]], [[G25]]
Status: Working

---

## 2026-09-27 — the settings window gets a card per figure of the pill

Done: [[A33]] revised — the **Limits** page has a card per figure, and the
"Pace and thresholds" page is gone.

The user found the window awkward: a figure's settings were spread across
categories. The day figure's switch was on one page and how the days are
counted on another; the pace had its form in one card, its switches in another
and its tolerance on a third page. Three ways to regroup were put to the user —
by figure, by limit with the shared settings shown on both limits' pages, and
by limit with every shared setting split in two — and by figure was chosen,
since it moves nothing in the file and duplicates nothing on screen.

Checked by opening the settings window on `--demo` through posted window
messages and capturing each page: every card in place, the dependent rows
indented under the setting they follow.

Decisions: [[A33]]
Status: Working

---

## 2026-09-27 — the pill's cells lose their reserved width, and one edge holds

Done: [[A34]] — a cell is as wide as its text, the gaps between figures are
the theme's alone, and `anchor` picks which edge of the pill stays put.

The user asked for steady spacing between the pill's figures and a choice of
the side the pill grows from, and guessed the cause right: every cell reserved
room for its widest value and centred its text in it, so the space between two
figures was mostly slack that changed with the value. Keeping the old slots as
an option was offered and turned down; the anchor is what makes their loss
cheap, so the two came as one change.

Three things turned up in the theme on the way. The rules between groups and
between two halves of a pace had been zero pixels tall in both themes for some
time, sized from a `pill_height` of `0px`; they are sized from their row now.
The amber stale dot had been sitting in the slack of the first cell, and would
have landed on the digit; the pill makes room for it while it shows. And the
built-in theme's vertical centring, ported into the live theme, folded the pill
to a sliver ([[G30]]) until the cell was given a height of its own.

Checked on `--demo` with the live theme: screenshots of the default, `split`
and `piped` forms, and the window's rectangle read before and after a figure
was switched on and off through the config — the right edge held at `right`,
the left at `left`, switching between them moved nothing, and a restart came
back with the right edge where it was. A drag by hand, done by the user, wrote
the new `width` into `window.json`.

Decisions: [[A34]] · Gotchas: [[G30]]
Status: Working

---

## 2026-09-25 — the settings window becomes pages of cards, chosen from three mockups

Done: [[A33]] — a list of pages down the left, cards of settings on each, the
first sentence of a tip under every name; `schema.rs` says where each setting
sits and what it depends on.

The user found the one-column list unpleasant and pointed at another utility's
two-column settings screen as the bar. Before any Slint was written, three
layouts were mocked up in HTML on a design canvas, on the real settings with
their real wording and values: pages with cards, two columns on one screen, and
tabs. The two-column one, closest to the example, did not fit its own window as
a mockup, and would have needed rebalancing by hand for every new setting; the
user chose the pages, with one idea taken from the tabs — a setting that does
nothing at the moment leaves the form, and only a switched-off source's settings
stay, greyed.

Two things in the existing window turned up on the way. Text was applied on every
keystroke, so the first letter typed into `data_dir` was a folder to move the
files to, followed by a restart; it is applied on Enter now. And the usage
endpoint's status was pushed to the form on every tick; with rows rewritten in
place that would have repainted the whole page every second, so it now touches
its one row, and only when its words change.

The window was checked without a mouse by rendering it from the binary with a
throwaway flag, removed before the commit: every page in both languages, a
switch and two choices through the callbacks a click reaches, and a slider drag
through synthetic pointer events. That is how [[G27]] and [[G28]] were found —
rows overlapping on the first page, and a short page spreading its cards over
the window. A real mouse on the form is still owed; `status.md` says what.

Status: Working

---

## 2026-09-23 — a setting's tip floats over the form instead of opening a line in it

Done: `ui/settings.slint` shows each tip in Slint's own `Tooltip` rather than as a
line that appears under the hovered row.

The line took room in the layout, so pointing at a row pushed every row below it
down, and the form jumped under the cursor while it was being read. A popup takes
no room. Slint 1.17 lowers `Tooltip` to an overlay popup that only observes input,
so the checkbox, the steppers and the menus under it still take their clicks.

The tooltip carries its own content rather than `text:`: the built-in text form
does not wrap, and the longer tips would have been one line wider than the window.
The first version of that content looked right in the source and came out as an
empty box with the text off to its right; why is [[G26]]. It was found by the
user, then reproduced and checked by rendering the form with a simulated hover,
at the top, at the right edge and at the bottom of the list.

Status: Working

## 2026-09-13 — egui is gone, and the settings window is a list

Done: [[A20]] finished, [[A24]] and [[A25]] written, `ui.rs` and `eframe` deleted.

The order mattered more than any single step. The composition of the pill moved into
`view.rs` first, while egui was still running and two `--demo` screenshots could be
compared to the byte; only then was there something the theme could be handed. Under
Slint nothing in the process can measure a string — the theme lays out its own text —
so a composition that still needed a font could not have crossed at all.

The settings window is the part that paid for `schema.rs`. Forty-five settings, four
kinds of row, and `ui/settings.slint` knows what none of them are. Two gaps in the
schema turned up while walking the form out of it: it could not read or write a value
by key, and its choices carried only the spellings the file uses while the wording for
a menu sat on `label(ru)` methods that only the old window called. Both are closed in
the schema rather than worked around in the form.

Four traps are recorded as gotchas because each cost a rebuild to find: a window handle
taken before `show` never starts working ([[G18]]), stripping the frame hands the client
area the margins ([[G19]]), hiding the last window ends the event loop ([[G20]]), and a
`show` restores every window style including the extended ones ([[G21]]). Two more came
from the contract itself: a theme one version behind draws its padding and nothing else
in complete silence ([[G22]]), and a model pushed on a timer is a repaint whether or not
it changed ([[G23]]) — that one cost twelve times the CPU of the front end being
replaced, and looked exactly like the toolkit's fault.

Two measurements in these docs were wrong and are corrected. CPU at rest had been "0.0 s
over ten seconds", which was the method running out of resolution rather than a zero.
The binary had been "doubled, 24 MB against 12", which compared a default-profile Slint
build against this project's `lto`/`strip` one; built the same way the difference is
about three megabytes.

Decisions: [[A20]], [[A24]], [[A25]]
Status: Working

## 2026-09-12 — the Slint front end lands beside the egui one, not instead of it

Done: the pill, its detail panel and the tray are drawn by Slint behind `--slint`,
with `themes/pill.slint` read at runtime. `ui.rs` stays for now.

Both front ends in one binary was the point, not an intermediate state to tidy up
later: it is what makes "reads figure for figure the same as the egui pill" a thing
that can be checked rather than asserted, by running the two side by side on real
data. The binary figure in [[A20]] cannot be taken until `eframe` goes, which is the
price of keeping them both.

What still blocks the removal: the settings window, and hands-on checks of dragging
and the tray menu — neither doable without a mouse.

Decisions: [[A20]], [[A24]], [[A25]] · Gotchas: [[G18]], [[G19]], [[G20]], [[G21]], [[G22]]
Status: Working behind a flag

## 2026-09-12 — two measurements A20 leaned on were wrong, both in its favour

Done: corrected the memory comparison and the binary-size comparison in [[A20]].

The first summed *working set* across a process tree, which counts shared pages once
per process, and made the browser-based option look three times dearer than it is; by
private working set the gap is about 10 %. The second compared a default-profile
Slint build against this project's `lto`/`strip`/`panic=abort` one, and charged the
toolkit seven megabytes that belonged to the profile.

The pattern is the part worth keeping: **both errors flattered the choice that had
already been made**, and neither was caught by re-reading the numbers — only by
re-running them a different way. The CPU figure has the same shape from the other
side: the first release build measured twelve times egui's, and that was the code
repainting on every tick, not the toolkit ([[G23]]). Slint still wins on memory,
which is the column that decided it; it wins by less than the first pass said.

Decisions: [[A20]] · Gotchas: [[G23]]
Status: Working

## 2026-09-12 — the pill's composition moved into `view.rs` so the move is a port

Done: `cells` returns the figures in order with a font *role* and a `Paint` instead
of a font and an RGB value; `bars` returns the hairline under each group. `ui.rs` only
measures and paints them.

This is what turns replacing the drawing layer into a port instead of a rewrite, and
the golden tests over the whole panel and the whole pill are the insurance: a diff in
those four lines is the difference between the two. Two `--demo` screenshots taken
across the change are identical to the byte — with one caveat for the next such
comparison, that the demo's reset time is a real wall-clock, so shots taken minutes
apart differ in that one cell and nothing else.

`--serve` publishes the same list, so a CSS widget gets the composition of the line
and not merely the numbers behind it.

Decisions: [[A19]], [[A21]]
Status: Working

## 2026-09-11 — the GUI decision settles on Slint, under the royalty-free terms

Done: [[A20]] opened and settled after two spikes.

The alternative was not "egui or Slint" but "keep hand-computing every coordinate, or
let a declarative toolkit do it": every appearance setting was a branch in the drawing
code, which is how the config reached 51 fields and `settings_body` 1339 lines, and
nothing a user wants to change could be changed without a rebuild.

The licence is the part that cost a decision rather than a measurement. Going GPLv3
was refused; the royalty-free terms apply instead, and in exchange the settings window
owes a visible acknowledgement of Slint. That is an implementation task, not an open
question.

Decisions: [[A20]]
Status: Working
