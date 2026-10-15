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

/** One session's journal: its total cost over time, each change of a window's reading, and what each model request added. */
export type SessionLog = {
  session: string
  /** `[time, session cost in USD]`, written when the cost grows. */
  spend: [number, number][]
  /** `[time, kind, resetsAt, percentUsed]`, written when a reading changes. */
  obs: [number, string, string, number][]
  /** `[time, model, USD]`: what the ledger grew by since the last request, put on the model that answered this one (short id). Absent in older journals. */
  bymodel?: [number, string, number][]
  /** Each main-loop turn: see TurnRow. Absent in older journals. */
  turns?: TurnRow[]
}

/**
 * `[time, USD, share of 5h, share of week, 5h reading at start, at end, model]`:
 * a main-loop turn's dollars and the shares the line priced them at, beside the
 * two readings of the five-hour window whose difference Turn Usage shows. A
 * reading or a model the session did not have is `null`.
 */
export type TurnRow = [number, number, number, number, number | null, number | null, string | null]

export const emptyLog = (session: string): SessionLog => ({ session, spend: [], obs: [], bymodel: [], turns: [] })

export function addTurn(log: SessionLog, row: TurnRow): void {
  ;(log.turns ??= []).push(row)
}

/** Adds what one request's model added to the ledger. */
export function addByModel(log: SessionLog, t: number, model: string, usd: number): void {
  if (!(usd > 0)) return
  ;(log.bymodel ??= []).push([t, model, usd])
}

/** Adds a cost point when the total moved. */
export function addSpend(log: SessionLog, t: number, usd: number | undefined): void {
  if (usd === undefined) return
  const last = log.spend[log.spend.length - 1]
  if (last !== undefined && last[1] === usd) return
  log.spend.push([t, usd])
}

/**
 * Adds a reading of each window whose value or reset moved since this session
 * last saw it. Every kind is kept, not only the two the line prices: whether
 * the API sends per-model weekly windows is still to be seen. A window with no
 * reset time is kept with an empty one.
 */
export function addReadings(log: SessionLog, t: number, limits: readonly SessionRateLimit[]): void {
  for (const w of limits) {
    const resetsAt = w.resetsAt ?? ''
    const last = log.obs.findLast((o) => o[1] === w.kind)
    if (last !== undefined && last[2] === resetsAt && last[3] === w.percentUsed) continue
    log.obs.push([t, w.kind, resetsAt, w.percentUsed])
  }
}

/** Every kind of window the journals saw, with its latest reading: `[time, resetsAt, percentUsed]`. */
export function windowKinds(logs: readonly SessionLog[]): Map<string, [number, string, number]> {
  const kinds = new Map<string, [number, string, number]>()
  for (const log of logs) {
    for (const [t, kind, resetsAt, pct] of log.obs) {
      const seen = kinds.get(kind)
      if (seen === undefined || seen[0] < t) kinds.set(kind, [t, resetsAt, pct])
    }
  }
  return kinds
}

export function prune(log: SessionLog, now: number): void {
  const from = now - KEEP_MS
  // Keep the last cost point before the cut: it is the baseline of a window that began before it.
  const firstKept = log.spend.findIndex(([t]) => t >= from)
  if (firstKept > 0) log.spend = log.spend.slice(firstKept - 1)
  else if (firstKept === -1 && log.spend.length > 1) log.spend = log.spend.slice(-1)
  log.obs = log.obs.filter(([t]) => t >= from)
  if (log.bymodel !== undefined) log.bymodel = log.bymodel.filter(([t]) => t >= from)
  if (log.turns !== undefined) log.turns = log.turns.filter(([t]) => t >= from)
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

/** What each model's requests added after `start` up to `t`, over every journal that records models. */
export function spentByModel(logs: readonly SessionLog[], start: number, t: number): Map<string, number> {
  const by = new Map<string, number>()
  for (const log of logs) {
    for (const [at, model, usd] of log.bymodel ?? []) {
      if (at > start && at <= t) by.set(model, (by.get(model) ?? 0) + usd)
    }
  }
  return by
}

export type WindowFit = {
  kind: string
  resetsAt: string
  /** USD by model over the anchors' span, from the journals that record models. */
  models?: Record<string, number>
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
      if (k !== kind || t < since || resetsAt === '') continue
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
    const times: number[] = []
    let top = -Infinity
    for (const [t, pct] of readings) {
      if (pct > top && top !== -Infinity) {
        points.push([pct, spentBetween(logs, start, t)])
        times.push(t)
      }
      top = Math.max(top, pct)
    }
    const fit = fitLine(points)
    const rise = points.length === 0 ? 0 : points[points.length - 1]![0] - points[0]![0]
    const by = times.length < 2 ? new Map<string, number>() : spentByModel(logs, times[0]!, times[times.length - 1]!)
    const base = { kind, resetsAt, rise, anchors: points.length, ...(by.size === 0 ? {} : { models: Object.fromEntries(by) }) }
    if (fit === undefined || rise < (MIN_RISE[kind] ?? 1)) {
      fits.push({ ...base, usdPerPct: fit?.slope ?? 0, r2: fit?.r2 ?? 0, accepted: false, why: 'мало пунктов' })
      continue
    }
    const why = judge(fit.slope, fit.r2, prior, SANE_RATIO)
    fits.push({ ...base, usdPerPct: fit.slope, r2: fit.r2, accepted: why === undefined, why })
  }
  return fits.sort((a, b) => a.resetsAt.localeCompare(b.resetsAt))
}

/** Why a fitted window does not count, or nothing when it does. */
function judge(slope: number, r2: number, prior: number, ratio: number): string | undefined {
  if (r2 < MIN_R2) return 'разброс'
  if (slope > prior * ratio || slope < prior / ratio) return 'далеко от конфига'
  return undefined
}

/** A window counts toward a model when this share of its spend went to that model. */
export const MODEL_SHARE = 0.9
/** Without a measured price of its own, a model's windows are held this close to the shared price: Haiku may sit far from Opus. */
const MODEL_RATIO = 3

/**
 * The windows where one model spent nine tenths or more, judged again against
 * that model's own measured price when there is one (2×), else the shared one (3×).
 */
export function modelFits(fits: readonly WindowFit[], model: string, prior: number, isMeasured: boolean): WindowFit[] {
  const out: WindowFit[] = []
  for (const f of fits) {
    const total = Object.values(f.models ?? {}).reduce((a, b) => a + b, 0)
    if (total <= 0 || (f.models?.[model] ?? 0) / total < MODEL_SHARE) continue
    if (f.why === 'мало пунктов') {
      out.push(f)
      continue
    }
    const why = judge(f.usdPerPct, f.r2, prior, isMeasured ? SANE_RATIO : MODEL_RATIO)
    out.push({ ...f, accepted: why === undefined, why })
  }
  return out
}

/** `claude-opus-5-5` (or `opus-5-5`) → `opus`, `[5, 5]`; a date suffix is dropped. */
export function parseModel(id: string): { family: string; version: number[] } | undefined {
  const m = /^([a-z]+)((?:-\d+)*)$/.exec(shortModel(id.trim().toLowerCase()))
  if (m === null) return undefined
  return { family: m[1]!, version: m[2]!.split('-').filter((p) => p !== '').map(Number) }
}

function isNewer(a: readonly number[], b: readonly number[]): boolean {
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const d = (a[i] ?? 0) - (b[i] ?? 0)
    if (d !== 0) return d > 0
  }
  return false
}

/** The newest version of `family` among `seen`, in the short form. */
export function newestOf(family: string, seen: Iterable<string>): string | undefined {
  let best: { id: string; version: number[] } | undefined
  for (const id of seen) {
    const p = parseModel(id)
    if (p === undefined || p.family !== family || p.version.length === 0) continue
    if (best === undefined || isNewer(p.version, best.version)) best = { id: shortModel(id), version: p.version }
  }
  return best?.id
}

/**
 * The models a comma list names, in the short form: a versioned name
 * (`opus-5`) is that version; a family (`opus`) is its newest version seen, or
 * the family itself while none is.
 */
export function resolveTracked(spec: string, seen: Iterable<string>): string[] {
  const ids = [...seen]
  const out: string[] = []
  for (const token of spec.split(/[,\s]+/).map((t) => shortModel(t.trim().toLowerCase())).filter((t) => t !== '')) {
    const p = parseModel(token)
    const id = p !== undefined && p.version.length === 0 ? (newestOf(p.family, ids) ?? token) : token
    if (!out.includes(id)) out.push(id)
  }
  return out
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

/** A model's price as limit-test measured it, the latest sound one in each window. */
export type Reference = { usdPerPct5h?: number; usdPerPctWeek?: number; at: number }

/** Points a limit-test result must span to be taken as a reference: all of its counted moves. */
const REFERENCE_POINTS = { five_hour: 3, seven_day: 2 }

/**
 * The references in limit-test's `data/results.json` (A39): per model, the
 * latest entry in each window with a price, not marked noisy and spanning its
 * counted moves. Anything that does not read as that file is no reference.
 */
export function referencesOf(results: unknown): Record<string, Reference> {
  const refs: Record<string, Reference> = {}
  if (!Array.isArray(results)) return refs
  for (const r of results as Record<string, unknown>[]) {
    if (r === null || typeof r !== 'object' || typeof r.model !== 'string' || typeof r.at !== 'number' || r.isNoisy === true) continue
    const take = (price: unknown, points: unknown, need: number) => (typeof price === 'number' && price > 0 && typeof points === 'number' && points >= need ? price : undefined)
    const five = take(r.usdPerPct5h, r.points5h, REFERENCE_POINTS.five_hour)
    const week = take(r.usdPerPctWeek, r.pointsWeek, REFERENCE_POINTS.seven_day)
    if (five === undefined && week === undefined) continue
    const ref = refs[r.model]
    const isLater = ref === undefined || ref.at <= r.at
    refs[r.model] = {
      usdPerPct5h: five !== undefined && (isLater || ref?.usdPerPct5h === undefined) ? five : ref?.usdPerPct5h,
      usdPerPctWeek: week !== undefined && (isLater || ref?.usdPerPctWeek === undefined) ? week : ref?.usdPerPctWeek,
      at: Math.max(ref?.at ?? 0, r.at),
    }
  }
  return refs
}

export type Rates = { five_hour: Calibration; seven_day: Calibration }

/** One model's price of a point in each window, and whether it is the model's own or the shared one. */
export type ModelRate = Rates & { hasOwn: { five_hour: boolean; seven_day: boolean } }

/**
 * A model's price: its own windows' median, started from its measured price
 * when there is one, else from the shared price. A measured price with no
 * windows yet stands alone.
 */
export function modelRate(fits: { five_hour: readonly WindowFit[]; seven_day: readonly WindowFit[] }, model: string, shared: Rates, ref?: Reference): ModelRate {
  const one = (kind: 'five_hour' | 'seven_day', measured: number | undefined): [Calibration, boolean] => {
    const prior = measured ?? shared[kind].usdPerPct
    const cal = calibrate(modelFits(fits[kind], model, prior, measured !== undefined), prior)
    if (cal.windows === 0 && measured !== undefined) return [{ ...cal, isLearned: true }, true]
    return [cal, cal.windows > 0]
  }
  const [five, ownFive] = one('five_hour', ref?.usdPerPct5h)
  const [week, ownWeek] = one('seven_day', ref?.usdPerPctWeek)
  return { five_hour: five, seven_day: week, hasOwn: { five_hour: ownFive, seven_day: ownWeek } }
}

/**
 * A turn's shares priced model by model: each model's dollars over its own
 * price where it has one, the rest (and dollars no request claimed) over the
 * shared price. Nothing when no request of the turn recorded its model.
 */
export function turnShares(byModel: ReadonlyMap<string, number>, usd: number, rates: ReadonlyMap<string, ModelRate>, shared: Rates): TurnFigures['shares'] {
  if (byModel.size === 0) return undefined
  let five = 0
  let week = 0
  let claimed = 0
  let isLearned = true
  for (const [model, spent] of byModel) {
    const r = rates.get(model)
    const p5 = r?.hasOwn.five_hour === true ? r.five_hour : shared.five_hour
    const pw = r?.hasOwn.seven_day === true ? r.seven_day : shared.seven_day
    five += spent / p5.usdPerPct
    week += spent / pw.usdPerPct
    isLearned &&= p5.isLearned && pw.isLearned
    claimed += spent
  }
  const rest = Math.max(0, usd - claimed)
  five += rest / shared.five_hour.usdPerPct
  week += rest / shared.seven_day.usdPerPct
  if (rest > 0) isLearned &&= shared.five_hour.isLearned && shared.seven_day.isLearned
  return { five_hour: five, seven_day: week, isLearned }
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
  /** The shares when the turn was priced model by model; else `usd` over the shared prices. */
  shares?: { five_hour: number; seven_day: number; isLearned: boolean }
  /** Each window's whole size in USD, for `showLimitSize`. */
  limitSize?: { five_hour: number; seven_day: number }
  durationMs: number
  model?: string
  steps: number
  /** A window is used up: the turn ran on extra usage, paid in money rather than in shares. */
  isExtraUsage?: boolean
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
  limitSize: boolean
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

/** `5ч ≈ $39 · нед ≈ $285`: a window's whole size, a hundred points; escaped for a reply's markdown. */
export function formatLimitSize(size: { five_hour: number; seven_day: number }, isEscaped: boolean): string {
  const d = isEscaped ? '\\$' : '$'
  return `5ч ≈ ${d}${Math.round(size.five_hour)} · нед ≈ ${d}${Math.round(size.seven_day)}`
}

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

/** In the order a request runs: the prompt from its cached head to its uncached tail, then the reply. */
function tokenParts(u: ModelUsage, input: boolean): string {
  const prompt = [`чтение ${formatTokens(u.cache_read_input_tokens)}`, `запись ${formatTokens(u.cache_creation_input_tokens)}`]
  if (input) prompt.push(`вход ${formatTokens(u.input_tokens)}`)
  return `${prompt.join(' · ')} → вывод ${formatTokens(u.output_tokens)}`
}

/**
 * The note under a reply, up to four rows joined by newlines: the context and
 * what the turn cost; its tokens; its time, requests and model; the windows'
 * size, when asked for. Groups within a row are split by a bar, and a row with
 * nothing to show is left out.
 */
export function formatLine(f: TurnFigures, show: Show): string | undefined {
  const cost: string[] = []
  if (show.context && f.context?.percent !== undefined) {
    cost.push(f.context.tokens === undefined ? `ctx ${f.context.percent}%` : `ctx ${f.context.percent}% · ${formatTokens(f.context.tokens)}`)
  }
  if (show.limits && f.isExtraUsage === true) {
    cost.push('**сверх лимита**')
  } else if (show.limits && f.usd !== undefined && (f.pct5h !== undefined || f.pctWeek !== undefined)) {
    const shares: string[] = []
    if (f.pct5h !== undefined) shares.push(`${formatShare(f.shares?.five_hour ?? f.usd / f.pct5h.usdPerPct, 1)} 5ч`)
    if (f.pctWeek !== undefined) shares.push(`${formatShare(f.shares?.seven_day ?? f.usd / f.pctWeek.usdPerPct, 2)} нед`)
    const learned = f.shares?.isLearned ?? ((f.pct5h?.isLearned ?? true) && (f.pctWeek?.isLearned ?? true))
    cost.push(`${learned ? '' : '≈'}${shares.join(' · ')}`)
  }
  if (show.usd && f.usd !== undefined) cost.push(formatUsd(f.usd))
  const tokens: string[] = []
  if (show.tokens && f.usage !== undefined) tokens.push(`ход: ${tokenParts(f.usage, show.input)}`)
  if (show.subagents && f.agents.count > 0) {
    tokens.push(`субагенты (${f.agents.count}): ${tokenParts(f.agents.usage, show.input)}`)
  }
  const extra: string[] = []
  if (show.duration) extra.push(formatDuration(f.durationMs))
  if (show.steps && f.steps > 0) extra.push(`запросов ${f.steps}`)
  if (show.model && f.model !== undefined) extra.push(shortModel(f.model))
  if (show.cache && f.cache !== undefined) extra.push(`кэш до ${formatClock(f.cache.until)}${f.cache.isShort ? ' (5м)' : ''}`)
  const size = show.limitSize && f.limitSize !== undefined ? formatLimitSize(f.limitSize, true) : ''
  const rows = [cost.join(' │ '), tokens.join(' │ '), extra.join(' · '), size].filter((row) => row !== '')
  return rows.length === 0 ? undefined : rows.join('\n')
}

/** A markdown table, the first row its head: command answers are drawn as markdown. */
export function markdownTable(rows: readonly (readonly string[])[]): string {
  const line = (r: readonly string[]) => `| ${r.join(' | ')} |`
  const [head, ...body] = rows
  if (head === undefined) return ''
  return [line(head), line(head.map(() => '---')), ...body.map(line)].join('\n')
}

/** The span `/turn-meter` compares turns over. */
export const DAY_MS = 24 * 60 * 60 * 1000

/** turn-meter's shares of the five-hour window against Turn Usage's way and against the window itself, over a span. */
export type TurnComparison = {
  turns: number
  /** turn-meter's shares summed, in points. */
  meter: number
  /** Turns with both readings and no reset between them: what Turn Usage could show. */
  stepped: { turns: number; meter: number; points: number }
  /** Turns whose reading fell: the window reset during them. */
  resets: number
  /** Turns missing a reading. */
  unread: number
  /** What the five-hour window rose by over the span; nothing when no reading fell in it. */
  rise?: number
  /** Per stepped turn, turn-meter's share minus Turn Usage's step, in points. */
  diff?: { median: number; p25: number; p75: number }
}

/** The `q`-quantile of sorted numbers, interpolated. */
function quantile(sorted: readonly number[], q: number): number {
  const at = (sorted.length - 1) * q
  const lo = Math.floor(at)
  const hi = Math.ceil(at)
  return sorted[lo]! + (sorted[hi]! - sorted[lo]!) * (at - lo)
}

/**
 * What a window of `kind` rose by from `from` to `to`, summed over every window
 * the readings show: from the last reading before `from` (or the first one
 * after it), or from zero for a window that began inside the span, to the
 * highest reading in the span.
 */
export function windowRise(logs: readonly SessionLog[], kind: string, from: number, to: number): number | undefined {
  const length = WINDOW_MS[kind]
  if (length === undefined) return undefined
  const byWindow = new Map<string, [number, number][]>()
  for (const log of logs) {
    for (const [t, k, resetsAt, pct] of log.obs) {
      if (k !== kind || resetsAt === '' || t > to) continue
      const key = new Date(Math.round(Date.parse(resetsAt) / 60_000) * 60_000).toISOString()
      byWindow.set(key, [...(byWindow.get(key) ?? []), [t, pct]])
    }
  }
  let rise: number | undefined
  for (const [resetsAt, readings] of byWindow) {
    const inside = readings.filter(([t]) => t >= from).map(([, pct]) => pct)
    if (inside.length === 0) continue
    readings.sort((a, b) => a[0] - b[0])
    const before = readings.findLast(([t]) => t < from)
    const base = Date.parse(resetsAt) - length >= from ? 0 : (before?.[1] ?? readings.find(([t]) => t >= from)![1])
    rise = (rise ?? 0) + Math.max(0, Math.max(...inside) - base)
  }
  return rise
}

/** The main-loop turns of every journal from `now - span` to `now`, compared. */
export function compareTurns(logs: readonly SessionLog[], now: number, span = DAY_MS): TurnComparison {
  const from = now - span
  const c: TurnComparison = { turns: 0, meter: 0, stepped: { turns: 0, meter: 0, points: 0 }, resets: 0, unread: 0 }
  const diffs: number[] = []
  for (const log of logs) {
    for (const [t, , share5h, , start, end] of log.turns ?? []) {
      if (t < from || t > now) continue
      c.turns += 1
      c.meter += share5h
      if (start === null || end === null) c.unread += 1
      else if (end < start) c.resets += 1
      else {
        c.stepped.turns += 1
        c.stepped.meter += share5h
        c.stepped.points += end - start
        diffs.push(share5h - (end - start))
      }
    }
  }
  const rise = windowRise(logs, 'five_hour', from, now)
  if (rise !== undefined) c.rise = rise
  if (diffs.length > 0) {
    const sorted = diffs.sort((a, b) => a - b)
    c.diff = { median: quantile(sorted, 0.5), p25: quantile(sorted, 0.25), p75: quantile(sorted, 0.75) }
  }
  return c
}

const signed = (n: number, digits: number) => `${n > 0 ? '+' : n < 0 ? '−' : ''}${Math.abs(n).toFixed(digits)}`

/** How far `a` is off `b`, in percent of `b`: `−12 %`; nothing when `b` is zero. */
const offBy = (a: number, b: number) => (b > 0 ? ` → turn-meter ${signed((a / b - 1) * 100, 0)} %` : '')

/** The comparison as `/turn-meter` prints it: a paragraph and a list, in markdown. */
export function formatComparison(c: TurnComparison): string {
  const head = 'сравнение с Turn Usage за сутки, окно 5ч'
  if (c.turns === 0) return `${head}: ходов ещё не записано`
  const skipped = [c.resets > 0 ? `через сброс окна ${c.resets}` : '', c.unread > 0 ? `без показаний ${c.unread}` : ''].filter((s) => s !== '')
  const lines = [
    `${head} (ходов ${c.turns} во всех журналах):`,
    `- turn-meter: Σ ${c.meter.toFixed(1)} %`,
    `- разность целых показаний, как у Turn Usage: Σ +${c.stepped.points} п. за ${c.stepped.turns} ход., turn-meter на них Σ ${c.stepped.meter.toFixed(1)} %${offBy(c.stepped.meter, c.stepped.points)}${skipped.length === 0 ? '' : ` (не в счёт: ${skipped.join(', ')})`}`,
    `- прирост окна: ${c.rise === undefined ? 'показаний нет' : `+${c.rise} п.${offBy(c.meter, c.rise)}`}`,
  ]
  if (c.diff !== undefined) {
    lines.push(`- на ход, turn-meter минус Turn Usage: медиана ${signed(c.diff.median, 2)} п., половина ходов от ${signed(c.diff.p25, 2)} до ${signed(c.diff.p75, 2)}`)
  }
  return lines.join('\n')
}
