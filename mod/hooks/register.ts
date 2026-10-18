// The desktop app never runs the status line (G06), so this module stands in
// for it (A37): each time the engine measures the session it hands the figures
// to `ccd-usage-monitor --bridge` in the status line's own JSON, and it pins
// the pill's line, as `--line` words it, under the prompt. Nothing here decides
// what a figure says; that stays in the executable.

import type { EngineInterface, PluginOptions, Register, SessionContextUsage, SessionRateLimit } from 'claude-code'

/** How often the pinned line is redrawn between measurements, so the countdowns move while the session is idle. */
const REFRESH_MS = 60_000
const RUN_TIMEOUT_MS = 10_000

let exePath: string | undefined
let warned = false

/** The status line's payload (`StatusLinePayload` in src/state.rs), from the engine's figures. Absent figures are left out, not zeroed: the bridge keeps what it had. */
export function payload(
  sessionId: string,
  model: string,
  context: SessionContextUsage,
  rateLimits: readonly SessionRateLimit[],
): Record<string, unknown> {
  const window: Record<string, number> = { context_window_size: context.window }
  if (context.percent !== undefined) window.used_percentage = context.percent
  if (context.tokens !== undefined) window.total_input_tokens = context.tokens
  const limits: Record<string, Record<string, number>> = {}
  for (const w of rateLimits) {
    if (w.kind !== 'five_hour' && w.kind !== 'seven_day') continue
    const entry: Record<string, number> = { used_percentage: w.percentUsed }
    const ms = w.resetsAt === undefined ? NaN : Date.parse(w.resetsAt)
    if (!Number.isNaN(ms)) entry.resets_at = Math.floor(ms / 1000)
    limits[w.kind] = entry
  }
  const out: Record<string, unknown> = { session_id: sessionId, model: { id: model }, context_window: window }
  if (Object.keys(limits).length > 0) out.rate_limits = limits
  return out
}

/** The executable named in a status line command that runs the bridge, quoted or not. */
export function exeFromStatusLine(command: string): string | undefined {
  const m = /^\s*(?:"([^"]+)"|(\S+))\s+--bridge\s*$/.exec(command)
  return m ? (m[1] ?? m[2]) : undefined
}

async function resolveExe($: EngineInterface, options: PluginOptions): Promise<string | undefined> {
  if (exePath !== undefined) return exePath
  const fromOptions = typeof options.exe === 'string' ? options.exe.trim() : ''
  if (fromOptions !== '') {
    exePath = fromOptions
    return exePath
  }
  const settings = await $.settings.read()
  const statusLine = settings.statusLine as { command?: unknown } | undefined
  if (typeof statusLine?.command === 'string') exePath = exeFromStatusLine(statusLine.command)
  return exePath
}

/** Only where something other than a terminal draws: a terminal runs the status line itself, and a second copy of the line there would be noise. */
async function inApp($: EngineInterface): Promise<boolean> {
  return (await $.session.surfaces()).some(s => s !== 'terminal')
}

function fail($: EngineInterface, why: string) {
  $.ui.status(undefined)
  if (warned) return
  warned = true
  $.ui.log(`ccd-usage-monitor: ${why}`)
}

async function refresh($: EngineInterface, options: PluginOptions) {
  if (!(await inApp($))) return
  if (options.showLine !== true) return $.ui.status(undefined)
  const exe = await resolveExe($, options)
  if (exe === undefined) return fail($, 'no executable: set the plugin option `exe`, or a statusLine command ending in --bridge')
  try {
    const id = await $.session.id()
    const r = await $.process.run([exe, '--line', '--session', id], { timeoutMs: RUN_TIMEOUT_MS })
    if (r.exitCode !== 0) return fail($, `--line exited ${r.exitCode}: ${r.stderr.trim()}`)
    $.ui.status(r.stdout.trim() || undefined)
  } catch (err) {
    fail($, `--line failed: ${String(err)}`)
  }
}

async function feed($: EngineInterface, options: PluginOptions, context: SessionContextUsage, rateLimits: readonly SessionRateLimit[]) {
  if (!(await inApp($))) return
  const exe = await resolveExe($, options)
  if (exe === undefined) return
  try {
    const body = payload(await $.session.id(), await $.session.model(), context, rateLimits)
    await $.process.run([exe, '--bridge'], { stdin: JSON.stringify(body), timeoutMs: RUN_TIMEOUT_MS })
  } catch (err) {
    fail($, `--bridge failed: ${String(err)}`)
  }
}

export const register: Register = (on, options) => {
  on('session.start', async ($, e, next) => {
    const r = await next(e)
    // The other sources have figures before this session's first response does.
    await refresh($, options)
    $.clock.every(REFRESH_MS, () => {
      void refresh($, options)
    })
    return r
  })

  on('session.measure', async ($, e, next) => {
    // Only a measurement feeds the bridge: a timer that did would make an idle
    // session the newest reporter every minute and take the pill's context
    // figure away from the conversation actually in use.
    await feed($, options, e.context, e.rateLimits)
    await refresh($, options)
    return next(e)
  })
}
