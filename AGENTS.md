# ccd-usage-monitor

A floating always-on-top pill for Windows showing Claude Code's quota windows and
how full the current session's context is.

AI documentation lives in `project-workbook/`. **Start with `project-workbook/start.md`.**

## Critical rules

- **`runtime.rs`, `view.rs` and `tray.rs` contain no egui, and must not.** That split
   is what makes the move to Slint a port rather than a rewrite — see `A19`.
- **Numbers belong in one file.** A figure repeated across `overview.md`, `status.md`
   and an `A##` will disagree with itself within a month (`project-workbook/_meta.md`).
- **Nothing here may reference the private environment store on this machine.** State
   a measurement method in full instead of citing a note nobody else can open
   (`project-workbook/_meta.md`).
