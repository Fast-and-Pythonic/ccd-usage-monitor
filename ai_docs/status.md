# Status

Snapshot of 2026-09-11.

## Works
- Four sources — the desktop app's own history (A08), the bridge, the usage
  endpoint, and the session transcripts (A18) — merged field by field (A03),
  each an independent switch.
- `ccd-usage-monitor --sources` prints all four side by side for diagnosis, with the whole
  context division rather than only its answer; `ccd-usage-monitor --sessions` lists every
  transcript on disk and what each would report.
- The context window is read from the transcript Claude Code writes for itself
  (A18), so the figure works in the desktop app, where the status line never
  runs (G06). Which session it is about is `session_pick` — running, freshest,
  or pinned from a list in the settings window. `ctx_view` shows the share, the
  token count, or both; the `ctx` group has a bar of its own like the two
  quotas.
- Endpoint polling is slow by default but wakes on demand (A09), so switching
  source or hovering the pill refreshes in seconds rather than minutes.
- Expiry: figures grey out after 15 minutes and disappear after an hour (A07).
- Panels flip above the pill near the bottom of the screen (A06).
- Bridge mode end to end: payload on stdin → state file → widget, plus an ANSI
  line for the terminal status bar.
- The pill: figures, hairline tracks that stay inside it whatever their
  thickness, drag with position persisted to `window.json` (A22). The weekly group
  shows days left rather than the name of the reset day.
- Settings in a window of their own with a system title bar (A11), opened by
  right-clicking the pill or from the tray; its position and size are
  remembered.
- The detail panel is sized to its own text (A12) and appears on hover, with
  the settings window, or never — `detail_mode`, default "with settings". An
  age is printed once: per source on the `src` line, or on the per-source lines
  when those are switched on.
- A request budget keeps the endpoint from having to refuse: past four requests
  a minute the poller waits a few seconds and then goes on its own. A real 429
  pauses polling, starting at a minute rather than five (A13).
- Sources are independent switches with their own staleness thresholds (A14);
  the detail panel can show a line per source instead of only the merged one.
- Pace has a figure of its own beside each limit on the pill, in whichever of
  seven forms `pace_view` names — ratio, points, both in either order, both with
  a drawn rule, both behind a written pipe, or none (A16). It is green only at
  or under the clock and orange to red above it, and the word in the panel
  switches at the same point. Bars can be coloured by pace instead of by fill.
- Pointing at a figure names it in a small box under the pill, or as the detail
  panel's first line when the panel is already open (A16, G16).
- Horizontal gaps are one multiplier (`gaps_x`) and the corners are a radius
  (`corner_radius`), both in the sizes menu; `0` squares the pill off and half
  its height gives back the lozenge.
- The widget keeps a log of past readings, seeded from the Claude app's own
  history (A17), and reads a rate out of it: `20%/h · runs out 07:19, 0:42
  before the reset`.
- The percentages and the bars show either what is spent or what is left
  (`pct_mode`); colour and the pace verdict stay keyed on spending, and the
  detail panel prints both readings.
- `ctx` has its own spent/left direction, not `pct_mode`'s: a window fills
  rather than draining, so it defaults to "spent" regardless of what the
  quotas are set to, and `ctx_pct_dir`/`ctx_tokens_dir` can be split apart with
  `ctx_dir_linked` when the percentage and the count should disagree (A18).
- Which days the weekly counter spreads the quota over is a setting: today and
  the reset day are each switched on or off, and the daily allowance divides by
  exactly the number shown beside it.
- Vertical paddings inside the pill are settable, and the pill grows to keep the
  bars inside whatever they are set to.
- Sizes are settable from the menu — scale, both fonts, pill height, bar
  thickness — with a reset row; opacity moves in 1% and 5% steps (A04).
- The window has no frame and no drop shadow: the DWM one is stripped through
  Win32 and re-stripped after the commands that bring it back (A10).
- Tray icon: hide/show, click-through, quit.
- `cargo test`: 163 tests over payload parsing, merge/prune rules and every
  derived figure.
- **The config is hand-editable and says so** (A22). `config.toml` is in
  sections — `[sources]`, `[behaviour]`, `[display]`, `[thresholds]`,
  `[style]` — written through `toml_edit`, so comments and key order survive a
  click in the settings menu. The three window rectangles moved to
  `%LOCALAPPDATA%\ccd-usage-monitor\window.json`, which is what made the rest
  possible: they were rewritten on every drag and took any comment with them.
  An old flat config is laid out into sections once, comments carried across
  with their keys, and nothing else about it changes.
- **An edit to `config.toml` is picked up while the program runs** (A23),
  by the widget and by `--serve` alike, on the same quarter-second tick that
  watches the state file. The program's own writing is told apart from a
  person's by the exact text last written, not by a timestamp, so a click in
  the settings window cannot be read back as somebody's edit. A re-read config
  drops the sources' remembered modification times but never wakes the poller
  (G14), and a desktop history that comes back with identical figures no
  longer costs a request either.
- **`schema.rs` describes every setting once** — kind, bounds, section, label
  and tooltip in both languages. It already lays out the file; the settings
  form is generated from it when the GUI moves.
- **The Slint spikes answered every question the move opens with** (in the
  session scratchpad, `pill-spike`). The crates link together — `slint`,
  `slint-interpreter`, `slint-build`, `raw-window-handle 0.6`, `windows-sys` —
  and `win.rs` works on the HWND Slint hands out, unchanged. The theme reports
  its own natural size through an `out property <length>`, which is what makes
  a content-sized window possible at all now that nothing in this process can
  measure a string. Click-through goes on and off a live window; the tray icon
  builds and is serviced by a `slint::Timer`; the panel works as a second
  window that does not take the focus. Four things had to be got right rather
  than assumed, and each is a gotcha: G18, G19, G20, G21.
- **`--serve` publishes the snapshot as JSON on `127.0.0.1:47317`** (A21) —
  both windows, the context figure, the detail panel already worded, and the
  display conventions in force. `--port N` moves it, `--no-api` leaves the
  endpoint to the widget when both are running. Ninety lines of `std::net`, no
  new dependency. Verified end to end against live data, and the CORS header
  verified the only way that counts: a page served from another port fetching
  it in a real browser, since that is what a Zebar widget is.
- The code is split into three layers (A19): `runtime.rs` (what the widget
  knows), `view.rs` (what it says) and `ui.rs` (how it is drawn), with
  `tray.rs` beside them. `runtime`, `view` and `tray` contain no egui at all,
  which is checkable with one grep — see A19.
- **The pill's composition lives in `view.rs`** (A19): `cells` returns the
  figures in order with a font *role* and a `Paint` instead of a font and an
  RGB value, `bars` the hairline under each group. `ui.rs` only measures and
  paints them, and `--serve` publishes the same list, so a CSS widget gets the
  composition of the line and not just the numbers behind it. Two `--demo`
  screenshots taken across the change are identical to the byte.
  *Caveat for the next such comparison:* the demo's reset time is a real
  wall-clock, so two shots taken minutes apart differ in that one cell and in
  nothing else.
- **`view.rs` is covered: 31 tests, including golden ones that assert the whole
  detail panel and the whole pill.** That is the insurance for the move to
  Slint (A20): the drawing layer is replaced, and a diff in those four lines is
  the difference between a port and a rewrite. Covered besides: the tone
  thresholds with A15's rule that colour reads spending and never the shown
  figure, A16's green-to-the-line boundary, `api_notice` in every state, and
  the observed rate in both units.
  Two things make those tests independent of the machine, and both are load
  bearing: fixed moments are built in **local** time, the way `pace.rs` does
  it, so an expected `15:37` holds in any timezone; and the fixtures use
  `Runtime::blank()`, which reads nothing, because `Runtime::new()` loads the
  history log from disk and a test built on it would pass or fail by whatever
  `samples.json` happens to hold.

## Open
- **The GUI moves to Slint (A20), under Slint's royalty-free licence.**
  Decided 2026-09-11 on the measurements below. The settings window owes a
  visible acknowledgement of Slint; that is an implementation task, not an
  open question. Two
  spikes, both against ~121 MB for the widget as it stands:

  | | processes | working set | **private** |
  |---|---|---|---|
  | **Slint 1.17 + interpreter** | 1 | 45 MB | **15 MB** |
  | eframe/wgpu, the widget today | 1 | 118 MB | **63 MB** |
  | Tauri 2.11, empty page | 7 | 335 MB | **70 MB** |

  **Read the private column.** Working set counts shared pages — the DLL images
  seven Chromium processes all map at once — so summing it across a tree counts
  the same bytes seven times and flatters a single-process program. By private
  working set, Tauri is about 10 % dearer than what runs today, not three times
  dearer as the first measurement suggested; the argument against the web was
  weaker than it looked (`ai_docs_env` EG45).

  What survives the correction is Slint: 15 MB private, a quarter of the
  current toolkit and a fifth of Tauri.

  **Slint against the toolkit in use today, release builds, same machine:**

  | | egui/eframe (today) | Slint + interpreter |
  |---|---|---|
  | private working set | 61 MB | **13 MB** one window, **22 MB** with a second window and a tray |
  | working set | 110 MB | **47–51 MB** |
  | CPU at rest, 70 s | 0.156 s | 0.156–0.172 s |
  | startup (7 warm runs) | **64 ms** | 143 ms |
  | binary, same release profile | **12.1 MB** | 14.9 MB |
  | transparency | yes | yes |
  | DPI | per-monitor | per-monitor |
  | 1-device-pixel hairline | by hand, via `Metrics` | snapped, plus a `phx` unit |
  | runtime-editable theme | no | yes |
  | licence | Apache-2.0 / MIT | GPLv3, or royalty-free **with attribution** |
  | downloads, 90 days | 5.2 M | 467 k (interpreter: **12 k**) |

  Slint costs a third to a fifth of the memory, starts twice as slowly, and
  adds about 3 MB to the binary. For a widget launched once a day, 143 ms is
  not a cost worth naming; 40-odd MB of private memory, held all day, is.

  **Read the memory row with its configuration.** 13 MB is one window and
  nothing else; 22 MB is the spike once it grew a second window and a tray
  icon, which is closer to what the widget actually is. The Slint figure will
  keep rising as the rest of the widget's work is added to it, so it is a floor
  rather than a forecast — the egui column is a finished program.

  **The binary row was wrong until 2026-09-12** and said 24 MB against 12 MB.
  That compared a default-profile Slint build with this project's
  `lto`/`strip`/`panic=abort` one; built the same way, the same spike is
  14.9 MB. The profile was worth 7 MB of the 12 that had been charged to the
  toolkit (`ai_docs_env` EG48).

  **CPU at rest is the same, and saying anything sharper than that would be
  making it up.** Two runs of the same Slint binary came out 0.156 s and
  0.172 s per 70 seconds — a spread as wide as the gap between the toolkits.
  The old line in `overview.md`, "0.0 s over ten seconds", was not a
  measurement at all but the method running out of resolution; the figures
  above are `TotalProcessorTime` deltas over 70 s with the window visible and
  the pointer elsewhere (`ai_docs_env` EG47, `tools/measure-cpu.ps1`).

  Two non-technical prices come with it. The interpreter is a narrow path —
  12 k downloads against Slint's own 467 k, so roughly one user in forty takes
  it, and the stable API does not cover swapping a live theme. And the licence
  is no longer unconditional: either the project goes GPLv3, or the
  royalty-free terms apply and the application must disclose that it uses
  Slint (an `AboutSlint` widget or badge — realistically in the settings
  window, since the pill is 34 pixels tall).

  **What the drawing layer costs today**, for scale: `draw` 411 lines,
  `build_cells` 141, `pace_cells` 47, `ctx_cells` 39, `draw_details` 28,
  `draw_bar` 27, `draw_track` 22, `Metrics` 80 — 795 lines that exist because
  immediate mode computes every coordinate by hand. A `.slint` equivalent is
  estimated at 150–250 lines; the pill used for these measurements took 25
  lines for six fields, a background, a corner radius and a bar.

  **What the interpreter costs, measured against the same theme compiled in:**

  | | resident | startup | binary |
  |---|---|---|---|
  | theme compiled in (`slint-build`) | 54.9 MB | 154 ms | 24 MB |
  | theme loaded at runtime (interpreter) | 55.4 MB | 140 ms | 49 MB |

  Memory and startup are a wash — the startup difference is inside the noise,
  and the interpreter was marginally *faster* across five runs. The whole cost
  is on disk: the interpreter carries the Slint compiler, doubling the binary.
  Both figures are debug builds.

  This matters because compiling a theme is not a separate step that produces a
  loadable artifact: `.slint` is compiled through `build.rs` into the
  application, so "compile my theme" means "rebuild the app", which needs a
  Rust toolchain. `slint-viewer` is a previewer and uses the interpreter too.

  Slint is the outlier: half the memory of what runs today and a fifth of the
  browser-based options, in a debug build, with the UI loaded from a `.slint`
  file at runtime rather than compiled in. Transparency works.

  **DPI is fine, and the earlier doubt was a measurement error.** Slint declares
  `PER-MONITOR` awareness and the window really is 450×188 device pixels at
  125 % — the 360×150 reading came from a probe that had not declared awareness
  itself and was handed virtualised coordinates (see `ai_docs_env` EG44).
  Hairlines are crisp: scanning a column of device pixels, a 1px line lands on
  exactly one row of the exact colour with no partial-brightness neighbour, and
  `phx` units are there when a physical pixel must be named outright.

  **Swapping the theme of a running instance** is not covered by the stable API:
  the interpreter loads a theme at startup, and replacing it means recreating
  the window. Accepted as a limitation — a "restart" button in the settings is
  the intended answer.

  **For authoring themes there is `slint-viewer --auto-reload`**, which watches
  the file and redraws on save. It shows the UI without the application's live
  data; `--load-data <file>` seeds public properties from JSON once, so a
  snapshot dumped by the widget would make the preview realistic.

  Both spike figures are a floor — an empty page in a debug build, where the
  Rust process is 20–27 MB and every other megabyte belongs to WebView2 and
  does not shrink with `--release`.

  **Transparency works under Tauri and did not under bare wry/tao.** Eight
  configurations of the latter — two version pairs, with and without the frame
  surgery, with and without an explicit `DefaultBackgroundColor`, and with
  DWM's blur-behind alpha trick — all painted an opaque backdrop; the same page
  under Tauri let the desktop through. So the transparency risk is closed, and
  what was missing was configuration rather than capability. The memory cost is
  the real open question, and it is what makes a self-rendering toolkit (Slint,
  Qt/QML) worth weighing against the web. The layer split (A19) is worth having
  under any of them.
- **The context window's size is inferred, not known.** The count of tokens is
  exact; what it is divided by is not on disk (the catalogue caches in
  `~/.claude.json` are empty on this machine), so it comes from the status line
  when one has ever run, and from a table by model name otherwise. Two cases
  read five times wrong with nothing on disk to reveal it: a 5-series model held
  to 200K by `CLAUDE_CODE_DISABLE_1M_CONTEXT`, and a model shipped after this
  build. `ctx_limit = "fixed"` is the escape, and it is manual on purpose —
  reading the environment variable out of `settings.json` would be a second
  guess dressed as a fact.
- **The desktop app does not appear to run the status line** (G06). The figures
  are covered without it — the app's own history for the quotas (A08), the
  transcripts for the context (A18) — but the status line is still the only
  source that *states* a window size, so running one terminal session is worth
  it. One check remains: whether a *newly started* desktop session picks the
  setting up, since settings are read at session start.
- **The OAuth access token is short-lived** — the one observed expired about
  eight hours after it was issued. It is refreshed whenever Claude Code runs;
  ccd-usage-monitor deliberately does not refresh it (writing to another program's
  credentials file to save a countdown is a bad trade). Expect the endpoint to
  go quiet on days when Claude Code is not started in a terminal.
- **Autostart** is not wired up. Nothing is written to the registry or the
  Startup folder yet; that is a deliberate omission pending a decision.
- **`cargo clippy -- -D warnings` does not pass, and has not for a while.**
  Twelve findings, none of them new: nine collapsible `if let` / `if` pairs
  (`session.rs`, `state.rs`, `runtime.rs`, `history.rs`), an `or_insert_with`
  that could be `or_default`, a manual char comparison, a `sort_by` that is a
  `sort_by_key`, and `stepper_row` at eight arguments. (It was thirteen until
  `draw_track` was folded into `view::bars`.) `--all-targets` adds ten
  more in test code, all of them the same `field_reassign_with_default`.
  Worth knowing before quoting the command as a gate: it is red for reasons
  that predate whatever is being checked, so the way to use it on a change is
  to count the findings per file before and after rather than to expect a clean
  run. `view.rs`, `serve.rs`, `schema.rs` and `geometry.rs` are clean.

## Fragile points
- The usage endpoint is undocumented and can disappear or change shape without
  notice. `api::parse_usage` already accepts two spellings of each field for
  that reason, and every failure degrades to "source unavailable" in the detail
  panel rather than an error.
- The state-file lock is best-effort by design (A02). With many simultaneous
  sessions, an interleaved write is possible; the worst case is one tick of
  stale figures, corrected on the next tick.
- Window position is stored in logical pixels and is trusted as saved, since a
  negative coordinate is legitimate on a multi-monitor desktop. If the widget
  ends up off-screen — a monitor unplugged, a resolution change — the tray menu
  has "Reset position", which is the intended recovery rather than a startup
  heuristic that would fight legitimate layouts.
