# Status

Snapshot of 2026-10-10. State only — why anything is the way it is lives in
`architecture.md`, how the work got here in `journal.md`.

> **Paused 2026-10-15.** After the desktop app update to Claude Code 2.1.295 a mod
> can no longer draw beside the model and effort picker: `$.ui.status` lands on a
> row under it, which lifts the bottom area and takes height from the transcript,
> and no other render site sits there (checked site by site with a probe mod).
> The mod is off (`showLine: false`, not in `CLAUDE_CODE_PLUGIN_DIRS`). When work
> resumes, the plan is to keep only the context figure, as `ctx 21% · 212k`, and
> show it at the end of a turn (under the reply, as `ui.render` on
> `AssistantMessage`) or elsewhere, not in the pill.

## Works

- **Four sources, merged field by field** (A03), each an independent switch with
   its own staleness threshold (A14): the desktop app's history (A08), the bridge,
   the usage endpoint, and the session transcripts (A18). Figures grey out after
   15 minutes and vanish after an hour (A07). `--sources` and `--sessions` print
   what each claims, for diagnosis.
- **The context figure** is read from the transcript Claude Code writes for itself
   (A18), so it works in the desktop app, where the status line never runs (G06).
   Which session it describes is `session_pick`; `ctx` has its own spent/left
   direction and a bar of its own.
- **The pill**: figures as wide as their text with fixed gaps, the `anchor` edge
   held as the width changes (A34), hairline tracks, drag with the position
   persisted, panels that flip near the screen edge (A06), a detail panel sized
   to its text (A12), pointing at a figure names it and says what it is now (A31,
   G16). No frame and no drop shadow — stripped through Win32 and re-stripped
   after the commands that bring it back (A10).
- **Pace** beside each limit in whichever of eight forms `pace_view` names (A16);
   green at or under the clock, orange to red above, keyed on spending (A15).
   Weekly days by calendar or clock, by clock as `3д16ч` or `3'16` (A29), and a weekly pace in time as `▲3д2ч` or `▲3'2` (A16), the separator and the zeros a setting; time-derived figures step on the minute (A30).
   Every cell but a window's percentage can be switched off, on the pill only.
- **The config is hand-editable, in sections, and re-read while running** (A22,
   A23); `data_dir` (A28) can move it, the theme and every other file this
   program writes to a folder of choice, for when the usual place is not
   directly editable. Comments and key order survive a click in the menu.
- **Rate awareness**: a request budget keeps the endpoint from having to refuse,
   a real 429 pauses polling (A13), and the widget reads a rate out of its own
   history log (A17), seeded from the app's.
- **`--serve` publishes the snapshot as JSON** on `127.0.0.1:47317` (A21) — both
   windows, the context figure, the panel already worded, CORS verified in a real
   browser from another port.
- **Three layers** (A19): `runtime.rs` knows, `view.rs` says, `slint_ui.rs`
   draws. The first two hold nothing about drawing, checkable with one grep,
   which is what made the move to Slint (A20, A24, A25) a port and not a rewrite.
- **Settings in a second native window** (A11), generated from `schema.rs` (A22,
   A33): five pages, a card per figure, fifty-one settings; the `.slint` file knows
   none of them. Files beside the executable, never in `%APPDATA%` (A36).
- **The tray icon goes with the process** (G31); a start sweeps older ghosts (G25).
- **A Claude Code mod covers the desktop app** (A37): it feeds `--bridge` and pins
   the pill's line (`--line`) under the prompt.
- **An expired token is renewed by the CLI** (A35), run out of sight with
   `claude -p /usage`; `--revive-token` does it by hand.
- **Tests**: 212 in all, 39 over `view.rs` with golden tests of panel and pill; 5 in `mod/`.
- **Checks before a commit** (A32): `cargo fmt`, `cargo clippy -D warnings` at zero
   findings, and the docs linter, through `tools/hooks/pre-commit`.

## Open

- **The context window's size is inferred where the mod (A37) does not run.** The
   token count is exact; the divisor is not on disk, so it comes from the status
   line when one has ever run and from a table by model name otherwise. Two cases read five times wrong
   with nothing on disk to reveal it: a 5-series model held to 200K by
   `CLAUDE_CODE_DISABLE_1M_CONTEXT`, and a model newer than this build.
   `ctx_limit = "fixed"` is the escape, manual on purpose.
- **Nobody has clicked the new settings window by hand yet.** Every page was
   rendered in both languages and a slider drag driven by synthetic events; a
   real mouse on the steppers, the menu, the text field and the conversation
   list, and the file each one writes, is still unchecked.
- **The CLI can drop its session** (G32), which no renewal mends; the cause on
   2026-10-01 is unknown. A whole unattended day on A35 is not yet confirmed.
- **Autostart** is not wired up: a deliberate omission pending a decision.

## Fragile points

- **The mod API is early access** (A37): after a Claude Code update, first run
   `claude plugin validate mod` and `claude plugin test mod`.
- **The usage endpoint is undocumented** and can change shape or disappear without
   notice. `api::parse_usage` accepts two spellings of each field for that reason,
   and every failure degrades to "source unavailable" rather than an error.
- **The state-file lock is best-effort by design** (A02). With many simultaneous
   sessions an interleaved write is possible; the worst case is one tick of stale
   figures, corrected on the next.
- **Window position is trusted as saved**: a negative coordinate is legitimate on
   a multi-monitor desktop, so the tray's "Reset position" is the recovery rather
   than a startup heuristic that would fight a real layout.
