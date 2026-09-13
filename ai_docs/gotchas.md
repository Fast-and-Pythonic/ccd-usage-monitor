# Gotchas

Non-obvious traps in this project. Each entry is a structured G##.

**Recording trigger:** after debugging for more than 30 minutes with a
non-obvious cause.

---

- **G01** · A backslash path in `statusLine` fails silently
- **G02** · `rate_limits` is legitimately absent, and absence is not zero
- **G03** · `~/.claude/.credentials.json` can be months out of date
- **G04** · eframe 0.36 has no `App::update`
- **G05** · the glow backend breaks the transparent window
- **G06** · the desktop app does not run the status line
- **G07** · a glyph missing from a custom font family renders as `?` or a box
- **G08** · a group's bar started one gap left of the group
- **G09** · four hand-placed egui widgets in one row collide on ids
- **G10** · the countdown was a minute behind the app it mirrors
- **G11** · `ureq` turns an HTTP status into an error, and its text reaches the user
- **G12** · answering `WM_NCACTIVATE` yourself stops the window being dragged
- **G13** · two clocks for one age, drifting by the length of a request
- **G14** · a thread that starts by acting turns "change a setting" into an action
- **G15** · a thin arrow in front of a digit is read as a digit
- **G16** · a window that resizes on hover, read through a pointer that does not move
- **G17** · three lines in a transcript that each report a believable, wrong context
- **G18** · a Slint window handle taken before `show` never starts working
- **G19** · stripping the frame gives the client area the margins it used to spend on it
- **G20** · hiding the last window ends a Slint program
- **G21** · a hidden Slint window comes back with its frame, and loses the bits set on it
- **G22** · a theme one version behind draws nothing, and says nothing
- **G23** · a property pushed on a timer is a repaint, whether or not it changed
- **G24** · the last cell of the pill draws a trailing gap nobody asked for

## G01: A backslash path in `statusLine` fails silently
**Symptom:** the bridge never runs. No error anywhere, no state file, no line
in the terminal status bar.
**Root cause:** on Windows, Claude Code runs the status-line command through
Git Bash when Git Bash is installed. Git Bash treats unquoted backslashes as
escapes, so `C:\<path>\ccd-usage-monitor.exe` reaches the runner with its separators
stripped and the command simply does not resolve.
**Fix:** write the path with forward slashes:
`C:/Home/0_My_Programming/ccd-usage-monitor/target/release/ccd-usage-monitor.exe --bridge`.
**How to spot it:** run the same `command` string by hand in Git Bash; if it
prints nothing at all rather than an error, this is it.

---

## G02: `rate_limits` is legitimately absent, and absence is not zero
**Symptom:** the pill shows `--%` right after a `/clear`, or a window vanishes
from the payload mid-session.
**Root cause:** Claude Code sends `rate_limits` only to Pro/Max subscribers, only
after the session's first API response, and drops a window from the payload once
its `resets_at` has passed. Each of `five_hour`, `seven_day` and `spend_limit`
can be missing independently.
**Fix:** treat missing as unknown everywhere. `StatusLinePayload::apply_to` never
clears a stored window, `StateFile::prune` deletes a window only when its reset
time has actually passed, and the UI renders `--%` in grey rather than `0%`.
**How to spot it:** `%LOCALAPPDATA%\ccd-usage-monitor\state.json` has `sessions` but no
`rate_limits`.

---

## G03: `~/.claude/.credentials.json` can be months out of date
**Symptom:** `ccd-usage-monitor --probe-api` prints `token expired`, while Claude Code
itself works perfectly.
**Root cause:** the credentials file is maintained by the CLI. The desktop app
keeps its authentication elsewhere, so on a machine used only through the
desktop app that file stays frozen at whenever the CLI last logged in — a month
or more.
**Fix:** nothing to fix in the app; the endpoint source is optional by design
and the widget runs on bridge data. Signing in with the CLI (`claude` in a
terminal, then `/login`) refreshes the file and revives the source.
**How to spot it:** `--probe-api` prints the token's expiry date next to the
current time before it gives up.

---

## G04: eframe 0.36 has no `App::update`
**Symptom:** after a version bump, `error[E0407]: method 'update' is not a
member of trait 'eframe::App'`.
**Root cause:** the trait is now `fn ui(&mut self, ui: &mut egui::Ui, ...)`,
which hands you a `Ui` for the whole window, plus `fn logic(&mut self, ctx,
frame)` which runs before it — and, crucially, **also runs while the window is
hidden**, when eframe skips the egui pass entirely.
**Fix:** all state pumping (data sources, tray events, viewport commands) lives
in `logic`; `ui` only paints. Putting tray handling in `ui` would make
"Show" unreachable after "Hide", because nothing would poll the menu channel.
**How to spot it:** hiding the window from the tray works, showing it again does
nothing.

---

## G05: the glow backend breaks the transparent window
**Symptom:** after switching `eframe` to `default-features = false, features =
["glow", ...]`, the pill renders inside an opaque dark rectangle and is clipped
part-way down; the window no longer matches the shape that is painted.
**Root cause:** eframe 0.36 defaults to the **wgpu** renderer, not glow. With
glow the framebuffer config picked on this machine has no usable alpha, so
`clear_color` of `[0, 0, 0, 0]` paints black instead of nothing and the window
size no longer agrees with the painted content.
**Fix:** stay on the default features. The saving was real — 6.8 MB instead of
11.4 MB on disk, 48 MB instead of 121 MB resident, 12 threads instead of 31 —
but not at the price of the entire visual design.
**How to spot it:** a rectangular dark box around the pill. Nothing in the log;
the app runs fine, it just looks wrong.

---

## G06: the desktop app does not run the status line
**Symptom:** `statusLine` is configured and correct, the bridge works when fed a
payload by hand, yet `state.json` never changes while working in the Claude Code
desktop app.
**Root cause:** the status line is a feature of the terminal client. The desktop
app renders its own footer and, as far as the evidence goes, never spawns the
command: over 82 minutes of a continuously active desktop session with the
setting in place, the state file was not touched once.
**Fix:** none available from this side, and none needed any more — this gotcha
is why two of the four sources exist. The quota figures come from the desktop
app's own history (A08), which needs no token, no session and no status line;
the context figure comes from the conversation transcripts (A18), which are
written whatever the client is. The usage endpoint remains an option where the
CLI credentials are current (G03), and the bridge still serves terminal
sessions. What is lost without a status line is only the *stated* context-window
size, which is why running the CLI once is still worth it (A18).
**How to spot it:** `sessions` in `state.json` contains no id matching the
session you are actually working in.

---

## G07: a glyph missing from a custom font family renders as `?` or a box
**Symptom:** `▾` in a menu label came out as `?`, and after adding fallback
fonts, as an empty box.
**Root cause:** `FontFamily::Name(..)` maps to the *list* of fonts you give it.
A list of one font has nowhere to fall back to. Appending egui's bundled
families fixes ordinary text, but neither Segoe UI nor egui's fonts carry
U+25BE, so the box remained.
**Fix:** both halves matter. Custom families are registered as
`[my font, ...egui defaults]` so Cyrillic and punctuation always resolve, and
the chevron is *painted* as a triangle instead of typed — a shape the font
cannot fail to provide.
**How to spot it:** a single character renders wrong while everything around it
is fine.

---

## G08: a group's bar started one gap left of the group
**Symptom:** the hairline tracks under the weekly group and the `ctx` group sat
visibly left of the figures they belong to; the first group looked right.
**Root cause:** after a divider the drawing cursor advances twice — `x +=
GROUP_GAP`, draw the divider, `x += GROUP_GAP` — while the bookkeeping for the
next track advanced once: `group_start = end + GROUP_GAP`. The first group has
no divider before it, which is why only the later ones were wrong.
**Fix:** `group_start = end + group_gap * 2.0`.
**How to spot it:** a bar and its text disagree by exactly one gap, and only
after the first divider.

---

## G09: four hand-placed egui widgets in one row collide on ids
**Symptom:** each stepper row showed a ghost pair of buttons a row below itself,
at the x of a differently-indented row. Clicks on the real buttons did nothing.
**Root cause:** widgets placed with `Ui::put` at hand-computed rectangles take
their id from the surrounding `Ui`, so several of them in one row end up sharing
one id. egui resolves the clash by drawing the later widget offset from its
rectangle, which is the ghost — and interaction goes to whichever of them won
the id.
**Fix:** wrap each in `ui.push_id((row_label, slot), |ui| ui.put(...))`. Note
that painting the button yourself and calling `ui.interact(rect, id, ...)` looks
equivalent and did **not** work here: the click never registered, while the same
`put` inside `push_id` does.
**How to spot it:** a duplicated widget slightly offset from where you placed
it. It is not a paint bug, it is egui telling you about an id clash.

---

## G10: the countdown was a minute behind the app it mirrors
**Symptom:** at 21:50 with the window resetting at 00:10, ccd-usage-monitor showed `2:19`
while Claude Code showed `2:20`; the reset time read `00:09` where the app said
`00:10`.
**Root cause:** integer division. `2 h 19 min 40 s` truncates to 2:19, and
`%H:%M` on a reset at `00:09:40` prints 00:09. Neither is wrong by much, and
both are wrong in the same direction, which makes the widget look stale.
**Fix:** the countdown rounds **up** to the whole minute — there really is more
than 2:19 left — and the reset clock rounds to the **nearest** minute, so the
two agree with each other and with the app.
**How to spot it:** a one-minute disagreement that never exceeds a minute.

---

## G11: `ureq` turns an HTTP status into an error, and its text reaches the user
**Symptom:** the widget's detail panel showed `API: http status: 429`.
**Root cause:** ureq's `http_status_as_error` defaults to true, so a 429
response comes back as `Error::StatusCode(429)`, the body and the headers are
gone, and the error's `Display` text goes straight into whatever shows the
failure. `Retry-After` is unreachable.
**Fix:** build the request with `.config().http_status_as_error(false).build()`
and read `res.status()` and `res.headers()` directly. 429 becomes a schedule
(A13); other statuses become a short `HTTP <code>` rather than a pasted page.
**How to spot it:** an error message that reads like a protocol detail rather
than something a person can act on.

---

## G12: answering `WM_NCACTIVATE` yourself stops the window being dragged
**Symptom:** the pill could no longer be dragged. No error, no log line; the
right-click menu, the tray and every other interaction still worked.
**Root cause:** the subclass added for the white-frame flash (A10) answered
`WM_NCACTIVATE` with 1 instead of forwarding it. Dragging a frameless window is
`ViewportCommand::StartDrag`, which winit implements by posting
`WM_NCLBUTTONDOWN` with `HTCAPTION`; the modal move loop that message starts
depends on the default activation handling that never ran.
**Fix:** forward the message with `lParam = -1`, which suppresses the caption
repaint and leaves everything else alone.
**How to spot it:** an interaction that goes through the *system* rather than
through egui stops working right after a window-procedure change. egui's own
hit-testing is unaffected, which is why everything else still responds.

---

## G13: two clocks for one age, drifting by the length of a request
**Symptom:** in the detail panel the `5h` and `7d` lines said `usage API 0:05`
while the `src` line under them said `API 0:03` — the same reading, two ages,
always one to two seconds apart.
**Root cause:** two timestamps for one event. `Window::observed_at` was stamped
in the poller *before* the HTTP request went out; `api_last_ok` was stamped in
the UI when the resulting event was picked up, which is after the request
returned. The gap was exactly the request's duration, which is why it looked
like a constant, mysterious drift rather than a race.
**Fix:** stamp once, pass it on. The snapshot is parsed with a time taken after
the answer arrives, and `api_last_ok` is read out of that snapshot rather than
sampled again.
**How to spot it:** two numbers that should be equal differ by a small, stable
amount. A race would jitter; a constant offset means two different measurements
of the same thing.
**Note:** the panel now prints an age once, on the `src` line, so the two
numbers can no longer be held side by side there. The invariant lives in the
code, not on the screen.

---

## G14: a thread that starts by acting turns "change a setting" into an action
**Symptom:** editing "requests per minute" in the settings refreshed the
figures over the network on every click — including the clicks meant to *lower*
the request budget.
**Root cause:** the poller took its interval and budget by value, so the only
way to apply a change was to drop the thread and spawn a new one. The loop's
first lap makes a request, so every click cost a request; five clicks could
provoke the very 429 the budget exists to prevent. The same applied to any
source switch, which called `request_api_refresh(true)` unconditionally.
**Fix:** share the settings (`Arc<PollerSettings>`, read each lap) so the
thread never restarts, and split the wake channel into `Wake::Now` and
`Wake::Recheck`. A recheck recomputes the schedule and goes back to sleep.
**How to spot it:** a setting whose *adjustment* has the side effect the
setting is about. Worth asking of any long-lived thread: does its first lap do
something, and what recreates it?

---

## G15: a thin arrow in front of a digit is read as a digit
**Symptom:** the new pace cell showed `2.5×` over pace with an up arrow in
front of it, and on the pill it read as `12.5×` — five times the real figure,
in the direction that matters most.
**Root cause:** Segoe UI's `↑` is a bare vertical stroke with a small head. At
pill size, tight against a digit, it is a `1`. The down arrow has no such twin,
so the problem showed on one side only and was easy to miss in review.
**Fix:** `▲` / `▼`. Solid triangles share no shape with any digit, and they
are the conventional up/down mark besides.
**How to spot it:** read a new glyph off a screenshot at real size, never off
the source. Anything that sits directly against a number — arrows, ticks,
primes, multiplication signs — gets checked for a digit it could pass for.

---

## G16: a window that resizes on hover, read through a pointer that does not move
**Symptom:** pointing at a figure made the widget flicker — the hint opened and
closed several times a second — and after a few seconds of it the window stopped
responding.
**Root cause:** the hint opens above the pill when the pill sits near the bottom
of the screen, which grows the window upwards, which means moving it. egui's
pointer position is window-relative and is only refreshed by a real mouse event,
so a still pointer kept the position it had before the move: 50 px too high,
which is the empty band the hint had just been drawn in. Nothing under the
pointer meant no hint, which shrank the window back, which restored the old
reading, which opened the hint again. Two viewport commands per frame — resize
and move — is also enough to wedge winit's event loop.
**Fix:** hold the last name while the pointer is anywhere over the widget, and
only drop it when the pointer leaves the window altogether. The detail panel had
avoided the same loop by accident, with `ctx.input(|i| i.pointer.has_pointer())`
beside its own `hovered()`.
**How to spot it:** anything whose *size or position* depends on what the
pointer is over is a feedback loop, and window-relative coordinates are the
input that goes stale in it. Watch the window rectangle over a couple of
seconds with the mouse held still (`GetWindowRect` in a loop) — a stable
rectangle is the proof, a screenshot is not: it catches one side of the
oscillation and looks merely wrong rather than unstable.

---

## G17: three lines in a transcript that each report a believable, wrong context
**Symptom:** the context figure reads 0 % in the middle of a busy session, or
five times too high, or matches a subagent rather than the conversation.
**Root cause:** three different lines in `~/.claude/projects/**/<uuid>.jsonl`
answer to "the last assistant line with a usage object", and only one of them is
the conversation's own reading.

* **Synthetic lines.** An interrupted request, a 529, an auth failure is written
   as `"model":"<synthetic>"` with every counter zero. Taking the last assistant
   line lands on one of these whenever the last thing that happened was an error,
   and reports a confident nought per cent.
* **Sidechain lines.** A subagent's transcript lives in
   `<uuid>/subagents/agent-*.jsonl` and carries the **parent's** `sessionId`, so
   the id cannot tell them apart. Enumerating the tree recursively lists them as
   sessions; matching on `sessionId` alone lets a subagent's context stand in for
   the conversation's. `isSidechain` is the field that separates them.
* **Repeated lines.** One API message is written as several lines sharing one
   `message.id`, each repeating the same usage verbatim — one per content block.
   Summing them multiplies the reading by however many blocks the answer had.

**Fix:** require `type == "assistant"`, `isSidechain != true`, `model !=
"<synthetic>"` and a sum above zero, take the last such line by *position* in
the file, and never add two together. Enumerate transcripts one level deep, so
`subagents/` and `memory/` are never walked into.
**How to spot it:** a figure that is exactly zero while the session is busy, or
one that jumps by a large round factor between two reads. `ccd-usage-monitor --sessions`
prints the token count beside the percentage for every transcript, which is
where a wrong numerator shows up as an implausible number rather than as a
plausible percentage.

---

## G18: a Slint window handle taken before `show` never starts working
**Symptom:** `Frame::from(window.window_handle())` returned `None` every time,
a hundred ticks in a row, while `window.is_visible()` said `true` and the
window was plainly on the screen. Nothing about the code said which of the two
was lying.
**Root cause:** `slint::Window::window_handle()` is a snapshot of whatever the
window adapter could provide *at the moment it was called*, not a live view. It
is documented to be available only "after at least one iteration of the event
loop following `show()`", and the spike had taken the handle before `show` and
kept it in a closure. The retry loop then asked the same dead snapshot again on
every tick, so retrying could never have helped.
**Fix:** ask the window for a handle each time one is needed — from inside the
callback, through an upgraded `Weak<ComponentInstance>` — rather than capturing
one. The first tick after the event loop starts then hands back a real
`RawWindowHandle::Win32`.
**How to spot it:** print the `Result` from `HasWindowHandle::window_handle`
rather than the `Option` a helper returns. An `Err` naming the handle as
unavailable is a different diagnosis from "this is not a Win32 window", and
`Option` throws exactly that difference away.

---

## G19: stripping the frame gives the client area the margins it used to spend on it
**Symptom:** the pill drew at the size the theme asked for on the first frame
(450x43 physical at 125 %) and was 468x90 on the second — a fifth wider and
twice as tall — with no resize anywhere in the code.
**Root cause:** `win::strip` answers `WM_NCCALCSIZE` with "the client area is
the whole window". The *outer* rectangle does not change, so everything the
caption and the sizing frame used to occupy becomes client area, and the
drawing grows into it. The window had been created at the size the theme asked
for, which was that size plus a frame.
**Fix:** state the size again after the surgery, not before it — one
`set_size` in the same callback that strips the frame. The rectangle is then
stable, which is what `GetWindowRect` in a loop is for (G16).
**How to spot it:** compare the toolkit's own idea of the window size with
`GetWindowRect` on the same tick. Two numbers that agree at first and diverge
by exactly a frame's worth of pixels is this, and it is invisible in a
screenshot: the picture is merely bigger than expected, which reads as a
layout mistake.

---

## G20: hiding the last window ends a Slint program
**Symptom:** the widget quit when the tray's "Hide" was used. No panic, no
error — `run` returned `Ok(())` and `main` fell off the end, which reads in a
log exactly like a clean shutdown somebody asked for.
**Root cause:** `ComponentInstance::run()` is show-plus-run, and the run half
is `slint::run_event_loop()`, which is documented to return once the last
window is gone. The widget spends whole afternoons hidden in the tray with no
window at all (G04), which is the one state that function treats as "there is
nothing left to do".
**Fix:** `show()` the component, then `slint::run_event_loop_until_quit()`,
which ends only on `quit_event_loop`. The tray's Quit then becomes the one way
out, which is what it was supposed to be anyway.
**How to spot it:** any toolkit whose event loop is tied to window lifetime
does this. The tell is that the program ends *at the moment* the last window
is hidden rather than at the moment anything failed.

---

## G21: a hidden Slint window comes back with its frame, and loses the bits set on it
**Symptom:** the pill came back from the tray with a title bar and a drop
shadow, and a panel marked "never take the focus" started taking the focus
again after its first hide.
**Root cause:** `hide()` followed by `show()` re-applies the window styles
winit computed from its own flags, over both `GWL_STYLE` and `GWL_EXSTYLE`.
Everything `win.rs` did to the window is undone by it. This is not new — A10
records the same thing under egui, where a visibility change is one of the two
commands that put the shadow back — but under Slint it reaches the extended
styles as well, and `WS_EX_NOACTIVATE` cannot simply be re-applied afterwards:
the `show` that would undo it is the same call that steals the focus.
Measured on the same window, before and after one hide/show:

```
before hiding      caption=false thickframe=false  toolwindow=true
straight after show caption=true  thickframe=true   toolwindow=false
```

**Fix:** two different answers for two different windows. The pill is hidden
and shown, so everything is re-applied after every `show` — the frame strip,
`WS_EX_TOOLWINDOW` (which is what keeps it out of the taskbar and Alt+Tab, and
which `ViewportBuilder::with_taskbar(false)` used to do), and the size, since
stripping the frame moves the client area again (G19). The panel is **never**
hidden — it is parked off-screen (`SetWindowPos` to −10000, −10000, with
`SWP_NOACTIVATE`) and moved back when it is wanted. A window that is never
hidden has nothing to restore.
**How to spot it:** read the style bits back after the operation rather than
trusting the call — `GetWindowLongPtrW` on `GWL_STYLE` and `GWL_EXSTYLE`. The
frame coming back is visible; a lost `WS_EX_NOACTIVATE` is not, until somebody
loses what they were typing.

---

## G22: a theme one version behind draws nothing, and says nothing
**Symptom:** the pill came up 36 pixels wide — its padding and no content — on
a build that had worked minutes earlier. No error, no diagnostic, no missing
property. The same binary drew correctly when the theme file was deleted.
**Root cause:** the theme is a file on disk that shadows the copy in the
binary, and it had been written before a field was added to the `PillCell`
struct. Slint matches a struct passed through the interpreter field by field,
so the whole `groups` model was refused with `SetPropertyError::WrongType` —
and the "tolerant contract" was tolerating it, because `set_property`'s result
was being dropped on the floor. Tolerance had been built for *properties* the
theme might not declare, which is safe; a struct that no longer lines up is a
different failure and not a survivable one.
**Fix:** two things, and both were needed.
1. `theme_api` is checked rather than merely declared. The theme states which
   contract it was written against, the program knows which one it sends, and
   a mismatch means the file is not used at all — the built-in copy draws, and
   stderr says which version was found and which was wanted. Bumped whenever a
   struct changes shape.
2. `set_property` failures are printed, once per property name. "Tolerant" has
   to mean "carries on and says so".
**How to spot it:** a UI that is present, correctly sized around its padding,
and empty. That shape — the frame without the content — is a model or a list
that was rejected rather than one that came back empty, because an empty model
still lays out its surroundings the same way.

---

## G23: a property pushed on a timer is a repaint, whether or not it changed
**Symptom:** the first release build of the Slint front end spent 1.9 seconds
of CPU per seventy at rest, against 0.16 for the egui one and 0.17 for the
Slint spike. Twelve times the cost, with nothing on screen moving and no
obvious suspect: the same toolkit had measured as cheap as egui a day earlier.
**Root cause:** the tick runs four times a second, because that is how often
the state file is worth looking at, and it was rebuilding the model of cells
and handing it to the theme every time. Slint has no way to know the new model
says the same thing as the old one, so each push meant a fresh layout and a
fresh paint — four identical pictures a second. The figures in them change
about once a minute.
**Fix:** keep what was last given to the theme and compare before pushing.
`view::Cell` and `view::Bar` derive `PartialEq` for exactly this. The cost
fell to 0.19 s per seventy — measured on the *debug* build, which is slower
than the release one that produced the 1.9.
**How to spot it:** the giveaway is a cost that scales with the *tick* rather
than with anything visible. Divide the CPU by the tick rate before blaming the
toolkit: 1.9 s over 70 s at four ticks a second is about 7 ms of work per tick,
which is a repaint, not a file check. The same trap is waiting in the settings
window, where a form bound to config values would otherwise repaint on a timer
for no reason.

---

## G24: the last cell of the pill draws a trailing gap nobody asked for
**Symptom:** the `ctx` percentage — or whichever figure ends up last on the
pill — sat with visible empty space to its right, inside the pill's own
rounded card rather than as ordinary window margin.
**Root cause:** `pill.slint` draws a gap `Rectangle` after *every* cell,
sized by that cell's own `gap` field (`view::Cell::new` defaults to
`Gap::Cell`, an ordinary space) — there is no separate code path for "this is
the last cell on the pill, draw nothing." `view::cells()` did call
`end_group()` to close the 5-hour and 7-day groups before starting the next
one, but nothing ever closed the *final* group, because closing it was
assumed to be pill.slint's job — and pill.slint's own "last group" check
(`g.last`) only suppresses the divider *between* groups, drawn by a separate
Rectangle one level up. It says nothing about the ordinary per-cell gap,
which fires regardless.
**Fix:** call `end_group(&mut cells)` once more, unconditionally, on the
whole vector right before `cells()` returns — whichever cell ended up last
(a quota figure if `ctx` is off, `ctx`'s own last figure otherwise) gets
`Gap::Group`, which `pill.slint` maps to `0px`.
**How to spot it:** any time a new field is appended after what used to be
the last cell (or a conditional branch changes which cell ends up last),
check whether the *true* last cell of the whole pill — not just the last
cell of its own group — carries a non-zero-width gap. Grep for `end_group`
calls in `view.rs`: there should always be exactly one more of them than
there are group boundaries.
