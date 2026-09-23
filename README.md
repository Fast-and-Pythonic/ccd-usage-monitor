# ccd-usage-monitor

A floating one-line readout for Windows 10/11: how much of your Claude Code
quota is gone, and how full the current session's context window is. Aimed first
at the Claude Code **desktop** app, which shows neither and does not run the
status line the terminal client reports them through. Native Rust + Slint, one
executable, no web stack — and the whole appearance is a `.slint` file you can
edit without a compiler.

```
24% ▼1.2× 3:19 20:49 │ 41% ▲1.4× 5d 20% │ ctx 38%
```

| Field | Meaning |
|---|---|
| `24%` | share of the rolling 5-hour limit already spent — or what is left of it, per `pct_mode` |
| `▼1.2×` | pace, next to the figure it qualifies: spending is running 1.2 times *slower* than an even burn, so there is headroom; `▲` is the other way. `pace_view` picks the form — the ratio, the gap in points (`▲14%`), the same gap in time (`▲0:42`), both of the first two, or neither |
| `3:19` | time left until that window resets |
| `20:49` | local clock time of the reset |
| `41%` | share of the weekly limit spent, under the same setting |
| `▲1.4×` | the same pace figure for the weekly window |
| `5d` | days left until the weekly window resets — whole calendar days, which of them count being a setting, or with `days_by = time` the exact time to the reset (`4d7h`) |
| `20%` | how much of the weekly limit may be spent per day to finish exactly at the reset — the remainder divided by the day count beside it, so by time half a day and 40% left reads `80%` |
| `ctx 38%` | how full the session's context window is — the share, the token count (`309k`), or both, per `ctx_view` |

Under each group a hairline track shows the same figure as a bar, so switching
to "left" makes the bars drain instead of fill. Colour never switches with them:
green through amber to red is keyed on spending either way. `bar_colour = pace`
keys it on the pace instead, which is a different question — a full bar and a
bar about to overflow are not the same thing — and then a short red bar means
quota left with no time to spend it in.

Point at a figure and a small box under the pill names it — `Until the 5-hour
limit resets`, `Weekly limit pace` — and says what it is right now: the date and
hour of the reset under the day count, what the pace means in words and in all
three units, the other reading of a percentage and where it came from. The bars
are covered too. With the detail panel already open the hint goes in it as the
first lines instead, so one hover never opens two surfaces.

Pace is coloured on a scale of its own: green only while spending is at or
under the clock, and orange deepening to red the moment it is over. There is no
green above the line — a ramp that starts there makes "just over" look like
"fine" — and the word in the panel switches at the same point, so `pace_tolerance`
now only separates "on pace" from a real surplus on the safe side.

The detail panel prints both readings at once (`62% spent · 38% left`), both
pace figures with the word for them, and, from the widget's own log of past
readings, how fast the quota is actually going and where that ends:

```
5h  9% left · 91% spent · resets Sat 08:00 · ▲2.5× 14% over pace · 20%/h · runs out 07:19, 0:42 before the reset · usage API
7d  58% left · 42% spent · resets Tue 10:00 · ▼1.3× 8% under pace · 17%/day · lasts to the reset · usage API · 19%/day allowed
```

That log lives in `%LOCALAPPDATA%\ccd-usage-monitor\samples.json`, holds eight days, and is
seeded at startup from the Claude desktop app's own `plan-usage-history.json`,
so the rate is answerable on the first minute rather than after an hour of
watching. The seeding is not tied to the **Desktop app** source switch: that
switch decides whose figures are shown now, while this is a one-off read of a
file already on disk.

Right-clicking the pill opens the settings in a window of their own — sources,
sizes, opacity, language, click-through — and the tray icon opens the same
window, which is the way in when clicks fall through the pill. Every row there
carries a hover hint saying what it actually does; the labels are only as long
as a row with four stepper buttons allows.

The detail panel under the pill is sized to its longest line and appears
according to `detail_mode`: with the settings window (the default), on hover, or
never. Near an edge of the screen it opens upwards or leftwards instead of
pushing the pill out of place. Each of its lines says one thing once: the window
lines name the source a figure came from, and how old each source is appears on
the `src` line below them.

Figures that stop being refreshed grey out after 15 minutes, with an amber dot
on the pill, and disappear entirely after an hour — a stale percentage is not a
slightly-old number, it is a different window.

## Install

```
cargo build --release
```

Working on the code, enable the checks a commit has to pass — `cargo fmt`,
`cargo clippy` with no findings, and the docs linter where it is available:

```
git config core.hooksPath tools/hooks
```

Then point Claude Code's status line at the same binary, in
`~/.claude/settings.json`:

```json
"statusLine": {
  "type": "command",
  "command": "C:/Home/0_My_Programming/ccd-usage-monitor/target/release/ccd-usage-monitor.exe --bridge"
}
```

Forward slashes are required: on Windows Claude Code runs the status-line
command through Git Bash, which eats unescaped backslashes and then fails
silently.

Run `ccd-usage-monitor.exe` to show the widget. Position, opacity, sizes, language and the
source setting live in `%APPDATA%\ccd-usage-monitor\config.toml`.

## Sizes

Right-click → **Sizes** opens a set of rows with `−− −` and `+ ++` on either
side, one small step and one large one each. What they set:

| Key | Default | What it does |
|---|---|---|
| `scale` | `1.0` | multiplies everything below, and the paddings with them |
| `font_pct` | `14.0` | the large percentages |
| `font_time` | `14.0` | countdown, day count, daily allowance |
| `pill_height` | `34.0` | height of the pill; text and bar positions follow it |
| `track_height` | `3.0` | thickness of the bars — `0` hides them; the pill grows to keep them inside |
| `pad_top` | `4.5` | clear space above the figures |
| `gap_text_bars` | `3.0` | between the figures and the bars |
| `pad_bottom` | `4.5` | below the bars |
| `gaps_x` | `1.0` | one multiplier for every horizontal gap: between the figures, either side of a group divider, and from the edge of the pill |
| `corner_radius` | `10.0` | how round the pill, the panel and the hints are — `0` for square corners, half the pill's height for the original lozenge |
| `pct_mode` | `spent` | whether the percentages and the bars count what is spent or what is `left` |
| `detail_mode` | `with_settings` | when the detail panel shows: `with_settings`, `hover`, `never` |
| `days_by` | `calendar` | how the weekly days are counted: `calendar` (whole days, the two settings below decide the ends) or `time` (the exact time to the reset, `4d7h`, and the allowance divides by it) |
| `count_today` | `false` | by calendar only: whether today counts towards the day figure and the allowance |
| `count_reset_day` | `true` | by calendar only: whether the day the quota resets counts — switch it off when the reset is in the morning |
| `pace_view` | `ratio` | the form of the pace figure: `ratio` (`▲2.0×`), `points` (`▲9%`), `time` (the gap in the window's time: `▲0:27` on the 5-hour window at 3 min a point, `▲3d2h` on the weekly one), `ratio_points`, `points_ratio`, `split` (both, with a drawn rule between them), `piped` (`▲9%|2.0×`, run together with a grey pipe) or `off` |
| `bar_colour` | `fill` | what a bar's colour means: `fill` for how spent it is, `pace` for how fast it filled |
| `pace_rate` | `true` | the observed rate and the forecast in the detail panel |
| `ctx_view` | `pct` | the form of the `ctx` group: `pct` (`14%`), `tokens` (`142k`), `pct_tokens`, `tokens_pct`, or `off` |
| `ctx_limit` | `learned` | how the context percentage gets its denominator: `learned` (the size the status line stated for that model, the table underneath), `table` (by model name only), `fixed` |
| `ctx_limit_tokens` | `200000` | the denominator when `ctx_limit = "fixed"`, clamped to 8 192 … 1 000 000 |
| `ctx_dir_linked` | `true` | whether the `ctx` percentage and token count share one direction, or are set separately below |
| `ctx_pct_dir` | `spent` | the direction of the `ctx` percentage: `spent` or `left`. Independent of `pct_mode` — a window fills, it does not drain like the two quotas |
| `ctx_tokens_dir` | `spent` | the direction of the `ctx` token count; only read when `ctx_dir_linked = false` |
| `session_pick` | `live` | which session the context figure is about: `live`, `freshest`, or `pinned` |
| `session_id` | `""` | the conversation pinned by hand, when `session_pick = "pinned"` |
| `session_scan_secs` | `30` | how often the transcript tree is enumerated again |
| `data_dir` | `""` | empty is the usual place; naming a folder moves `config.toml`, `theme.slint` and everything else this program keeps for itself there, and restarts the widget. Useful when the usual place is not directly editable (A28) |

`scale` is the knob to reach for when the widget moves to another monitor;
the rest are for taste. **Reset sizes** puts them all back. Opacity has the same
pair of steps, 1% and 5%.

## Modes

| Command | What it does |
|---|---|
| `ccd-usage-monitor` | the widget |
| `ccd-usage-monitor --demo` | the widget on synthetic data, for working on the layout |
| `ccd-usage-monitor --bridge` | status-line mode: reads one JSON payload on stdin, updates the state file, prints the same line for the terminal |
| `ccd-usage-monitor --print` | prints the current line and exits |
| `ccd-usage-monitor --probe-api` | one request to the usage endpoint, printed raw |
| `ccd-usage-monitor --sources` | what each source reports right now, side by side |
| `ccd-usage-monitor --sessions` | every conversation on disk, what it reports, and which one is picked |

## Where the numbers come from

Four sources, each an independent switch.

**The desktop app's history** — `%APPDATA%\Claude\plan-usage-history.json`. The
Claude desktop app appends its own usage sample there roughly every fifteen
minutes: `{"t": <epoch ms>, "org": "<uuid>", "u": {"fh": 8, "sd": 19}}`, where
`fh` and `sd` are the five-hour and seven-day percentages. It asks nothing of
you — no token, no session, no status line — and it is the source that keeps
working when everything else lapses. It carries no reset times, so on its own
the countdowns read `--:--`.

**The bridge** — Claude Code passes its status-line command a JSON payload with
`rate_limits.five_hour`, `rate_limits.seven_day`, `context_window` and the
model's id. `--bridge` folds each payload into `%LOCALAPPDATA%\ccd-usage-monitor\state.json`,
and the widget watches that file. This is the documented path, and the only one
that states the *size* of the context window outright — which is why one
terminal session is worth running even if you work in the desktop app: the size
is remembered per model and used by the transcript reader afterwards. Note that
the status line is a feature of the *terminal* client: the desktop app appears
never to run it.

**The session transcripts** — `~/.claude/projects/<encoded-cwd>/<uuid>.jsonl`,
the conversation files Claude Code writes for itself, whichever client is
running. The last assistant line of one carries the token counters, and their
sum is what sits in the context window:

```json
{"type":"assistant","message":{"model":"claude-opus-5","usage":{
  "input_tokens":2,"cache_creation_input_tokens":817,
  "cache_read_input_tokens":88139,"output_tokens":569}}}
```

`input_tokens + cache_creation_input_tokens + cache_read_input_tokens` — the
same three counters, and not the output, that Claude Code's own status-line code
divides. This is the only source of a context figure that works in the desktop
app, where the status line never runs.

What it cannot know is the *denominator*. The size of a model's window comes
from a catalogue fetched from the server, and nothing on disk records it, so
`ctx_limit` decides: the size the status line stated for that model if one ever
did, a small built-in table by model name otherwise (the 5-series has a
million-token window, everything before it two hundred thousand), or a number
you set by hand. Getting this wrong is not a rounding error — the same 88 958
tokens are 9 % of a million and 44 % of two hundred thousand — so the detail
panel prints the whole division and marks a guessed denominator `(guessed)`,
`ctx_view` can put the raw token count on the pill beside the percentage, and a
reading larger than the assumed window raises it rather than pinning at 100 %.

A context window fills toward its limit; the 5-hour and weekly quotas drain
from theirs. Reusing `pct_mode` for `ctx` would make one of the two figures
wrong on every pill where the other is set to taste, so `ctx` keeps its own
direction — `ctx_pct_dir` for the percentage, `ctx_tokens_dir` for the count,
`ctx_dir_linked` for whether the two follow each other or are set apart.

`session_pick` says which conversation is reported: `live` reads the freshest of
those Claude Code currently lists in `~/.claude/sessions` — and stays quiet when
none is running, rather than falling back to a finished one; `freshest` takes
whichever transcript was written to last; `pinned` takes the one you chose in
the settings window. `ccd-usage-monitor --sessions` prints the same list with the figure
each one would report.

**The usage endpoint** — `https://api.anthropic.com/api/oauth/usage` with the
OAuth token from `~/.claude/.credentials.json`. Exact percentages *and* reset
times, with no session open. It is undocumented — a community discovery — and
the access token is short-lived; ccd-usage-monitor only ever reads that file and never
refreshes the token, so when it expires the endpoint goes quiet until Claude
Code is run again. The widget says so in the detail panel and carries on with
the other two.

Each source is a switch of its own (`bridge_enabled`, `api_enabled`,
`desktop_enabled`, `session_enabled`), and whatever is switched on is merged
field by field: the percentage from whichever source read it last, the reset
time from whichever source has one. That pairing matters — a reset time is an
absolute moment, so an hour-old reading of it is still correct, while a
percentage from the same reading is not.

The context figure merges the same way, and for the same reason: the fill comes
from the newest reading, the window's size from whichever source was told it.
So one terminal session is enough to teach the widget how big a model's window
is, and every transcript read afterwards stops guessing — even in the desktop
app, which never runs a status line at all.

Each source also has its own staleness threshold (`*_stale_secs`), because they
age differently: 15 minutes for the endpoint and the status line, 30 for the
desktop history, which is only written every quarter of an hour.

Background polling of the endpoint is deliberately slow (`api_interval_secs`,
180 s by default). Responsiveness comes from wake-ups instead: the poller goes
immediately on startup, on switching a source, on "refresh now", and whenever
the desktop app writes a new sample.

Both are shown and edited as `m:ss` — a 150-second interval reads `2:30`, not
"2 min". The endpoint rate-limits, so ccd-usage-monitor keeps a budget of its own —
`api_budget_per_min`, four by default. Past it the poller waits a few seconds
and then makes the request anyway, which is a far better deal than the minutes
a real 429 costs.

## Requirements

- Windows 10/11
- Claude Code 2.1.x or newer for `rate_limits` in the status-line payload
- A Claude Pro or Max subscription; API-key users get no rate-limit fields

## The look is a file

The pill's whole appearance — palette, type, paddings, corner radius, the
thickness of the bars — is `theme.slint`, written to
`%APPDATA%\ccd-usage-monitor\` the first time the widget runs. Edit it and use
"Перезапустить" / "Restart" in the tray or the settings window; no Rust
toolchain is involved.

What the theme does **not** decide is deliberate: the wording and the figures
arrive already worked out, which colour a figure gets is decided by what it
means rather than by the file, and the window's behaviour — frameless, on top,
click-through — is applied by the program. A broken theme falls back to the
built-in copy and says what was wrong; it cannot produce a window you are
unable to close.

## Built with Slint

This application uses [Slint](https://slint.dev) under its royalty-free
licence, which asks that the use be visibly acknowledged. It is: in the
settings window, and in the tray menu.
