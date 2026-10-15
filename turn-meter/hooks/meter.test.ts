import { expect, test } from 'claude-code/testing'

import { addReadings, addSpend, calibrate, emptyLog, fitWindows, formatLine, formatShare, formatTokens, prune, type SessionLog, type Show, type TurnFigures } from './meter'

const ALL: Show = { context: true, tokens: true, input: true, subagents: true, limits: true, usd: true, duration: true, model: true, steps: true }
const NONE: Show = { context: false, tokens: false, input: false, subagents: false, limits: false, usd: false, duration: false, model: false, steps: false }
const USAGE = { input_tokens: 12, output_tokens: 3100, cache_read_input_tokens: 1_200_000, cache_creation_input_tokens: 45_000 }
const FIGURES: TurnFigures = {
  context: { percent: 21, tokens: 212_000 },
  usage: USAGE,
  agents: { count: 0, usage: { ...USAGE, output_tokens: 0 } },
  usd: 0.55,
  pct5h: { usdPerPct: 0.39, windows: 0, isLearned: false },
  pctWeek: { usdPerPct: 2.85, windows: 0, isLearned: false },
  durationMs: 95_000,
  model: 'claude-opus-5-5',
  steps: 7,
}

test('numbers: tokens in k and M, shares with a floor', () => {
  expect(formatTokens(950)).toBe('950')
  expect(formatTokens(3100)).toBe('3.1k')
  expect(formatTokens(212_000)).toBe('212k')
  expect(formatTokens(1_200_000)).toBe('1.2M')
  expect(formatShare(1.41, 1)).toBe('1.4%')
  expect(formatShare(0.004, 2)).toBe('<0.01%')
  expect(formatShare(0, 1)).toBe('0.0%')
})

test('the whole line, every part on', () => {
  expect(formatLine(FIGURES, ALL)).toBe(
    'ctx 21% · 212k │ ход: вывод 3.1k · запись 45k · чтение 1.2M · вход 12 │ ≈1.4% 5ч · 0.19% нед │ \\$0.55 │ 1м 35с · запросов 7 · opus-5-5',
  )
})

test('each switch takes its part away; all off is no line', () => {
  expect(formatLine(FIGURES, NONE)).toBe(undefined)
  expect(formatLine(FIGURES, { ...NONE, usd: true })).toBe('\\$0.55')
  expect(formatLine(FIGURES, { ...NONE, tokens: true })).toBe('ход: вывод 3.1k · запись 45k · чтение 1.2M')
  expect(formatLine({ ...FIGURES, pct5h: { usdPerPct: 0.4, windows: 3, isLearned: true }, pctWeek: { usdPerPct: 2.5, windows: 3, isLearned: true } }, { ...NONE, limits: true })).toBe('1.4% 5ч · 0.22% нед')
})

test('subagents are a group of their own, only when some finished', () => {
  const withAgents = { ...FIGURES, agents: { count: 2, usage: USAGE } }
  expect(formatLine(withAgents, { ...NONE, subagents: true })).toBe('субагенты (2): вывод 3.1k · запись 45k · чтение 1.2M')
  expect(formatLine(FIGURES, { ...NONE, subagents: true })).toBe(undefined)
})

test('no ledger: no dollars and no shares, tokens still shown', () => {
  expect(formatLine({ ...FIGURES, usd: undefined }, { ...NONE, usd: true, limits: true, tokens: true })).toBe('ход: вывод 3.1k · запись 45k · чтение 1.2M')
})

const H = 60 * 60 * 1000
const RESET = '2026-10-15T15:00:00.000Z'
const START = Date.parse(RESET) - 5 * H

/** A window where the account spends `price` dollars per point, split over sessions, read by the first one. */
function window(price: number, points: number, sessions: number, unseen = 0): SessionLog[] {
  const logs = Array.from({ length: sessions }, (_, i) => emptyLog(`s${i}`))
  logs.forEach((l) => addSpend(l, START - 1000, 10))
  let seen = 0
  let total = 0
  for (let k = 1; k <= points * 4; k++) {
    const t = START + k * 60_000
    const step = price / 4
    total += step
    // Part of the spend happens where no journal sees it.
    if (k % 3 !== 0 || unseen === 0) {
      seen += step
      const l = logs[k % sessions]!
      addSpend(l, t, (l.spend[l.spend.length - 1]?.[1] ?? 10) + step)
    }
    addReadings(logs[0]!, t + 1, [{ kind: 'five_hour', percentUsed: Math.floor(total / price + 1e-9), resetsAt: RESET }])
  }
  return logs
}

test('a clean window gives back its price, whichever session spent it', () => {
  const fits = fitWindows(window(0.39, 20, 3), 'five_hour', 0.39)
  expect(fits.length).toBe(1)
  expect(fits[0]!.accepted).toBe(true)
  expect(fits[0]!.usdPerPct).toBeGreaterThan(0.37)
  expect(fits[0]!.usdPerPct).toBeLessThan(0.41)
})

test('a window that rose too little is not trusted', () => {
  const fits = fitWindows(window(0.39, 5, 1), 'five_hour', 0.39)
  expect(fits[0]!.accepted).toBe(false)
  expect(fitWindows(window(0.39, 5, 1), 'seven_day', 2.85)).toEqual([])
})

test('a third of the spend unseen reads cheaper, and is refused when far off', () => {
  const fits = fitWindows(window(0.39, 20, 2, 1), 'five_hour', 0.39)
  expect(fits[0]!.usdPerPct).toBeLessThan(0.3)
  const far = fitWindows(window(0.39, 20, 2, 1), 'five_hour', 0.9)
  expect(far[0]!.accepted).toBe(false)
  expect(far[0]!.why).toBe('далеко от конфига')
})

test('few windows blend with the config; three are trusted alone', () => {
  const fit = { kind: 'five_hour', resetsAt: RESET, rise: 10, anchors: 10, r2: 1, accepted: true }
  expect(calibrate([], 0.39)).toEqual({ usdPerPct: 0.39, windows: 0, isLearned: false })
  const one = calibrate([{ ...fit, usdPerPct: 0.3 }], 0.39)
  expect(Math.abs(one.usdPerPct - 0.36)).toBeLessThan(1e-9)
  expect(one.isLearned).toBe(false)
  const three = calibrate([0.3, 0.32, 0.5].map((usdPerPct) => ({ ...fit, usdPerPct })), 0.39)
  expect(three).toEqual({ usdPerPct: 0.32, windows: 3, isLearned: true })
})

test('the journal keeps changes only, and old points go, but the baseline stays', () => {
  const log = emptyLog('s')
  addSpend(log, 1, 1)
  addSpend(log, 2, 1)
  addSpend(log, 3, 2)
  addReadings(log, 3, [{ kind: 'five_hour', percentUsed: 4, resetsAt: RESET }, { kind: 'spend_limit', percentUsed: 1 }])
  addReadings(log, 4, [{ kind: 'five_hour', percentUsed: 4, resetsAt: RESET }])
  expect(log.spend).toEqual([[1, 1], [3, 2]])
  expect(log.obs.length).toBe(1)
  const later = 10 * 24 * H
  addSpend(log, later, 5)
  prune(log, later)
  expect(log.spend).toEqual([[3, 2], [later, 5]])
  expect(log.obs).toEqual([])
})
