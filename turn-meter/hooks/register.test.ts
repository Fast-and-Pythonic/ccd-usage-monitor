import { expect, mock, test } from 'claude-code/testing'
import type { On, SessionUsage } from 'claude-code'

import type { TurnNote } from '../types'

const USAGE = { input_tokens: 12, output_tokens: 3100, cache_read_input_tokens: 1_200_000, cache_creation_input_tokens: 45_000 }

/** The session beneath the plugin: its id, a cost the test moves, and an empty file system. */
function engine(on: On, usage: { current: SessionUsage }, written: Record<string, string>): { notes: TurnNote[] } {
  const seen = { notes: [] as TurnNote[] }
  mock.store(on)
  mock.clock(on, { now: Date.parse('2026-10-15T12:00:00.000Z') })
  on('turn.start', ($, e) => ({ turnId: e.turnId }))
  on('turn.complete', ($, e) => ({ text: e.answer, usage: e.usage }))
  on('state.set', ($, e, next) => {
    if (e.plugin === 'turn-meter' && e.key === 'notes') seen.notes = e.value as TurnNote[]
    return next(e)
  })
  on('session.id', () => ({ value: 'abc' }))
  on('session.usage', () => ({ value: usage.current }))
  on('fs.exists', ($, e) => ({ value: e.path in written }))
  on('fs.list', () => ({ value: [] }))
  on('fs.write', ($, e) => {
    written[e.path] = e.text
    return { value: undefined }
  })
  return seen
}

const at = (usd: number): SessionUsage => ({ startedAt: 0, context: { window: 1_000_000, tokens: 212_000, percent: 21 }, rateLimits: [], cost: { usd } })

test('a turn: its dollars are the ledger moved between start and end, its subagents a group of their own', async ($, on) => {
  const usage = { current: at(1.0) }
  const written: Record<string, string> = {}
  const seen = engine(on, usage, written)
  await $.turn.start({ text: 'привет', turnId: 't1' })
  usage.current = at(1.55)
  await $.turn.complete({ answer: 'отчёт субагента', durationMs: 1000, isAborted: false, turnId: 'a1', agentId: 'agent-1', usage: { ...USAGE, model: 'claude-haiku-5-5' }, reason: 'answer' })
  await $.turn.complete({ answer: 'Готово.', durationMs: 95_000, isAborted: false, turnId: 't1', usage: { ...USAGE, model: 'claude-opus-5-5' }, reason: 'answer' })
  const value = seen.notes
  expect(value.length).toBe(1)
  expect(value[0]?.answer).toBe('Готово.')
  expect(value[0]?.line).toBe(
    'ctx 21% · 212k │ ≈1.4% 5ч · 0.19% нед │ \\$0.55\nход: вывод 3.1k · запись 45k · чтение 1.2M · вход 12 │ субагенты (1): вывод 3.1k · запись 45k · чтение 1.2M · вход 12\n1м 35с · opus-5-5',
  )
})

test('switches from the config shape the line', { options: { showContext: false, showTokens: false, showSubagents: false, showDuration: false, showModel: false, showSteps: false, autoCalibrate: false, usdPerPct5h: 0.5 } }, async ($, on) => {
  const usage = { current: at(0) }
  const seen = engine(on, usage, {})
  await $.turn.start({ text: '', turnId: 't1' })
  usage.current = at(1)
  await $.turn.complete({ answer: 'Да.', durationMs: 1, isAborted: false, turnId: 't1', usage: { ...USAGE, model: 'm' }, reason: 'answer' })
  const value = seen.notes
  expect(value[0]?.line).toBe('2.0% 5ч · 0.35% нед │ \\$1.00')
})

test('a turn with no answer leaves no line; a measurement goes into the journal', async ($, on) => {
  const usage = { current: at(0) }
  const written: Record<string, string> = {}
  const seen = engine(on, usage, written)
  on('session.measure', ($, e) => ({ changed: e.changed }))
  await $.session.measure({ context: usage.current.context, rateLimits: [{ kind: 'five_hour', percentUsed: 7, resetsAt: '2026-10-15T15:00:00.000Z' }], cost: { usd: 2 }, changed: ['rateLimits', 'cost'] })
  await $.turn.start({ text: '', turnId: 't1' })
  await $.turn.complete({ answer: '', durationMs: 1, isAborted: false, turnId: 't1', reason: 'answer' })
  const value = seen.notes
  expect(value).toEqual([])
  const files = Object.keys(written)
  expect(files.length).toBe(1)
  expect(files[0]).toMatch(/data[\\/]abc\.json$/)
  const log = JSON.parse(written[files[0]!]!)
  expect(log.spend.length).toBe(1)
  expect(log.obs[0].slice(1)).toEqual(['five_hour', '2026-10-15T15:00:00.000Z', 7])
})
