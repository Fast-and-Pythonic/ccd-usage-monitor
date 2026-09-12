# Status

Snapshot of 2026-09-12. State only — why anything is the way it is lives in
`architecture.md`, how the work got here in `journal.md`.

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
- **The pill**: figures, hairline tracks, drag with the position persisted,
  panels that flip near the screen edge (A06), a detail panel sized to its text
  (A12), pointing at a figure names it (A16, G16). No frame and no drop shadow —
  the DWM one is stripped through Win32 and re-stripped after the commands that
  bring it back (A10).
- **Pace** beside each limit in whichever of seven forms `pace_view` names (A16);
  green at or under the clock, orange to red above. Colour keys on spending
  whatever the shown figure is (A15).
- **Settings** in a second native window (A11), described once in `schema.rs`
  (A22) — kind, bounds, section, label and tooltip, in both languages.
- **The config is hand-editable, in sections, and re-read while running** (A22,
  A23). Comments and key order survive a click in the menu; the program's own
  writes are told apart from a person's by the exact text last written, not by a
  timestamp.
- **Rate awareness**: a request budget keeps the endpoint from having to refuse,
  a real 429 pauses polling (A13), and the widget reads a rate out of its own
  history log (A17), seeded from the app's.
- **`--serve` publishes the snapshot as JSON** on `127.0.0.1:47317` (A21) — both
  windows, the context figure, the panel already worded, CORS verified in a real
  browser from another port.
- **Three layers** (A19): `runtime.rs` knows, `view.rs` says, `slint_ui.rs`
  draws. The first two hold nothing about drawing, checkable with one grep,
  which is what made the move to Slint (A20, A24, A25) a port and not a rewrite.
- **The settings window is generated from `schema.rs`** (A22): forty-five
  settings, four kinds of row, and the `.slint` file knows what none of them
  are. It carries the `AboutSlint` acknowledgement the licence asks for.
- **Tests**: 167 in all; 31 of them over `view.rs`, including golden tests of the
  whole panel and the whole pill.

## Open

- **The context window's size is inferred, not known.** The token count is exact;
  the divisor is not on disk, so it comes from the status line when one has ever
  run and from a table by model name otherwise. Two cases read five times wrong
  with nothing on disk to reveal it: a 5-series model held to 200K by
  `CLAUDE_CODE_DISABLE_1M_CONTEXT`, and a model newer than this build.
  `ctx_limit = "fixed"` is the escape, manual on purpose.
- **Nobody has clicked the generated form yet.** Dragging, the right click, the
  tray and the hover were confirmed by hand; a checkbox, a stepper and a session
  row have not been. The arithmetic is covered — every numeric setting is walked
  to both its bounds in a test — the wiring from click to saved file is not.
- **The desktop app does not appear to run the status line** (G06). The figures
  are covered without it, but it is still the only source that *states* a window
  size. One check remains: whether a newly started desktop session picks the
  setting up.
- **The OAuth token is short-lived** and deliberately not refreshed: writing to
  another program's credentials file to save a countdown is a bad trade, so the
  endpoint goes quiet on days when Claude Code is not run in a terminal.
- **Autostart** is not wired up: a deliberate omission pending a decision.
- **`cargo clippy -- -D warnings` does not pass**, and has not for a while: eleven
  findings, none new, none in the modules written since.
  Use it by counting findings per file before and after a change.

## Fragile points

- **The usage endpoint is undocumented** and can change shape or disappear without
  notice. `api::parse_usage` accepts two spellings of each field for that reason,
  and every failure degrades to "source unavailable" rather than an error.
- **The state-file lock is best-effort by design** (A02). With many simultaneous
  sessions an interleaved write is possible; the worst case is one tick of stale
  figures, corrected on the next.
- **Window position is trusted as saved**: a negative coordinate is legitimate on
  a multi-monitor desktop, so the tray's "Reset position" is the recovery rather
  than a startup heuristic that would fight a real layout.
