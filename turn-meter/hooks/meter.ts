// Pure parts of turn-meter: the line under a reply, and the price of one point
// of a usage window learned from every session's spend journal (A38).

import type { ModelUsage, SessionRateLimit } from 'claude-code'

/** Window lengths, to find where a window began from when it resets. */
export const WINDOW_MS: Record<string, number> = {
  five_hour: 5 * 60 * 60 * 1000,
  seven_day: 7 * 24 * 60 * 60 * 1000,
}

/** Smallest rise, in points, a window needs before its slope is trusted (eight points for five hours, three for the week). */
export const MIN_RISE: Record<string, number> = { five_hour: 8, seven_day: 3 }

/** A learned price this far off the configured one is taken for spend the journal never saw. */
const SANE_RATIO = 2
/** How many accepted windows the median is taken over, and how many it takes to trust it alone. */
const RECENT_WINDOWS = 10
const TRUSTED_WINDOWS = 3
/** Below this fit quality a window had spend the journal did not see, or a limit change inside it. */
const MIN_R2 = 0.9

/** Journal entries older than this are dropped; the weekly window needs seven days of them. */
export const KEEP_MS = 8 * 24 * 60 * 60 * 1000

/** One session's journal: its total cost over time, and each change of a window's reading. */
export type SessionLog = {
  session: string
  /** `[time, session cost in USD]`, written when the cost grows. */
  spend: [number, number][]
  /** `[time, kind, resetsAt, percentUsed]`, written when a reading changes. */
  obs: [number, string, string, number][]
}

export const emptyLog = (session: string): SessionLog => ({ session, spend: [], obs: [] })

/** Adds a cost point when the total moved. */
export function addSpend(log: SessionLog, t: number, usd: number | undefined): void {
  if (usd === undefined) return
  const last = log.spend[log.spend.length - 1]
  if (last !== undefined && last[1] === usd) return
  log.spend.push([t, usd])
}

/** Adds a reading of each window whose value or reset moved since this session last saw it. */
export function addReadings(log: SessionLog, t: number, limits: readonly SessionRateLimit[]): void {
  for (const w of limits) {
    if (WINDOW_MS[w.kind] === undefined || w.resetsAt === undefined) continue
    const last = log.obs.findLast((o) => o[1] === w.kind)
    if (last !== undefined && last[2] === w.resetsAt && last[3] === w.percentUsed) continue
    log.obs.push([t, w.kind, w.resetsAt, w.percentUsed])
  }
}

export function prune(log: SessionLog, now: number): void {
  const from = now - KEEP_MS
  // Keep the last cost point before the cut: it is the baseline of a window that began before it.
  const firstKept = log.spend.findIndex(([t]) => t >= from)
  if (firstKept > 0) log.spend = log.spend.slice(firstKept - 1)
  else if (firstKept === -1 && log.spend.length > 1) log.spend = log.spend.slice(-1)
  log.obs = log.obs.filter(([t]) => t >= from)
}

/** A session's cost at `t`: the last point at or before it; before its first point, that point. */
function costAt(spend: readonly [number, number][], t: number): number | undefined {
  if (spend.length === 0) return undefined
  let value = spend[0]![1]
  for (const [at, usd] of spend) {
    if (at > t) break
    value = usd
  }
  return value
}

/** What every session spent from `start` to `t`. */
export function spentBetween(logs: readonly SessionLog[], start: number, t: number): number {
  let sum = 0
  for (const log of logs) {
    const a = costAt(log.spend, start)
    const b = costAt(log.spend, t)
    if (a !== undefined && b !== undefined) sum += Math.max(0, b - a)
  }
  return sum
}

export type WindowFit = {
  kind: string
  resetsAt: string
  /** USD per point: the slope of spend against the reading. */
  usdPerPct: number
  /** Points the reading rose over the anchors. */
  rise: number
  anchors: number
  r2: number
  accepted: boolean
  why?: string
}

/**
 * Fits each window of `kind`: the moments a reading first showed a new value
 * (across every session) are where it sits closest to a whole point, and the
 * spend up to each of them against the value is a line whose slope is the
 * price of one point.
 */
export function fitWindows(logs: readonly SessionLog[], kind: string, prior: number, since = 0): WindowFit[] {
  const length = WINDOW_MS[kind]
  if (length === undefined) return []
  const byWindow = new Map<string, [number, number][]>()
  for (const log of logs) {
    for (const [t, k, resetsAt, pct] of log.obs) {
      if (k !== kind || t < since) continue
      // The same reset reported a few seconds apart is the same window.
      const key = new Date(Math.round(Date.parse(resetsAt) / 60_000) * 60_000).toISOString()
      const list = byWindow.get(key) ?? []
      list.push([t, pct])
      byWindow.set(key, list)
    }
  }
  const fits: WindowFit[] = []
  for (const [resetsAt, readings] of byWindow) {
    readings.sort((a, b) => a[0] - b[0])
    const start = Date.parse(resetsAt) - length
    const points: [number, number][] = []
    let top = -Infinity
    for (const [t, pct] of readings) {
      if (pct > top && top !== -Infinity) points.push([pct, spentBetween(logs, start, t)])
      top = Math.max(top, pct)
    }
    const fit = fitLine(points)
    const rise = points.length === 0 ? 0 : points[points.length - 1]![0] - points[0]![0]
    const base = { kind, resetsAt, rise, anchors: points.length }
    if (fit === undefined || rise < (MIN_RISE[kind] ?? 1)) {
      fits.push({ ...base, usdPerPct: fit?.slope ?? 0, r2: fit?.r2 ?? 0, accepted: false, why: 'мало пунктов' })
      continue
    }
    let why: string | undefined
    if (fit.r2 < MIN_R2) why = 'разброс'
    else if (fit.slope > prior * SANE_RATIO || fit.slope < prior / SANE_RATIO) why = 'далеко от конфига'
    fits.push({ ...base, usdPerPct: fit.slope, r2: fit.r2, accepted: why === undefined, why })
  }
  return fits.sort((a, b) => a.resetsAt.localeCompare(b.resetsAt))
}

/** Least squares of spend on the reading: slope and R². */
export function fitLine(points: readonly [number, number][]): { slope: number; r2: number } | undefined {
  if (points.length < 2) return undefined
  const n = points.length
  const mx = points.reduce((s, p) => s + p[0], 0) / n
  const my = points.reduce((s, p) => s + p[1], 0) / n
  let sxx = 0
  let sxy = 0
  let syy = 0
  for (const [x, y] of points) {
    sxx += (x - mx) ** 2
    sxy += (x - mx) * (y - my)
    syy += (y - my) ** 2
  }
  if (sxx === 0) return undefined
  const slope = sxy / sxx
  const r2 = syy === 0 ? 1 : (sxy * sxy) / (sxx * syy)
  return { slope, r2 }
}

export type Calibration = { usdPerPct: number; windows: number; isLearned: boolean }

/** The median of the latest accepted windows, blended with the configured price while they are few. */
export function calibrate(fits: readonly WindowFit[], prior: number): Calibration {
  const slopes = fits.filter((f) => f.accepted).slice(-RECENT_WINDOWS).map((f) => f.usdPerPct)
  if (slopes.length === 0) return { usdPerPct: prior, windows: 0, isLearned: false }
  const sorted = [...slopes].sort((a, b) => a - b)
  const mid = sorted.length / 2
  const median = sorted.length % 2 === 1 ? sorted[Math.floor(mid)]! : (sorted[mid - 1]! + sorted[mid]!) / 2
  if (slopes.length >= TRUSTED_WINDOWS) return { usdPerPct: median, windows: slopes.length, isLearned: true }
  const w = slopes.length / TRUSTED_WINDOWS
  return { usdPerPct: median * w + prior * (1 - w), windows: slopes.length, isLearned: false }
}

/** Token counts summed. */
export function addUsage(a: ModelUsage, b: ModelUsage): ModelUsage {
  return {
    input_tokens: a.input_tokens + b.input_tokens,
    output_tokens: a.output_tokens + b.output_tokens,
    cache_read_input_tokens: a.cache_read_input_tokens + b.cache_read_input_tokens,
    cache_creation_input_tokens: a.cache_creation_input_tokens + b.cache_creation_input_tokens,
  }
}

export const ZERO_USAGE: ModelUsage = {
  input_tokens: 0,
  output_tokens: 0,
  cache_read_input_tokens: 0,
  cache_creation_input_tokens: 0,
}

/** What one turn came to, as the line shows it. */
export type TurnFigures = {
  context?: { percent?: number; tokens?: number }
  /** The main loop's tokens. */
  usage?: ModelUsage
  /** Subagents that finished during the turn. */
  agents: { count: number; usage: ModelUsage }
  usd?: number
  pct5h?: Calibration
  pctWeek?: Calibration
  durationMs: number
  model?: string
  steps: number
  /** When the main loop's prompt cache lapses, and whether on the short (5-minute) lifetime. */
  cache?: { until: number; isShort: boolean }
}

/** Which parts the line shows: each a `userConfig` switch. */
export type Show = {
  context: boolean
  tokens: boolean
  input: boolean
  subagents: boolean
  limits: boolean
  usd: boolean
  duration: boolean
  model: boolean
  steps: boolean
  cache: boolean
}

export function formatTokens(n: number): string {
  if (n < 1000) return String(n)
  if (n < 100_000) return `${(n / 1000).toFixed(1).replace(/\.0$/, '')}k`
  if (n < 1_000_000) return `${Math.round(n / 1000)}k`
  return `${(n / 1_000_000).toFixed(1).replace(/\.0$/, '')}M`
}

/** A share of a window: one decimal for five hours, two for the week. */
export function formatShare(pct: number, decimals: number): string {
  const floor = 10 ** -decimals
  if (pct > 0 && pct < floor) return `<${floor}%`
  return `${pct.toFixed(decimals)}%`
}

// Escaped, so a line with a dollar amount never reads as inline math.
export const formatUsd = (n: number) => (n < 0.005 ? '<\\$0.01' : `\\$${n.toFixed(2)}`)

export function formatDuration(ms: number): string {
  const s = Math.round(ms / 1000)
  if (s < 60) return `${s}с`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m}м ${s % 60}с`
  return `${Math.floor(m / 60)}ч ${m % 60}м`
}

/** The prompt cache's lifetime: an hour on a subscription, five minutes once a window is used up and turns run on extra usage. */
export const CACHE_LONG_MS = 60 * 60 * 1000
export const CACHE_SHORT_MS = 5 * 60 * 1000

/** Local wall-clock time, `17:42`. */
export function formatClock(ms: number): string {
  const d = new Date(ms)
  return `${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}

/** `claude-opus-5-5` → `opus-5-5`. */
export const shortModel = (id: string) => id.replace(/^claude-/, '').replace(/-\d{8}$/, '')

function tokenParts(u: ModelUsage, input: boolean): string[] {
  const parts = [`вывод ${formatTokens(u.output_tokens)}`, `запись ${formatTokens(u.cache_creation_input_tokens)}`, `чтение ${formatTokens(u.cache_read_input_tokens)}`]
  if (input) parts.push(`вход ${formatTokens(u.input_tokens)}`)
  return parts
}

/**
 * The note under a reply, up to three rows joined by newlines: the context and
 * what the turn cost; its tokens; its time, requests and model. Groups within a
 * row are split by a bar, and a row with nothing to show is left out.
 */
export function formatLine(f: TurnFigures, show: Show): string | undefined {
  const cost: string[] = []
  if (show.context && f.context?.percent !== undefined) {
    cost.push(f.context.tokens === undefined ? `ctx ${f.context.percent}%` : `ctx ${f.context.percent}% · ${formatTokens(f.context.tokens)}`)
  }
  if (show.limits && f.usd !== undefined && (f.pct5h !== undefined || f.pctWeek !== undefined)) {
    const shares: string[] = []
    if (f.pct5h !== undefined) shares.push(`${formatShare(f.usd / f.pct5h.usdPerPct, 1)} 5ч`)
    if (f.pctWeek !== undefined) shares.push(`${formatShare(f.usd / f.pctWeek.usdPerPct, 2)} нед`)
    const learned = (f.pct5h?.isLearned ?? true) && (f.pctWeek?.isLearned ?? true)
    cost.push(`${learned ? '' : '≈'}${shares.join(' · ')}`)
  }
  if (show.usd && f.usd !== undefined) cost.push(formatUsd(f.usd))
  const tokens: string[] = []
  if (show.tokens && f.usage !== undefined) tokens.push(`ход: ${tokenParts(f.usage, show.input).join(' · ')}`)
  if (show.subagents && f.agents.count > 0) {
    tokens.push(`субагенты (${f.agents.count}): ${tokenParts(f.agents.usage, show.input).join(' · ')}`)
  }
  const extra: string[] = []
  if (show.duration) extra.push(formatDuration(f.durationMs))
  if (show.steps && f.steps > 0) extra.push(`запросов ${f.steps}`)
  if (show.model && f.model !== undefined) extra.push(shortModel(f.model))
  if (show.cache && f.cache !== undefined) extra.push(`кэш до ${formatClock(f.cache.until)}${f.cache.isShort ? ' (5м)' : ''}`)
  const rows = [cost.join(' │ '), tokens.join(' │ '), extra.join(' · ')].filter((row) => row !== '')
  return rows.length === 0 ? undefined : rows.join('\n')
}
