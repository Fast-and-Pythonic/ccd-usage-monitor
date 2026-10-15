import { expect, mock, test } from 'claude-code/testing'
import type { Engine, MockClock } from 'claude-code/testing'
import type { On, SessionUsage } from 'claude-code'

import type { ResultEntry } from './run'

const USAGE = { input_tokens: 10, output_tokens: 1000, cache_read_input_tokens: 0, cache_creation_input_tokens: 3000 }
const RESET = '2026-10-15T15:00:00.000Z'

/**
 * The account beneath the mod: each subagent's answer costs `perRequest`, and
 * the five-hour reading is whole points of what was spent at `price` a point.
 */
type World = {
  price: number
  perRequest: number
  spent: number
  base: number
  /** The ledger of this session. */
  cost: number
  spawns: string[]
  pending: string[]
  logs: string[]
  files: Record<string, string>
  usage: () => SessionUsage
}

function world(on: On, base: number, price = 0.4, perRequest = 0.1): World {
  const w: World = {
    price,
    perRequest,
    spent: 0,
    base,
    cost: 5,
    spawns: [],
    pending: [],
    logs: [],
    files: {},
    usage: () => ({
      startedAt: 0,
      context: { window: 1_000_000, tokens: 30_000, percent: 3 },
      rateLimits: [{ kind: 'five_hour', percentUsed: w.base + Math.floor(w.spent / w.price + 1e-9), resetsAt: RESET }],
      cost: { usd: w.cost },
    }),
  }
  mock.store(on)
  on('session.start', ($, e) => ({ cwd: e.cwd }))
  on('session.id', () => ({ value: 'own' }))
  on('session.usage', () => ({ value: w.usage() }))
  on('command.register', ($, e) => ({ value: { command: e.name } }))
  on('agent.register', ($, e) => ({ value: { agent: `limit-test:${e.name}` } }))
  on('agent.spawn', ($, e) => {
    const agentId = `a${w.spawns.length + 1}`
    w.spawns.push(e.model ?? '')
    w.pending.push(agentId)
    return { model: e.model ?? 'claude-opus-5-5', agentId }
  })
  on('turn.complete', ($, e) => ({ text: e.answer, usage: e.usage }))
  on('ui.log', ($, e) => {
    w.logs.push(e.text)
    return { value: undefined }
  })
  on('fs.exists', ($, e) => ({ value: e.path in w.files }))
  on('fs.list', () => ({ value: [] }))
  on('fs.read', ($, e) => ({ value: w.files[e.path] ?? '' }))
  on('fs.write', ($, e) => {
    w.files[e.path] = e.text
    return { value: undefined }
  })
  return w
}

/** Answers each spawned subagent, as many as `max`, letting the run go on between them. */
async function drive($: Engine, w: World, clock: MockClock, max: number): Promise<void> {
  for (let i = 0; i < max; i++) {
    await clock.advance(0)
    const agentId = w.pending.shift()
    if (agentId === undefined) return
    w.spent += w.perRequest
    w.cost += w.perRequest
    await $.turn.complete({ answer: '1. …', durationMs: 1, isAborted: false, turnId: agentId, agentId, usage: { ...USAGE, model: 'claude-opus-5-5' }, reason: 'answer' })
  }
}

const results = (w: World): ResultEntry[] => {
  const path = Object.keys(w.files).find((p) => /data[\\/]results\.json$/.test(p))
  return path === undefined ? [] : (JSON.parse(w.files[path]!) as ResultEntry[])
}

/** The person typing `/limit-test <args>`. */
const command = ($: Engine, args: string) =>
  $.command.run({ command: 'limit-test', args, origin: { kind: 'composer' }, presentation: { isFullscreen: false, columns: 100 } })

async function begin($: Engine, args: string): Promise<string> {
  await $.session.start({ cwd: '.', surface: null, isInteractive: false })
  return (await command($, args)).text ?? ''
}

test('a run at $0.40 a point measures $0.400, writes it and says where the table is', async ($, on) => {
  const clock = mock.clock(on, { now: Date.parse('2026-10-15T12:00:00.000Z') })
  const w = world(on, 10)
  const text = await begin($, 'opus-5-5')
  expect(text).toMatch(/^замер запущен: opus-5-5\./)
  await drive($, w, clock, 40)
  expect(w.spawns.every((m) => m === 'claude-opus-5-5')).toBe(true)
  const [entry] = results(w)
  expect(entry?.model).toBe('opus-5-5')
  expect(entry?.points5h).toBe(3)
  expect(entry?.isNoisy).toBe(false)
  expect(Math.abs(entry!.usdPerPct5h! - 0.4)).toBeLessThan(0.02)
  expect(w.logs[w.logs.length - 1]).toBe('замер готов — `/limit-test last`')
  expect(w.logs.some((l) => /^opus-5-5 · запрос 1 · 5ч 10 % · \$0\.100 \(итого \$0\.10\)$/.test(l))).toBe(true)
  const last = (await command($, 'last')).text ?? ''
  expect(last).toMatch(/\| opus \| 5\.5 \| \\\$0\.400 \| \\\$40 \| — \| 3 п\. 5ч \|/)
})

test('a window used up or above 80 % is refused before any request', async ($, on) => {
  mock.clock(on, { now: 0 })
  const full = world(on, 100)
  expect(await begin($, '')).toMatch(/сверх лимита/)
  full.base = 85
  expect((await command($, 'opus')).text).toMatch(/уже 85 % \(больше 80 %\)/)
  expect(full.spawns).toEqual([])
})

test('stop: nothing is spawned after it, and what was measured is kept', async ($, on) => {
  const clock = mock.clock(on, { now: 0 })
  const w = world(on, 10)
  await begin($, 'opus-5-5 sonnet')
  await drive($, w, clock, 3)
  expect((await command($, 'stop')).text).toBe('замер остановлен; измеренное — `/limit-test last`')
  const spawned = w.spawns.length
  await drive($, w, clock, 5)
  await clock.advance(60_000)
  expect(w.spawns.length).toBe(spawned)
  expect(results(w).map((e) => [e.model, e.note])).toEqual([['opus-5-5', 'остановлено']])
  expect((await command($, 'stop')).text).toBe('замер не идёт')
})
