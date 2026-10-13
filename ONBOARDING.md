# Joining ccd-usage-monitor

ccd-usage-monitor is a floating always-on-top pill for Windows that shows Claude Code's
quota windows and how full the current session's context is, written in Rust with a Slint
front end. Start reading at [`project-workbook/start.md`](project-workbook/start.md);
[README.md](README.md) is for users of the widget.

## What works with nothing installed

- **[`project-workbook/`](project-workbook/start.md)** — the project's working notes: the
  map, the current state, the gotchas (`G##`) and decisions (`A##`) that cost time to
  find. Plain Markdown; start at `start.md`.
- **[`AGENTS.md`](AGENTS.md)** — the pointer any coding agent reads first, with the few
  rules that hold for every task: no egui in `runtime.rs`, `view.rs` and `tray.rs`, each
  number written in one file only, and no reference to a private environment store.

## Command-line tools

**Required:**

- **A Rust toolchain** — the docs give Rust 1.97, edition 2024, and the build is
  `cargo build --release`. Slint is pinned to 1.17.1 in `Cargo.toml`. The profiles and
  what each is for are in the Install section of [README.md](README.md). The widget is a
  Windows program.
- **Python 3.8+** (3.11+ to read `tools/lint_workbook.toml`) — runs the workbook linter.
- **The commit hook**, once per clone:

  ```bash
  git config core.hooksPath tools/hooks
  ```

  Before each commit it runs `cargo fmt --check` and `cargo clippy -- -D warnings` when
  Rust is staged, and lints the workbook when the workbook is staged. When code is staged
  with no workbook change it prints the update triggers — a reminder, it never blocks the
  commit.
- **The Project Workbook tools** — the standard checked out beside this repository
  (`0_project-workbook-method`), or anywhere if `WORKBOOK_STANDARD` points at it. Without
  it the hook lets the commit through and says the docs were not checked.

**Recommended:**

- [`rg`](https://github.com/BurntSushi/ripgrep) — fast search; agents use it when present.
- [`ast-grep`](https://ast-grep.github.io/) — structural search and rewrite over the Rust
  code; the layer split in `AGENTS.md` is checked with one grep.

## Claude Code

The repository ships nothing that loads by itself: no project hooks, skills or MCP
servers. Each item below is optional.

- **Mods you enable yourself.** A repository cannot turn a mod on for a visitor. From a
  terminal session:

  ```
  /plugin install workbook-loader --marketplace Fast-and-Pythonic/project-workbook-method
  /plugin install workbook-triggers --marketplace Fast-and-Pythonic/project-workbook-method
  ```

  - `workbook-loader` puts `start.md`, `status.md` and the newest journal entries into
    the session before its first turn, in full.
  - `workbook-triggers` asks, at the end of a turn with signs of recordable work, whether
    an update trigger fired, and has the agent write the entry before it stops.

## What the authors use and you do not need

- The absolute `C:/Home/0_My_Programming/ccd-usage-monitor/...` paths in the README and
  the gotchas are the authors' machine; use the path to your own build.
- The build timings in `overview.md` were measured on the authors' 2014 laptop; yours
  will differ.
- `settings/` is the data folder of a build run from the repository: machine state,
  ignored by git except `theme.slint`.
