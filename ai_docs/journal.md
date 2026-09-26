# Journal

Append-only, newest on top. Why the work went the way it did — the substance lives
in the registers and is not restated here. `git log` has what changed line by line.

## Index

- **2026-09-27** — a polite `taskkill` ends the widget, and a start clears old ghosts
- **2026-09-27** — the pill's cells lose their reserved width, and one edge holds
- **2026-09-25** — the settings window becomes pages of cards, chosen from three mockups
- **2026-09-23** — a setting's tip floats over the form instead of opening a line in it
- **2026-09-13** — egui is gone, and the settings window is a list
- **2026-09-12** — the Slint front end lands beside the egui one, not instead of it
- **2026-09-12** — two measurements A20 leaned on were wrong, both in its favour
- **2026-09-12** — the pill's composition moved into `view.rs` so the move is a port
- **2026-09-11** — the GUI decision settles on Slint, under the royalty-free terms

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
