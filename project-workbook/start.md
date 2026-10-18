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
- `journal.md` — why the work went the way it did, newest on top. Read the top
   entries when picking up something that was left mid-move
- `_meta.md` — which standard these docs follow, the tier, and the three things
   this project most often gets wrong

## What to read when
- **First time here** → overview → architecture
- **Quota numbers are wrong, missing, or stale** → gotchas G01–G03, then
   `src/state.rs`; `ccd-usage-monitor --sources` says which source claims what
- **The context figure is wrong or silent** → G17 and A18, then `src/session.rs`;
   `ccd-usage-monitor --sessions` lists every transcript and what each reports
- **Changing what a figure or a panel line *says*** → `src/view.rs` (A19). It
   holds nothing about drawing and is where the wording, the pace verdict, the
   colour *meaning* and the pill's composition (`cells`, `bars`) live
- **Changing how a source is read or merged** → `src/runtime.rs` (A19)
- **Adding or changing a setting** → `src/schema.rs` (A22). One entry there is
   its section in the file, its bounds, its wording, its card in the settings
   window and what it depends on (A33); four tests refuse a field that is
   described in only one of the two places, and more refuse a card or a
   dependency that points nowhere
- **Changing how the settings window looks** → `ui/settings.slint` (A33), and
   G27–G29 before touching its layouts or its scrolling
- **The figures in the desktop app's own window, or the bridge fed from it** →
   `mod/` and A37. Run `claude plugin validate mod` and `claude plugin test mod`
   after editing it. The line it pins is `--line`, worded by `view::plain_line`
- **The line under each reply in the desktop app** → `turn-meter/` and A38.
   Run `claude plugin validate turn-meter` and `claude plugin test turn-meter`
- **A model's price of a point, measured** → `limit-test/` and A39; the files
   it shares with turn-meter are contracts both ways (A38, A39). Run `claude
   plugin validate limit-test` and `claude plugin test limit-test`
- **A widget for some other bar (Zebar, Rainmeter, eww)** → `ccd-usage-monitor
   --serve` and A21; the JSON is the contract, `src/serve.rs` owns its shape
- **Changing how the pill looks** → `settings/theme.slint` first: any build
   inside this repository keeps its files in `settings/` (A36), so that file is
   the theme that actually loads. It is hand-tuned; the default is a copy of
   it (since 2026-09-27, last copied 2026-10-18), and the two part only when
   one is edited without the other. `themes/pill.slint` is only the
   built-in default, written out where no theme exists yet (A24) — change it
   too when the default should follow, but an edit made only there never shows
   on this machine. No rebuild needed for the live file; run
   `ccd-usage-monitor --demo` to see it on figures that hold still
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
- **The widget will not quit, or leaves a tray icon behind** → G31 and G25
- **Settings, theme or demo figures look like someone else's** → A36 and G33:
   where the data folder is, and why `%APPDATA%` is not used
- **Upgrading Slint** → G18–G23 and G26, and re-read A24 before changing a struct the theme receives

## Five facts first
1. There are four sources, each an independent switch (A03, A14): the Claude
   Code status line (official, terminal only), an undocumented usage endpoint,
   the desktop app's own history file, and the conversation transcripts. The
   first three carry the quota windows; the transcripts carry the context figure
   (A18), and the status line carries both. None is guaranteed to be present,
   so the UI must render honestly when a source is silent.
2. The same executable is both the widget and the status-line command
   (`--bridge`); they communicate through one JSON file, not through IPC.
3. **The GUI is Slint (A20)**, and the whole appearance is a `.slint` file read
   at runtime, so it can be changed without a Rust toolchain (A24). Used under
   Slint's royalty-free terms, which is why the settings window and the tray
   both say "Built with Slint"; the project does not go GPLv3.
4. The code is in three layers (A19) — `runtime.rs` knows, `view.rs` says,
   `slint_ui.rs` draws — and the first two hold nothing about drawing on
   purpose, which is what made the move from egui a replacement of one layer
   rather than a rewrite. One grep checks it; A19 has the line.
5. The endpoint's token is the CLI's, not the desktop app's, so it goes stale on
   a machine where Claude Code is only ever used through the app (G03). The
   widget asks the CLI to renew it (A35); a session the CLI has dropped needs
   `claude auth login` once (G32). Running the CLI is worth doing anyway, because the
   status line is also the only thing that ever *states* the context window's
   size (A18).
