# _meta — where the rules for these docs live

The rules for maintaining this documentation are **not copied here**. Two sources of
truth drift apart, which is the disease the method exists to cure.

Standard: https://github.com/Fast-and-Pythonic/project-workbook-method — audited against v2.1
on 2026-09-12.

| You need | Read |
|----------|------|
| What is mandatory, triggers, session protocol | `RULES.md` of the standard |
| How to format a `G##` / `A##`, links, indexes | `FORMATS.md` |
| Which files this project's size warrants | `RULES.md` §6, "Tiers and scaling" |

There is no local method folder: this project has no departures from the standard. If
one becomes necessary, it goes in `workbook-method/` at the root as a `DELTA.md`, and
this file gains a line saying the local method wins where the two disagree.

## Project settings

**Profile:** engineering
**Tier:** M — four areas a session has to hold at once: the sources and their merge,
the view layer, the drawing layer, and the JSON/bridge surface. No `subsystems/` pages
yet; the layer split in A19 is doing that work for now, and the first area whose notes
outgrow a register entry earns a page.
**Language:** English, including these documents. Russian appears only as quoted data —
UI labels in `schema.rs` are bilingual by design and stay as they are.

**Links point outward only.** Nothing here may reference the private environment store
on this machine: this project is public. Where a measurement method needs explaining,
state it in full — `TotalProcessorTime` over 70 seconds, private working set rather
than working set — rather than citing a note nobody else can open.

## The three things most often broken here

1. **`status.md` absorbing the chronicle.** It reached 328 lines against a limit of 80,
   most of it a measurement report for one decision. State goes here; the reasoning
   goes in an `A##`, the story of how it was reached in `journal.md`.
2. **Register indexes going stale.** Both registers ran without one at all. Do not
   hand-edit an index — run `make_index.py`, which regenerates it from the entries.
3. **The same figure in several files.** A number that appears in `overview.md`,
   `status.md` and an `A##` will disagree with itself within a month. One home, and
   links from the others.

## Checking

```
python <standard>/tools/lint_workbook.py --root . --docs project-workbook --profile engineering --tier M
python <standard>/tools/make_index.py --root . --docs project-workbook
```

Both run by themselves before every commit that touches `project-workbook/`, together with
`cargo fmt --check` and `cargo clippy -- -D warnings` for code, through
`tools/hooks/pre-commit` (A32) — once `git config core.hooksPath tools/hooks` has
been run in the clone. Nothing here is advice.
