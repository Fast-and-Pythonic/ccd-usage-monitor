# ccd-usage-monitor — start

## What this is
A floating always-on-top pill for Windows that keeps Claude Code's 5-hour and
weekly quota figures, and how full the current session's context window is, in
view without opening any panel. It is aimed first at the Claude Code **desktop**
app, which shows none of this itself and does not run the status line (G06).

## Documentation map
- `overview.md` — stack, layout of the code, data flow, cost at rest
- `architecture.md` — why it is built this way (A##)
- `gotchas.md` — traps worth knowing before touching the data layer (G##)
- `status.md` — what works today, what is open, what is deliberately not done
  and where the fragile points are. The one file that tracks current state

## What to read when
- **First time here** → overview → architecture
- **Quota numbers are wrong, missing, or stale** → gotchas G01–G03, then
  `src/state.rs`; `ccd-usage-monitor --sources` says which source claims what
- **The context figure is wrong or silent** → G17 and A18, then `src/session.rs`;
  `ccd-usage-monitor --sessions` lists every transcript and what each reports
- **Changing what a figure or a panel line *says*** → `src/view.rs` (A19). It
  holds no egui and is where the wording, the pace verdict, the colour
  *meaning* and the pill's composition (`cells`, `bars`) live
- **Changing how a source is read or merged** → `src/runtime.rs` (A19)
- **Adding or changing a setting** → `src/schema.rs` (A22). One entry there is
  its section in the file, its bounds and its wording; four tests refuse a
  field that is described in only one of the two places
- **A widget for some other bar (Zebar, Rainmeter, eww)** → `ccd-usage-monitor
  --serve` and A21; the JSON is the contract, `src/serve.rs` owns its shape
- **Changing how the pill looks** → on the Slint front end, `themes/pill.slint`
  or the copy in `%APPDATA%` (A24); on the egui one, `src/ui.rs`, where sizes
  come from `Metrics` built from the config (A04). Run `ccd-usage-monitor
  --slint --demo` and `ccd-usage-monitor --demo` side by side
- **The pill is blank, or one figure is missing** → G22 first: a theme one
  contract version behind draws its padding and nothing else
- **The widget is using more CPU than it should** → G23: anything pushed to a
  Slint property on a timer is a repaint, changed or not
- **The window has a frame, a shadow, or a dark rectangle** → A10, `src/win.rs`
- **Doing that frame surgery on a Slint window** → G18 (ask for the handle
  after `show`, never before), G19 (state the size *after* stripping the frame,
  or the drawing grows into the margins), G21 (a `show` puts the frame back and
  wipes the extended styles with it)
- **The widget quits when it is hidden, or a window steals the focus** → G20
  and G21
- **Upgrading eframe/egui** → gotcha G04

## Five facts first
1. There are four sources, each an independent switch (A03, A14): the Claude
   Code status line (official, terminal only), an undocumented usage endpoint,
   the desktop app's own history file, and the conversation transcripts. The
   first three carry the quota windows; the transcripts carry the context figure
   (A18), and the status line carries both. None is guaranteed to be present,
   so the UI must render honestly when a source is silent.
2. The same executable is both the widget and the status-line command
   (`--bridge`); they communicate through one JSON file, not through IPC.
3. **The GUI is moving to Slint (A20)** — `ui.rs` is on its way out, the theme
   becomes a `.slint` file loaded at runtime. `runtime.rs` and `view.rs` are
   unaffected by design. Licensed under Slint's royalty-free terms, which
   means the settings window must carry a visible acknowledgement of Slint;
   the project does not go GPLv3.
4. The code is in three layers (A19) — `runtime.rs` knows, `view.rs` says,
   `ui.rs` draws — and the first two contain no egui on purpose, which is what
   makes the move in fact 3 a replacement of one layer rather than a rewrite.
   One grep checks it; A19 has the line.
5. The endpoint's token is the CLI's, not the desktop app's, so it goes stale on
   a machine where Claude Code is only ever used through the app (G03). It is
   revived by running the CLI once — which is worth doing anyway, because the
   status line is also the only thing that ever *states* the context window's
   size (A18).
