# Journal

Append-only, newest on top. Why the work went the way it did — the substance lives
in the registers and is not restated here. `git log` has what changed line by line.

## Index

- **2026-09-12** — the Slint front end lands beside the egui one, not instead of it
- **2026-09-12** — two measurements A20 leaned on were wrong, both in its favour
- **2026-09-12** — the pill's composition moved into `view.rs` so the move is a port
- **2026-09-11** — the GUI decision settles on Slint, under the royalty-free terms

---

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
