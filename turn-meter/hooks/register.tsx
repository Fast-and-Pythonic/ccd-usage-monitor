// A dim line under each reply: the context's fill, what the turn spent in
// tokens and dollars, and what share of the five-hour and weekly windows that
// comes to, priced in dollars per point and learned from every session's
// spend journal (A38).

import { atom, read, update } from 'claude-code'
import type { EngineInterface, ModelUsage, PluginOptions, Register } from 'claude-code'

import type { TurnNote } from '../types'
import {
  addReadings,
  addSpend,
  addUsage,
  CACHE_LONG_MS,
  CACHE_SHORT_MS,
  calibrate,
  emptyLog,
  fitWindows,
  formatLine,
  KEEP_MS,
  prune,
  ZERO_USAGE,
  type Calibration,
  type SessionLog,
  type Show,
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
  log?: SessionLog
  isDirty: boolean
  needsFit: boolean
  rates: { five_hour: Calibration; seven_day: Calibration }
  // The main turn under way, and what its subagents and requests came to so far.
  current?: { turnId: string; usd?: number; steps: number; lastRequestAt?: number; agents: { count: number; usage: ModelUsage } }
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

async function refit($: EngineInterface, m: Meter): Promise<void> {
  if (!m.isAuto || !m.needsFit) return
  m.needsFit = false
  const logs = await allLogs($, await ownLog($, m))
  const since = Number((await $.store.get('resetAt')) ?? 0)
  m.rates = {
    five_hour: calibrate(fitWindows(logs, 'five_hour', m.prior5h, since), m.prior5h),
    seven_day: calibrate(fitWindows(logs, 'seven_day', m.priorWeek, since), m.priorWeek),
  }
}

async function report($: EngineInterface, m: Meter): Promise<string> {
  m.needsFit = true
  await refit($, m)
  const since = Number((await $.store.get('resetAt')) ?? 0)
  const logs = await allLogs($, await ownLog($, m))
  const rows: string[] = [`turn-meter: журналов ${logs.length}, автокалибровка ${m.isAuto ? 'вкл' : 'выкл'}`]
  for (const [kind, prior, label] of [
    ['five_hour', m.prior5h, '5ч'],
    ['seven_day', m.priorWeek, 'неделя'],
  ] as const) {
    const r = m.rates[kind]
    rows.push(`${label}: 1 % = $${r.usdPerPct.toFixed(3)} (конфиг $${prior}, окон ${r.windows}${r.isLearned ? '' : ', ещё не выучено'})`)
    for (const f of fitWindows(logs, kind, prior, since).slice(-5)) {
      rows.push(`  сброс ${f.resetsAt}: $${f.usdPerPct.toFixed(3)}, +${f.rise} п., точек ${f.anchors}, R² ${f.r2.toFixed(2)}${f.accepted ? '' : ` — отброшено: ${f.why}`}`)
    }
  }
  return rows.join('\n')
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
    isDirty: false,
    needsFit: true,
    rates: {
      five_hour: { usdPerPct: prior5h, windows: 0, isLearned: !isAuto },
      seven_day: { usdPerPct: priorWeek, windows: 0, isLearned: !isAuto },
    },
  }

  on('session.start', async ($, e, next) => {
    const r = await next(e)
    await $.command.register({
      name: 'turn-meter',
      description: 'Цена 1 % лимита, выученная turn-meter; `reset` забывает выученное',
    })
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
    const { cost } = await $.session.usage()
    m.current = { turnId: e.turnId, usd: cost?.usd, steps: 0, agents: { count: 0, usage: ZERO_USAGE } }
    return next(e)
  })

  on('turn.step', async function* ($, e, next) {
    const result = yield* next(e)
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
    await flush($, m)
    if (answer === '') return result

    await refit($, m)
    const { context, cost, rateLimits } = await $.session.usage()
    const isShort = rateLimits.some((w) => (w.kind === 'five_hour' || w.kind === 'seven_day') && w.percentUsed >= 100)
    const lastRequestAt = turn?.lastRequestAt
    const usd = cost?.usd !== undefined && turn?.usd !== undefined ? Math.max(0, cost.usd - turn.usd) : undefined
    const line = formatLine(
      {
        context,
        usage: e.usage,
        agents: turn?.agents ?? { count: 0, usage: ZERO_USAGE },
        usd,
        pct5h: m.rates.five_hour,
        pctWeek: m.rates.seven_day,
        durationMs: e.durationMs,
        model: e.usage?.model,
        steps: turn?.steps ?? 0,
        cache: lastRequestAt === undefined ? undefined : { until: lastRequestAt + (isShort ? CACHE_SHORT_MS : CACHE_LONG_MS), isShort },
      },
      show,
    )
    if (line !== undefined) await update($, notes, (list) => [...list, { answer, line }].slice(-MAX_NOTES))
    return result
  })

  on('command.run', { command: 'turn-meter' }, async ($, e) => {
    if (e.args.trim() === 'reset') {
      await $.store.set('resetAt', await $.clock.now())
      m.needsFit = true
      await refit($, m)
      return { text: 'turn-meter: выученное забыто, до новых окон — цены из конфига.' }
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
