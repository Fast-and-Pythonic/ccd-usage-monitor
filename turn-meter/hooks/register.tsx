// A dim line under each reply: the context's fill, what the turn spent in
// tokens and dollars, and what share of the five-hour and weekly windows that
// comes to, priced in dollars per point model by model and learned from every
// session's spend journal, started from limit-test's measurements (A38).

import { atom, read, update } from 'claude-code'
import type { EngineInterface, ModelUsage, PluginOptions, Register } from 'claude-code'

import type { TurnNote } from '../types'
import {
  addReadings,
  addByModel,
  addSpend,
  addTurn,
  addUsage,
  CACHE_LONG_MS,
  CACHE_SHORT_MS,
  calibrate,
  compareTurns,
  emptyLog,
  fitWindows,
  formatComparison,
  formatLimitSize,
  formatLine,
  KEEP_MS,
  markdownTable,
  modelRate,
  newestOf,
  parseModel,
  prune,
  referencesOf,
  resolveTracked,
  shortModel,
  turnShares,
  ZERO_USAGE,
  type ModelRate,
  type Rates,
  type Reference,
  type SessionLog,
  type Show,
  type WindowFit,
  windowKinds,
} from './meter'

const MAX_NOTES = 100
const notes = atom({ plugin: 'turn-meter', key: 'notes' } as const, [] as TurnNote[])

const flag = (options: PluginOptions, key: string, fallback: boolean) => (typeof options[key] === 'boolean' ? (options[key] as boolean) : fallback)
const price = (options: PluginOptions, key: string, fallback: number) => {
  const v = options[key]
  return typeof v === 'number' && v > 0 ? v : fallback
}

export function showOf(options: PluginOptions): Show {
  return {
    context: flag(options, 'showContext', true),
    tokens: flag(options, 'showTokens', true),
    input: flag(options, 'showInput', true),
    subagents: flag(options, 'showSubagents', true),
    limits: flag(options, 'showLimits', true),
    usd: flag(options, 'showUsd', true),
    duration: flag(options, 'showDuration', true),
    model: flag(options, 'showModel', true),
    steps: flag(options, 'showSteps', true),
    cache: flag(options, 'showCache', true),
    limitSize: flag(options, 'showLimitSize', false),
  }
}

const dataDir = ($: EngineInterface) => `${$.plugin.root}/data`

/** Every other session's journal younger than the keep limit, and this one's as held in memory. */
async function allLogs($: EngineInterface, own: SessionLog): Promise<SessionLog[]> {
  const dir = dataDir($)
  const logs = [own]
  if (!(await $.fs.exists(dir))) return logs
  const now = await $.clock.now()
  for (const entry of await $.fs.list(dir)) {
    if (entry.kind !== 'file' || !entry.name.endsWith('.json') || entry.name === `${own.session}.json`) continue
    if (now - entry.mtimeMs > KEEP_MS) continue
    try {
      logs.push(JSON.parse(await $.fs.read(`${dir}/${entry.name}`)) as SessionLog)
    } catch {
      // A journal another session is writing right now: next time.
    }
  }
  return logs
}

/** What the module holds for its session: the journal, the prices in use and the main turn under way. */
type Meter = {
  isAuto: boolean
  prior5h: number
  priorWeek: number
  trackModels: string
  log?: SessionLog
  isDirty: boolean
  needsFit: boolean
  rates: Rates
  /** Each model's price, for every model the journals or limit-test's references name. */
  modelRates: Map<string, ModelRate>
  /** The models seen, short form. */
  seen: Set<string>
  references: Record<string, Reference>
  /** The ledger when the last request's dollars were put on its model. */
  lastCost?: number
  // The main turn under way, and what its subagents, requests and models came to so far.
  current?: { turnId: string; usd?: number; pct5h?: number; steps: number; lastRequestAt?: number; agents: { count: number; usage: ModelUsage }; byModel: Map<string, number> }
}

async function ownLog($: EngineInterface, m: Meter): Promise<SessionLog> {
  if (m.log !== undefined) return m.log
  const id = await $.session.id()
  const path = `${dataDir($)}/${id}.json`
  m.log = emptyLog(id)
  try {
    if (await $.fs.exists(path)) m.log = JSON.parse(await $.fs.read(path)) as SessionLog
  } catch {
    m.log = emptyLog(id)
  }
  return m.log
}

async function flush($: EngineInterface, m: Meter): Promise<void> {
  if (!m.isDirty || m.log === undefined) return
  prune(m.log, await $.clock.now())
  await $.fs.write(`${dataDir($)}/${m.log.session}.json`, JSON.stringify(m.log))
  m.isDirty = false
}

/** A request's model in the short form; an alias (`haiku`) as the newest of its family seen. */
function modelName(m: Meter, model: string): string {
  const short = shortModel(model)
  const p = parseModel(short)
  if (p !== undefined && p.version.length === 0) return newestOf(p.family, m.seen) ?? short
  return short
}

/** Puts what the ledger grew by since the last request on the model that answered this one. */
async function recordStep($: EngineInterface, m: Meter, model: string): Promise<void> {
  const { cost } = await $.session.usage()
  if (cost === undefined) return
  const usd = cost.usd - (m.lastCost ?? cost.usd)
  m.lastCost = cost.usd
  if (!(usd > 0)) return
  const name = modelName(m, model)
  m.seen.add(name)
  addByModel(await ownLog($, m), await $.clock.now(), name, usd)
  m.isDirty = true
  if (m.current !== undefined) m.current.byModel.set(name, (m.current.byModel.get(name) ?? 0) + usd)
}

/** limit-test's results, in its own folder beside this one (A39); none while it has measured nothing. */
async function readReferences($: EngineInterface): Promise<Record<string, Reference>> {
  const path = `${$.plugin.root}/../limit-test/data/results.json`
  try {
    if (!(await $.fs.exists(path))) return {}
    return referencesOf(JSON.parse(await $.fs.read(path)))
  } catch {
    return {}
  }
}

async function refit($: EngineInterface, m: Meter): Promise<void> {
  if (!m.needsFit) return
  m.needsFit = false
  m.references = await readReferences($)
  const logs = await allLogs($, await ownLog($, m))
  for (const log of logs) for (const [, model] of log.bymodel ?? []) m.seen.add(model)
  for (const id of Object.keys(m.references)) m.seen.add(id)
  const fits = { five_hour: [] as WindowFit[], seven_day: [] as WindowFit[] }
  if (m.isAuto) {
    const since = Number((await $.store.get('resetAt')) ?? 0)
    fits.five_hour = fitWindows(logs, 'five_hour', m.prior5h, since)
    fits.seven_day = fitWindows(logs, 'seven_day', m.priorWeek, since)
    m.rates = { five_hour: calibrate(fits.five_hour, m.prior5h), seven_day: calibrate(fits.seven_day, m.priorWeek) }
  }
  m.modelRates = new Map([...m.seen].map((id) => [id, modelRate(fits, id, m.rates, m.references[id])]))
}

// Escaped: a command's answer is drawn as markdown, and two amounts on a line would read as math.
const usd = (n: number, digits: number) => `\\$${n.toFixed(digits)}`

/** The price table: one row a tracked model, its own price in each window or `мало данных`. */
function modelTable(m: Meter): string {
  const rows = [['модель', '1 % 5ч', 'окно 5ч', '1 % нед', 'окон', 'источник']]
  for (const id of resolveTracked(m.trackModels, m.seen)) {
    const r = m.modelRates.get(id)
    if (r === undefined || (!r.hasOwn.five_hour && !r.hasOwn.seven_day)) {
      rows.push([id, 'мало данных', '', 'мало данных', '', ''])
      continue
    }
    const isLive = r.five_hour.windows > 0 || r.seven_day.windows > 0
    rows.push([
      id,
      r.hasOwn.five_hour ? usd(r.five_hour.usdPerPct, 3) : 'мало данных',
      r.hasOwn.five_hour ? usd(r.five_hour.usdPerPct * 100, 0) : '',
      r.hasOwn.seven_day ? usd(r.seven_day.usdPerPct, 3) : 'мало данных',
      `${r.five_hour.windows} / ${r.seven_day.windows}`,
      isLive ? 'живые окна' : 'замер limit-test',
    ])
  }
  return markdownTable(rows)
}

/** The five-hour window's reading as the session last measured it: what Turn Usage takes its difference of. */
const reading5h = (limits: readonly { kind: string; percentUsed: number }[]) => limits.find((w) => w.kind === 'five_hour')?.percentUsed

async function report($: EngineInterface, m: Meter): Promise<string> {
  m.needsFit = true
  await refit($, m)
  const since = Number((await $.store.get('resetAt')) ?? 0)
  const logs = await allLogs($, await ownLog($, m))
  // Drawn as markdown: a paragraph a line, each price's fitted windows a list under it.
  const rows: string[] = [`журналов ${logs.length}, автокалибровка ${m.isAuto ? 'вкл' : 'выкл'}`]
  const kinds = [...windowKinds(logs)].map(([kind, [, resetsAt, pct]]) => `${kind} ${pct}%${resetsAt === '' ? '' : ` (сброс ${resetsAt})`}`)
  rows.push(`окна лимитов: ${kinds.length === 0 ? 'ещё не видели' : kinds.join(', ')}`)
  rows.push(`размер окна: ${formatLimitSize({ five_hour: m.rates.five_hour.usdPerPct * 100, seven_day: m.rates.seven_day.usdPerPct * 100 }, true)}`)
  for (const [kind, prior, label] of [
    ['five_hour', m.prior5h, '5ч'],
    ['seven_day', m.priorWeek, 'неделя'],
  ] as const) {
    const r = m.rates[kind]
    const lines = [`${label}: 1 % = ${usd(r.usdPerPct, 3)} (конфиг \\$${prior}, окон ${r.windows}${r.isLearned ? '' : ', ещё не выучено'})`]
    for (const f of fitWindows(logs, kind, prior, since).slice(-5)) {
      lines.push(`- сброс ${f.resetsAt}: ${usd(f.usdPerPct, 3)}, +${f.rise} п., точек ${f.anchors}, R² ${f.r2.toFixed(2)}${f.accepted ? '' : ` — отброшено: ${f.why}`}`)
    }
    rows.push(lines.join('\n'))
  }
  rows.push('по моделям (окна, где на модель ≥ 90 % трат; без них — замер limit-test):', modelTable(m))
  rows.push(formatComparison(compareTurns(logs, await $.clock.now())))
  return rows.join('\n\n')
}

export const register: Register = (on, options) => {
  const show = showOf(options)
  const isAuto = flag(options, 'autoCalibrate', true)
  const prior5h = price(options, 'usdPerPct5h', 0.39)
  const priorWeek = price(options, 'usdPerPctWeek', 2.85)
  const m: Meter = {
    isAuto,
    prior5h,
    priorWeek,
    trackModels: typeof options.trackModels === 'string' && options.trackModels.trim() !== '' ? options.trackModels : 'opus, sonnet, haiku',
    isDirty: false,
    needsFit: true,
    rates: {
      five_hour: { usdPerPct: prior5h, windows: 0, isLearned: !isAuto },
      seven_day: { usdPerPct: priorWeek, windows: 0, isLearned: !isAuto },
    },
    modelRates: new Map(),
    seen: new Set(),
    references: {},
  }

  on('session.start', async ($, e, next) => {
    const r = await next(e)
    await $.command.register({
      name: 'turn-meter',
      description: 'Цена 1 % лимита по моделям, размер окон и сравнение с Turn Usage за сутки; `reset` — забыть выученное',
    })
    m.lastCost = (await $.session.usage()).cost?.usd
    await refit($, m)
    return r
  })

  // Every measurement with a new cost or reading goes into this session's journal.
  on('session.measure', async ($, e, next) => {
    const own = await ownLog($, m)
    const now = await $.clock.now()
    const before = own.spend.length + own.obs.length
    addSpend(own, now, e.cost?.usd)
    addReadings(own, now, e.rateLimits)
    if (own.spend.length + own.obs.length !== before) m.isDirty = true
    if (e.changed.includes('rateLimits')) {
      m.needsFit = true
      await flush($, m)
    }
    return next(e)
  })

  on('turn.start', async ($, e, next) => {
    const { cost, rateLimits } = await $.session.usage()
    // A module reloaded mid-session saw no session.start: its first request is priced from here.
    m.lastCost ??= cost?.usd
    m.current = { turnId: e.turnId, usd: cost?.usd, pct5h: reading5h(rateLimits), steps: 0, agents: { count: 0, usage: ZERO_USAGE }, byModel: new Map() }
    return next(e)
  })

  on('turn.step', async function* ($, e, next) {
    const result = yield* next(e)
    if (result.usage !== null) await recordStep($, m, result.usage.model)
    // Each main-loop request reads the cache and starts its lifetime over; a subagent's prompt is its own.
    if (e.agentId === undefined && m.current?.turnId === e.turnId && result.usage !== null) {
      m.current.steps += 1
      m.current.lastRequestAt = await $.clock.now()
    }
    return result
  })

  on('turn.complete', async ($, e, next) => {
    const result = await next(e)
    if (e.agentId !== undefined) {
      // A subagent that finished while a main turn ran counts toward it.
      if (m.current !== undefined && e.usage !== undefined) {
        m.current.agents = { count: m.current.agents.count + 1, usage: addUsage(m.current.agents.usage, e.usage) }
      }
      return result
    }
    const turn = m.current?.turnId === e.turnId ? m.current : undefined
    m.current = undefined
    const answer = e.answer.trim()
    await refit($, m)
    const { context, cost, rateLimits } = await $.session.usage()
    const usd = cost?.usd !== undefined && turn?.usd !== undefined ? Math.max(0, cost.usd - turn.usd) : undefined
    const shares = usd === undefined || turn === undefined ? undefined : turnShares(turn.byModel, usd, m.modelRates, m.rates)
    // Every main turn, with a line or without, for the comparison with Turn Usage in /turn-meter.
    if (usd !== undefined && turn !== undefined) {
      const share5h = shares?.five_hour ?? usd / m.rates.five_hour.usdPerPct
      const shareWeek = shares?.seven_day ?? usd / m.rates.seven_day.usdPerPct
      const model = e.usage?.model === undefined ? null : shortModel(e.usage.model)
      addTurn(await ownLog($, m), [await $.clock.now(), usd, share5h, shareWeek, turn.pct5h ?? null, reading5h(rateLimits) ?? null, model])
      m.isDirty = true
    }
    await flush($, m)
    if (answer === '') return result

    const isShort = rateLimits.some((w) => (w.kind === 'five_hour' || w.kind === 'seven_day') && w.percentUsed >= 100)
    const lastRequestAt = turn?.lastRequestAt
    const line = formatLine(
      {
        context,
        usage: e.usage,
        agents: turn?.agents ?? { count: 0, usage: ZERO_USAGE },
        usd,
        pct5h: m.rates.five_hour,
        pctWeek: m.rates.seven_day,
        shares,
        limitSize: { five_hour: m.rates.five_hour.usdPerPct * 100, seven_day: m.rates.seven_day.usdPerPct * 100 },
        durationMs: e.durationMs,
        model: e.usage?.model,
        steps: turn?.steps ?? 0,
        isExtraUsage: isShort,
        cache: lastRequestAt === undefined ? undefined : { until: lastRequestAt + (isShort ? CACHE_SHORT_MS : CACHE_LONG_MS), isShort },
      },
      show,
    )
    if (line !== undefined) await update($, notes, (list) => [...list, { answer, line }].slice(-MAX_NOTES))
    return result
  })

  on('command.run', { command: 'turn-meter' }, async ($, e) => {
    const args = e.args.trim().split(/\s+/).filter((a) => a !== '')
    if (args[0] === 'reset') {
      await $.store.set('resetAt', await $.clock.now())
      m.needsFit = true
      await refit($, m)
      return { text: 'выученное забыто, до новых окон — цены из конфига и замеров limit-test.' }
    }
    return { text: await report($, m) }
  })

  // The reply's last text block is the one the turn's answer ends with.
  on('ui.render', { component: 'AssistantMessage' }, async ($, e, next) => {
    const text = e.props.text.trim()
    const list = await read($, notes)
    const note = text === '' ? undefined : list.findLast((n) => n.answer.endsWith(text))
    if (note === undefined) return next(e)
    // Line up with the reply's text: the terminal indents it past the bullet,
    // the desktop half a cell in from the message's edge.
    const indent = e.surface === 'terminal' ? 2 : 0.5
    const { Box, Markdown } = $.ui.resolve(e)
    return (
      <Box flexDirection="column">
        {await next(e)}
        <Box marginTop={1} paddingLeft={indent} flexDirection="column">
          {note.line.split('\n').map((row, i) => (
            <Markdown key={String(i)} dimColor text={row} />
          ))}
        </Box>
      </Box>
    )
  })
}
