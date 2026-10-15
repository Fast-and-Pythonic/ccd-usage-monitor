import { expect, test } from 'claude-code/testing'

import { formatLast, parseArgs, planOf, priceOf, referencesOf, type ResultEntry, type Shift } from './run'

test('the words: nothing is the three aliases, a family any case, a version exactly itself', () => {
  expect(parseArgs('')).toEqual({ specs: ['opus', 'sonnet', 'haiku'], isWeek: false })
  expect(parseArgs('opus Sonnet --week')).toEqual({ specs: ['opus', 'sonnet'], isWeek: true })
  const plans = parseArgs('opus-5 opus-5-5').specs.map((s) => planOf(s, [], {}))
  expect(plans.map((p) => p.callModel)).toEqual(['claude-opus-5', 'claude-opus-5-5'])
  expect(plans.map((p) => p.id)).toEqual(['opus-5', 'opus-5-5'])
})

test('a family is its newest version seen, called by full id; never seen, by the alias', () => {
  const plan = planOf('opus', ['opus-5', 'opus-5-5', 'haiku-5-5'], { 'opus-5-5': { usdPerPct5h: 0.41, at: 1 } })
  expect(plan.callModel).toBe('claude-opus-5-5')
  expect(plan.usdPerPct).toEqual({ five_hour: 0.41, seven_day: 2.85 })
  const alias = planOf('sonnet', ['opus-5-5'], {})
  expect(alias.callModel).toBe('sonnet')
  expect(alias.id).toBe(undefined)
  expect(alias.usdPerPct).toEqual({ five_hour: 0.39, seven_day: 2.85 })
})

/** Moves where the test spent `test` dollars and others `others`, at $0.40 a point for the test and the shared $0.39 for others. */
function moves(testUsd: number, othersUsd: number, ownOther = 0): Shift[] {
  const points = testUsd / 0.4 + (othersUsd + ownOther) / 0.39
  return [
    { t: 1, pct: 11, spent: 0.4, ledger: 5 },
    { t: 2, pct: 11 + points, spent: 0.4 + testUsd, ledger: 5 + testUsd + ownOther },
  ]
}

test('others spending meanwhile is taken off the points: a fifth is no mark, a third is noisy', () => {
  const clean = priceOf(moves(1.2, 0), 0, 0.39)!
  expect(Math.abs(clean.usdPerPct! - 0.4)).toBeLessThan(1e-9)
  const fifth = priceOf(moves(1.2, 0.3), 0.3, 0.39)!
  expect(Math.abs(fifth.usdPerPct! - 0.4)).toBeLessThan(1e-9)
  expect(fifth.isNoisy).toBe(false)
  const third = priceOf(moves(1.2, 0.6), 0.6, 0.39)!
  expect(Math.abs(third.usdPerPct! - 0.4)).toBeLessThan(1e-9)
  expect(third.isNoisy).toBe(true)
  // This session's own other spend (a main turn between requests) counts as others too.
  const own = priceOf(moves(1.2, 0, 0.3), 0, 0.39)!
  expect(Math.abs(own.usdPerPct! - 0.4)).toBeLessThan(1e-9)
  expect(Math.abs(own.othersUsd - 0.3)).toBeLessThan(1e-9)
})

const ENTRIES: ResultEntry[] = [
  { model: 'opus-5', at: 50, run: 10, usdPerPct5h: 0.6, points5h: 3, pointsWeek: 0, spent: 3, requests: 12, isNoisy: false, isWeek: false },
  { model: 'opus-5-5', at: 200, run: 100, usdPerPct5h: 0.4, points5h: 3, pointsWeek: 0, spent: 2.41, requests: 17, isNoisy: false, isWeek: false },
  { model: 'haiku-5-5', at: 300, run: 100, usdPerPct5h: 0.21, points5h: 2, pointsWeek: 0, spent: 0.9, requests: 40, isNoisy: true, isWeek: false, note: 'потолок времени' },
  { model: 'sonnet', at: 310, run: 100, points5h: 0, pointsWeek: 0, spent: 0, requests: 0, isNoisy: false, isWeek: false, note: 'остановлено' },
]

test('the last run as a markdown table', () => {
  expect(formatLast(ENTRIES)).toBe(
    [
      'замер от 1970-01-01 00:00 UTC',
      '',
      '| модель | версия | 1 % 5ч | окно 5ч | 1 % нед | точность | потрачено |',
      '| --- | --- | --- | --- | --- | --- | --- |',
      '| opus | 5.5 | \\$0.400 | \\$40 | — | 3 п. 5ч | \\$2.41 |',
      '| haiku | 5.5 | \\$0.210 | \\$21 | — | 2 п. 5ч · шумно · потолок времени | \\$0.90 |',
      '| sonnet | — | мало данных | — | — | 0 п. 5ч · остановлено | \\$0.00 |',
    ].join('\n'),
  )
  expect(formatLast([])).toBe('замеров ещё не было — `/limit-test` запускает')
})

test('the references: the latest sound entry per model', () => {
  expect(referencesOf(ENTRIES)).toEqual({ 'opus-5': { usdPerPct5h: 0.6, usdPerPctWeek: undefined, at: 50 }, 'opus-5-5': { usdPerPct5h: 0.4, usdPerPctWeek: undefined, at: 200 } })
})
