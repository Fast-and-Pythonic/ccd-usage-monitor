# turn-meter

A Claude Code mod that draws a dim line under each reply in the desktop app:

```
ctx 21% · 212k │ 0.8% 5ч · 0.12% нед │ $0.31
```

How full the session's context is, what share of the plan's five-hour and weekly
limits the turn used, and what it cost at API prices. More parts can be switched
on in the plugin's settings: tokens by kind, subagents, duration, requests,
model, when the prompt cache lapses, and the size of each window in dollars.
`/turn-meter` prints the learned prices, the windows behind them, and a day's
comparison with the whole-point steps of the limits' readings.

The labels are in Russian, the author's language.

## How the share is found

Claude Code reports a session's cost in dollars, but the limits only as whole
percentages for the whole account. The mod takes the turn's dollars from the
session's ledger and divides them by a price of one point of each window, which
it learns from how the readings move against the spend of every session that
runs the mod. Models are priced apart: on the author's Pro account in October
2026 a point of five hours came to about $0.42 of Opus 5.5 and $0.24 of
Sonnet 5.5.

The price is learned, not known, and the figure is rough while it is:

- **The first hours.** Until three five-hour windows have risen enough to fit,
  the price is blended with a starting value ($0.39 a point of five hours,
  $2.30 of the week), and the line ends with `(цена учится)`.
- **Spend between turns** (a background subagent finishing after its turn) is
  in the ledger but under no reply. `/turn-meter` shows it.
- **Spend it never sees** (claude.ai, the phone, another computer, a session
  without the mod) moves the windows anyway. Windows where that shows are left
  out of the fit; the line cannot know about it.
- **Calls other mods make themselves** (`$.model.complete`) are not in the
  session's ledger, so no line counts them.

## What it reads, writes and sends

- **Reads** the session's cost, token counts and limit readings that Claude
  Code hands to mods (`$.session.usage()`, turn and step events).
- **Writes** one small JSON journal per session (cost over time, limit readings,
  per-model spend, one row per turn) and the fitted windows. Installed from a
  marketplace, they go to `~/.claude/plugins/data/turn-meter-<marketplace>/`;
  loaded from a folder, to `data/` beside the manifest. Journals older than
  eight days are dropped. `/turn-meter reset` forgets the learned prices; the
  time of that reset is kept in the plugin's store (`$.store`).
- **Reads** `results.json` of the sibling mod limit-test, if it is installed, as
  measured starting prices.
- **Sends nothing anywhere.** No network calls, no processes started, no
  model calls of its own.

## Install

```
claude plugin marketplace add Fast-and-Pythonic/ccd-usage-monitor
claude plugin install turn-meter@ccd-usage-monitor
```

More, and the widget it comes with: https://github.com/Fast-and-Pythonic/ccd-usage-monitor

MIT licence.
