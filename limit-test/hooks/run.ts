// Pure parts of limit-test: measuring one model's price of a point by spending
// on it alone through subagents and watching the windows move (A39).

import type { SessionRateLimit } from 'claude-code'

/** Models measured when the command names none: the engine's aliases, each its family's newest. */
export const DEFAULT_MODELS = ['opus', 'sonnet', 'haiku']
/** Moves of the reading counted after the first one, which only marks where a point begins. */
export const SHIFTS = { five_hour: 3, seven_day: 2 }
/** Each model stops past this many times what it was expected to cost. No time limit: spend, not time, uses the window. */
export const CAP_RATIO = 3
/** The run stops when the five-hour window reaches this. */
export const STOP_AT = 90
/** It does not start above this, nor when what it would spend leaves less room than `ROOM`. */
export const REFUSE_ABOVE = 80
export const ROOM = 95
/** A result where others spent more than this share of the money is marked noisy and not kept. */
export const NOISY = 0.25
/** Each request is sized to cost about this much of a five-hour point, between the token bounds. */
export const TARGET_POINT = 0.25
export const FIRST_TOKENS = 2000
export const MIN_TOKENS = 1000
export const MAX_TOKENS = 32_000
/** Requests in a row that come back with nothing before a model is given up. */
export const MAX_ERRORS = 3
/**
 * The price of a point when nothing better is known: turn-meter's defaults.
 * Also what others' dollars are turned into points at, since turn-meter keeps
 * its learned shared price nowhere a neighbour could read it.
 */
export const START_PRICE = { five_hour: 0.39, seven_day: 2.85 }

export type Kind = 'five_hour' | 'seven_day'

/** One model in the queue: the name it was asked by, the id it is known by and called by, the prices its run is planned by. */
export type ModelPlan = {
  /** As the command named it, lowercased: `opus`, `opus-5`. */
  spec: string
  /** The short id (`opus-5-5`); absent while a family was never seen, until the spawn names it. */
  id?: string
  /** What the spawn names: `claude-<id>` when known, else the alias. */
  callModel: string
  usdPerPct: { five_hour: number; seven_day: number }
}

/** `claude-opus-5-5` → `opus-5-5`; a date suffix is dropped. */
export const shortModel = (id: string) => id.replace(/^claude-/, '').replace(/-\d{8}$/, '')

/** `opus-5-5` → `opus`, `[5, 5]`. */
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

/** The command's words: models (none → the three aliases) and `--week`. */
export function parseArgs(args: string): { specs: string[]; isWeek: boolean } {
  const words = args.trim().split(/\s+/).filter((w) => w !== '')
  const specs: string[] = []
  for (const w of words) {
    if (w === '--week') continue
    const spec = shortModel(w.toLowerCase())
    if (!specs.includes(spec)) specs.push(spec)
  }
  return { specs: specs.length === 0 ? [...DEFAULT_MODELS] : specs, isWeek: words.includes('--week') }
}

/**
 * The plan of one named model: a versioned name is exactly it; a family is its
 * newest version the journals saw, or the alias while none is. Its expected
 * price is its latest reference, else the start price.
 */
export function planOf(spec: string, seen: Iterable<string>, refs: Readonly<Record<string, Reference>>): ModelPlan {
  const p = parseModel(spec)
  const id = p !== undefined && p.version.length === 0 ? newestOf(p.family, seen) : spec
  const ref = id === undefined ? undefined : refs[id]
  return {
    spec,
    ...(id === undefined ? {} : { id }),
    callModel: id === undefined ? spec : `claude-${id}`,
    usdPerPct: { five_hour: ref?.usdPerPct5h ?? START_PRICE.five_hour, seven_day: ref?.usdPerPctWeek ?? START_PRICE.seven_day },
  }
}

/** What a model's run is expected to spend: a point to reach the first move and the counted ones after it. */
export function expectedUsd(plan: ModelPlan, isWeek: boolean): number {
  const five = (1 + SHIFTS.five_hour) * plan.usdPerPct.five_hour
  const week = isWeek ? (1 + SHIFTS.seven_day) * plan.usdPerPct.seven_day : 0
  return Math.max(five, week)
}

/** The same in points of the five-hour window. */
export const expectedPoints = (plan: ModelPlan, isWeek: boolean) => expectedUsd(plan, isWeek) / plan.usdPerPct.five_hour

/** Why the run must not start, or nothing. */
export function refusal(limits: readonly SessionRateLimit[], isWeek: boolean, plannedPoints5h: number): string | undefined {
  const five = limits.find((w) => w.kind === 'five_hour')
  const week = limits.find((w) => w.kind === 'seven_day')
  if (five === undefined) return 'нет показаний окна 5ч — замер возможен только на подписке, после первого ответа модели'
  if (isWeek && week === undefined) return 'нет показаний недельного окна'
  const windows = [[five, '5ч'], [isWeek ? week : undefined, 'неделя']] as const
  for (const [w, label] of windows) {
    if (w !== undefined && w.percentUsed >= 100) return `окно ${label} уже ${w.percentUsed} % — сверх лимита, замер не запускаю`
  }
  if (five.percentUsed > REFUSE_ABOVE) return `окно 5ч уже ${five.percentUsed} % (больше ${REFUSE_ABOVE} %) — замер не запускаю`
  if (five.percentUsed + plannedPoints5h > ROOM) {
    return `окно 5ч ${five.percentUsed} %, а замер займёт ещё ~${Math.ceil(plannedPoints5h)} п. — выйдет за ${ROOM} %; назовите меньше моделей или дождитесь сброса`
  }
  return undefined
}

/** The next request's length: sized from what the last one cost per output token. */
export function nextTokens(lastUsd: number, lastOutput: number, usdPerPct5h: number, current: number): number {
  if (!(lastUsd > 0) || lastOutput <= 0) return current
  const want = Math.round((TARGET_POINT * usdPerPct5h) / (lastUsd / lastOutput))
  return Math.min(MAX_TOKENS, Math.max(MIN_TOKENS, want))
}

/** The request itself: long enough to come to about `tokens` of output, about nothing that needs thought. */
export function testPrompt(tokens: number): string {
  const items = Math.max(20, Math.round(tokens / 25))
  return `Write a numbered list of ${items} distinct one-sentence facts about the history of mathematics. Start at once with "1." and write nothing but the list.`
}

/** The system prompt of the mod's own agent type: as short as an agent can be. */
export const WRITER_PROMPT = 'You write exactly the text you are asked for, in full, and nothing else. You have no tools.'

/** A moment the reading rose: when, to what, what the test had spent by then, and the session's whole ledger then. */
export type Shift = { t: number; pct: number; spent: number; ledger: number }

export type Price = {
  /** USD per point; absent when no points are left after others' spend. */
  usdPerPct?: number
  points: number
  testUsd: number
  othersUsd: number
  isNoisy: boolean
}

/**
 * The price between the first and the last move: the test's dollars over the
 * points it moved, less the points others' dollars would have moved at the
 * shared price. Others are the other sessions' journals (`journalUsd`) and
 * whatever else this session's ledger grew by over the span.
 */
export function priceOf(shifts: readonly Shift[], journalUsd: number, sharedPrice: number): Price | undefined {
  if (shifts.length < 2) return undefined
  const first = shifts[0]!
  const last = shifts[shifts.length - 1]!
  const points = last.pct - first.pct
  const testUsd = last.spent - first.spent
  const ownOther = Math.max(0, last.ledger - first.ledger - testUsd)
  const othersUsd = journalUsd + ownOther
  const own = points - othersUsd / sharedPrice
  const total = testUsd + othersUsd
  return {
    usdPerPct: own > 0 && testUsd > 0 ? testUsd / own : undefined,
    points,
    testUsd,
    othersUsd,
    isNoisy: total > 0 && othersUsd / total > NOISY,
  }
}

/** Where a model's run stands: what it spent, the moves seen. */
export type ModelRun = {
  plan: ModelPlan
  startedAt: number
  requests: number
  spent: number
  /** How long a list the next request asks for. */
  tokens: number
  /** Requests in a row that came back with nothing. */
  errors: number
  cap: number
  last: { five_hour?: number; seven_day?: number }
  shifts: { five_hour: Shift[]; seven_day: Shift[] }
}

export function startRun(plan: ModelPlan, now: number, isWeek: boolean, limits: readonly SessionRateLimit[]): ModelRun {
  return {
    plan,
    startedAt: now,
    requests: 0,
    spent: 0,
    tokens: FIRST_TOKENS,
    errors: 0,
    cap: CAP_RATIO * expectedUsd(plan, isWeek),
    last: { five_hour: limits.find((w) => w.kind === 'five_hour')?.percentUsed, seven_day: limits.find((w) => w.kind === 'seven_day')?.percentUsed },
    shifts: { five_hour: [], seven_day: [] },
  }
}

/** Takes one answered request into the run: its dollars and any move of the readings after it. */
export function takeAnswer(run: ModelRun, t: number, usd: number, ledger: number, limits: readonly SessionRateLimit[]): void {
  run.requests += 1
  run.errors = 0
  run.spent += Math.max(0, usd)
  for (const kind of ['five_hour', 'seven_day'] as const) {
    const pct = limits.find((w) => w.kind === kind)?.percentUsed
    if (pct === undefined) continue
    const before = run.last[kind]
    if (before !== undefined && pct > before) run.shifts[kind].push({ t, pct, spent: run.spent, ledger })
    // A drop is a reset: the moves seen so far belonged to the window that ended.
    if (before !== undefined && pct < before) run.shifts[kind] = []
    run.last[kind] = pct
  }
}

/** Whether the run has what it came for, or why it stops short; nothing to go on. */
export function verdict(run: ModelRun, now: number, isWeek: boolean): { isDone: true; note?: string } | { isDone: false } {
  const enough = (kind: Kind) => run.shifts[kind].length >= 1 + SHIFTS[kind]
  if (enough('five_hour') && (!isWeek || enough('seven_day'))) return { isDone: true }
  if ((run.last.five_hour ?? 0) >= STOP_AT) return { isDone: true, note: `окно 5ч дошло до ${STOP_AT} %` }
  if (run.spent >= run.cap) return { isDone: true, note: 'потолок трат' }
  return { isDone: false }
}

/**
 * One model's line in `data/results.json`, the file turn-meter reads its
 * references from (A39): keep the field names, add rather than rename.
 */
export type ResultEntry = {
  /** The short id measured (`opus-5-5`), or the alias when the spawn never named one. */
  model: string
  /** When the model's run ended, ms. */
  at: number
  /** When the whole run started, ms: the entries of one `/limit-test` share it. */
  run: number
  usdPerPct5h?: number
  usdPerPctWeek?: number
  points5h: number
  pointsWeek: number
  spent: number
  requests: number
  isNoisy: boolean
  /** Whether the run asked for the weekly window. */
  isWeek: boolean
  note?: string
}

export function entryOf(run: ModelRun, at: number, runAt: number, five: Price | undefined, week: Price | undefined, isWeek: boolean, note?: string): ResultEntry {
  return {
    model: run.plan.id ?? run.plan.spec,
    at,
    run: runAt,
    ...(five?.usdPerPct === undefined ? {} : { usdPerPct5h: five.usdPerPct }),
    ...(week?.usdPerPct === undefined ? {} : { usdPerPctWeek: week.usdPerPct }),
    points5h: five?.points ?? 0,
    pointsWeek: week?.points ?? 0,
    spent: run.spent,
    requests: run.requests,
    isNoisy: (five?.isNoisy ?? false) || (week?.isNoisy ?? false),
    isWeek,
    ...(note === undefined ? {} : { note }),
  }
}

/** A model's latest sound price in each window, as turn-meter takes it. */
export type Reference = { usdPerPct5h?: number; usdPerPctWeek?: number; at: number }

/** The latest entry per model and window with a price, not noisy and spanning all its counted moves. */
export function referencesOf(entries: readonly ResultEntry[]): Record<string, Reference> {
  const refs: Record<string, Reference> = {}
  for (const r of [...entries].sort((a, b) => a.at - b.at)) {
    if (r.isNoisy) continue
    const five = r.usdPerPct5h !== undefined && r.points5h >= SHIFTS.five_hour ? r.usdPerPct5h : undefined
    const week = r.usdPerPctWeek !== undefined && r.pointsWeek >= SHIFTS.seven_day ? r.usdPerPctWeek : undefined
    if (five === undefined && week === undefined) continue
    const ref = refs[r.model]
    refs[r.model] = { usdPerPct5h: five ?? ref?.usdPerPct5h, usdPerPctWeek: week ?? ref?.usdPerPctWeek, at: r.at }
  }
  return refs
}

/** The entries `results.json` holds, whatever else it holds. */
export function entriesOf(json: unknown): ResultEntry[] {
  if (!Array.isArray(json)) return []
  return json.filter((r): r is ResultEntry => r !== null && typeof r === 'object' && typeof r.model === 'string' && typeof r.at === 'number')
}

// Escaped: a command's answer is drawn as markdown, and two amounts on a line would read as math.
const usd = (n: number, digits: number) => `\\$${n.toFixed(digits)}`

/** `opus-5-5` → `opus`, `5.5`; an alias stands as itself with no version. */
function nameOf(model: string): [string, string] {
  const p = parseModel(model)
  if (p === undefined || p.version.length === 0) return [model, '—']
  return [p.family, p.version.join('.')]
}

/** The last run as `/limit-test last` answers it: a line on when, and a markdown table. */
export function formatLast(entries: readonly ResultEntry[]): string {
  if (entries.length === 0) return 'замеров ещё не было — `/limit-test` запускает'
  const runAt = Math.max(...entries.map((e) => e.run ?? e.at))
  const last = entries.filter((e) => (e.run ?? e.at) === runAt)
  const isWeek = last.some((e) => e.isWeek)
  const head = ['модель', 'версия', '1 % 5ч', 'окно 5ч', '1 % нед', 'точность', 'потрачено']
  const rows = last.map((e) => {
    const accuracy = [`${e.points5h} п. 5ч`, ...(isWeek ? [`${e.pointsWeek} п. нед`] : []), ...(e.isNoisy ? ['шумно'] : []), ...(e.note === undefined ? [] : [e.note])]
    return [
      ...nameOf(e.model),
      e.usdPerPct5h === undefined ? 'мало данных' : usd(e.usdPerPct5h, 3),
      e.usdPerPct5h === undefined ? '—' : usd(e.usdPerPct5h * 100, 0),
      !isWeek ? '—' : e.usdPerPctWeek === undefined ? 'мало данных' : usd(e.usdPerPctWeek, 3),
      accuracy.join(' · '),
      usd(e.spent, 2),
    ]
  })
  const line = (r: readonly string[]) => `| ${r.join(' | ')} |`
  const when = new Date(runAt).toISOString().slice(0, 16).replace('T', ' ')
  return [`замер от ${when} UTC${isWeek ? ', с неделей' : ''}`, '', line(head), line(head.map(() => '---')), ...rows.map(line)].join('\n')
}

/** A session's cost at `t`: the last point at or before it; before its first point, that point. */
function costAt(spend: readonly [number, number][], t: number): number | undefined {
  if (spend.length === 0) return undefined
  let value = spend[0]![1]
  for (const [at, usdAt] of spend) {
    if (at > t) break
    value = usdAt
  }
  return value
}

/** turn-meter's journal as this mod reads it (A38): the cost points, and what each model's requests added. */
export type Journal = { session: string; spend: [number, number][]; bymodel?: [number, string, number][] }

/** What the journals' sessions spent from `start` to `t`. */
export function spentBetween(logs: readonly Journal[], start: number, t: number): number {
  let sum = 0
  for (const log of logs) {
    const a = costAt(log.spend, start)
    const b = costAt(log.spend, t)
    if (a !== undefined && b !== undefined) sum += Math.max(0, b - a)
  }
  return sum
}
