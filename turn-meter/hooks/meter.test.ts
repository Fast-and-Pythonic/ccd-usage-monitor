import { expect, test } from 'claude-code/testing'

import {
  addByModel,
  addReadings,
  addSpend,
  addTurn,
  calibrate,
  compareTurns,
  DAY_MS,
  emptyLog,
  fitWindows,
  formatComparison,
  formatLine,
  formatShare,
  formatTokens,
  modelFits,
  prune,
  referencesOf,
  resolveTracked,
  turnShares,
  windowKinds,
  windowRise,
  type ModelRate,
  type Rates,
  type SessionLog,
  type Show,
  type TurnFigures,
} from './meter'

const ALL: Show = { context: true, tokens: true, input: true, subagents: true, limits: true, usd: true, duration: true, model: true, steps: true, cache: true, limitSize: false }
const NONE: Show = { context: false, tokens: false, input: false, subagents: false, limits: false, usd: false, duration: false, model: false, steps: false, cache: false, limitSize: false }
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
    'ctx 21% · 212k │ ≈1.4% 5ч · 0.19% нед │ \\$0.55\nход: чтение 1.2M · запись 45k · вход 12 → вывод 3.1k\n1м 35с · запросов 7 · opus-5-5',
  )
})

test('each switch takes its part away; all off is no line', () => {
  expect(formatLine(FIGURES, NONE)).toBe(undefined)
  expect(formatLine(FIGURES, { ...NONE, usd: true })).toBe('\\$0.55')
  expect(formatLine(FIGURES, { ...NONE, tokens: true })).toBe('ход: чтение 1.2M · запись 45k → вывод 3.1k')
  expect(formatLine({ ...FIGURES, pct5h: { usdPerPct: 0.4, windows: 3, isLearned: true }, pctWeek: { usdPerPct: 2.5, windows: 3, isLearned: true } }, { ...NONE, limits: true })).toBe('1.4% 5ч · 0.22% нед')
})

test('subagents are a group of their own, only when some finished', () => {
  const withAgents = { ...FIGURES, agents: { count: 2, usage: USAGE } }
  expect(formatLine(withAgents, { ...NONE, subagents: true })).toBe('субагенты (2): чтение 1.2M · запись 45k → вывод 3.1k')
  expect(formatLine(FIGURES, { ...NONE, subagents: true })).toBe(undefined)
})

test('the cache lapse: local time in the third row, with the short lifetime marked', () => {
  const until = new Date(2026, 9, 15, 17, 42).getTime()
  expect(formatLine({ ...FIGURES, cache: { until, isShort: false } }, { ...NONE, cache: true })).toBe('кэш до 17:42')
  expect(formatLine({ ...FIGURES, cache: { until, isShort: true } }, { ...NONE, model: true, cache: true })).toBe('opus-5-5 · кэш до 17:42 (5м)')
  expect(formatLine(FIGURES, { ...NONE, cache: true })).toBe(undefined)
})

test('on extra usage the shares give way to a mark, and the dollars stay', () => {
  expect(formatLine({ ...FIGURES, isExtraUsage: true }, { ...NONE, limits: true, usd: true })).toBe('**сверх лимита** │ \\$0.55')
})

test('no ledger: no dollars and no shares, tokens still shown', () => {
  expect(formatLine({ ...FIGURES, usd: undefined }, { ...NONE, usd: true, limits: true, tokens: true })).toBe('ход: чтение 1.2M · запись 45k → вывод 3.1k')
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
  expect(log.obs.length).toBe(2)
  expect([...windowKinds([log])].map(([k, [, r, p]]) => [k, r, p])).toEqual([['five_hour', RESET, 4], ['spend_limit', '', 1]])
  const later = 10 * 24 * H
  addSpend(log, later, 5)
  prune(log, later)
  expect(log.spend).toEqual([[3, 2], [later, 5]])
  expect(log.obs).toEqual([])
})

/** One session's window at `price` dollars a point, its spend split over models by `shares`, each request journaled on its model. */
function modelWindow(price: number, points: number, shares: Record<string, number>): SessionLog[] {
  const log = emptyLog('m')
  addSpend(log, START - 1000, 10)
  let total = 0
  for (let k = 1; k <= points * 4; k++) {
    const t = START + k * 60_000
    for (const [model, share] of Object.entries(shares)) {
      const usd = (price / 4) * share
      total += usd
      addByModel(log, t, model, usd)
    }
    addSpend(log, t, 10 + total)
    addReadings(log, t + 1, [{ kind: 'five_hour', percentUsed: Math.floor(total / price + 1e-9), resetsAt: RESET }])
  }
  return [log]
}

test('a clean Opus window gives back its price as that of the model', () => {
  const fits = fitWindows(modelWindow(0.4, 20, { 'opus-5-5': 0.95, 'haiku-5-5': 0.05 }), 'five_hour', 0.39)
  const own = modelFits(fits, 'opus-5-5', 0.39, false)
  expect(own.length).toBe(1)
  expect(own[0]!.accepted).toBe(true)
  expect(own[0]!.usdPerPct).toBeGreaterThan(0.38)
  expect(own[0]!.usdPerPct).toBeLessThan(0.42)
  expect(modelFits(fits, 'haiku-5-5', 0.39, false)).toEqual([])
})

test('a 60/40 window counts for no model, but for the shared price', () => {
  const fits = fitWindows(modelWindow(0.4, 20, { 'opus-5-5': 0.6, 'sonnet-5-5': 0.4 }), 'five_hour', 0.39)
  expect(fits[0]!.accepted).toBe(true)
  expect(modelFits(fits, 'opus-5-5', 0.39, false)).toEqual([])
  expect(modelFits(fits, 'sonnet-5-5', 0.39, false)).toEqual([])
})

test('a family is its newest version seen; a versioned name is itself', () => {
  expect(resolveTracked('opus, haiku-4-5', ['opus-5', 'opus-5-5'])).toEqual(['opus-5-5', 'haiku-4-5'])
  expect(resolveTracked('Sonnet', [])).toEqual(['sonnet'])
})

test('a turn is priced model by model: $0.40 at 0.40 and $0.10 at 0.20 come to 1.5 points', () => {
  const cal = (usdPerPct: number) => ({ usdPerPct, windows: 3, isLearned: true })
  const shared: Rates = { five_hour: cal(0.39), seven_day: cal(2.5) }
  const own = (p: number): ModelRate => ({ five_hour: cal(p), seven_day: cal(2.5), hasOwn: { five_hour: true, seven_day: false } })
  const rates = new Map([['opus-5-5', own(0.4)], ['haiku-5-5', own(0.2)]])
  const shares = turnShares(new Map([['opus-5-5', 0.4], ['haiku-5-5', 0.1]]), 0.5, rates, shared)
  expect(formatLine({ ...FIGURES, usd: 0.5, shares }, { ...NONE, limits: true })).toBe('1.5% 5ч · 0.20% нед')
})

test('the size of the windows is a fourth row of its own', () => {
  const line = formatLine({ ...FIGURES, limitSize: { five_hour: 39, seven_day: 285 } }, { ...ALL, limitSize: true })
  expect(line).toBe(
    'ctx 21% · 212k │ ≈1.4% 5ч · 0.19% нед │ \\$0.55\nход: чтение 1.2M · запись 45k · вход 12 → вывод 3.1k\n1м 35с · запросов 7 · opus-5-5\n5ч ≈ \\$39 · нед ≈ \\$285',
  )
})

test('the results of limit-test: the latest sound entry per model and window', () => {
  const refs = referencesOf([
    { model: 'opus-5-5', at: 1, usdPerPct5h: 0.41, points5h: 3, pointsWeek: 0, spent: 2, requests: 9, isNoisy: false },
    { model: 'opus-5-5', at: 2, usdPerPct5h: 0.39, usdPerPctWeek: 2.9, points5h: 3, pointsWeek: 2, spent: 2, requests: 9, isNoisy: false },
    { model: 'opus-5-5', at: 3, usdPerPct5h: 0.6, points5h: 3, pointsWeek: 0, spent: 2, requests: 9, isNoisy: true },
    { model: 'haiku-5-5', at: 3, usdPerPct5h: 0.2, points5h: 1, pointsWeek: 0, spent: 1, requests: 30, isNoisy: false },
  ])
  expect(refs).toEqual({ 'opus-5-5': { usdPerPct5h: 0.39, usdPerPctWeek: 2.9, at: 2 } })
  expect(referencesOf('not json')).toEqual({})
})

const NOW = Date.parse('2026-10-15T12:00:00.000Z')

/** A day of turns; the window's readings in two windows: one that began before the day, one that runs to 14:00. */
function dayOfTurns(): SessionLog {
  const log = emptyLog('s')
  // Yesterday's window from 10:00: 30 % read before the span began, 40 % inside it.
  log.obs.push([NOW - 25 * H, 'five_hour', '2026-10-14T15:00:00.000Z', 30], [NOW - 22 * H, 'five_hour', '2026-10-14T15:00:00.000Z', 40])
  // The window under way began at 09:00 and reads 6 %.
  log.obs.push([NOW - 2 * H, 'five_hour', '2026-10-15T14:00:00.000Z', 2], [NOW - H, 'five_hour', '2026-10-15T14:00:00.000Z', 6])
  addTurn(log, [NOW - 26 * H, 0.39, 1, 0.14, 29, 30, 'opus-5-5']) // before the span
  addTurn(log, [NOW - 5 * H, 0.39 * 1.4, 1.4, 0.2, 38, 40, 'opus-5-5'])
  addTurn(log, [NOW - 3 * H, 0.39 * 0.6, 0.6, 0.08, 40, 1, 'opus-5-5']) // across the reset
  addTurn(log, [NOW - 2 * H, 0.39 * 0.3, 0.3, 0.04, 2, 2, 'opus-5-5'])
  addTurn(log, [NOW - H, 0.39 * 3.9, 3.9, 0.5, 2, 6, null])
  addTurn(log, [NOW - H / 2, 0.39 * 0.5, 0.5, 0.07, null, null, 'haiku-5-5'])
  return log
}

test('the turns of the last day against the steps Turn Usage shows and against the window', () => {
  const c = compareTurns([dayOfTurns()], NOW)
  expect(c.turns).toBe(5)
  expect(Math.abs((c.meter ?? NaN) - 6.7)).toBeLessThan(1e-9)
  expect(c.stepped).toEqual({ turns: 3, meter: 1.4 + 0.3 + 3.9, points: 6 })
  expect(c.resets).toBe(1)
  expect(c.unread).toBe(1)
  expect(c.ledger).toBeUndefined()
  // 30 → 40 in the window before, 0 → 6 in the one under way.
  expect(c.rise).toBe(16)
  // Per turn: −0.6, +0.3, −0.1.
  expect(Math.abs((c.diff?.median ?? NaN) - -0.1)).toBeLessThan(1e-9)
  expect(Math.abs((c.diff?.p25 ?? NaN) - -0.35)).toBeLessThan(1e-9)
  expect(Math.abs((c.diff?.p75 ?? NaN) - 0.1)).toBeLessThan(1e-9)
  expect(formatComparison(c)).toBe(
    [
      'сравнение с Turn Usage за сутки, окно 5ч (ходов 5 во всех журналах):',
      '- turn-meter: Σ 6.7 %',
      '- разность целых показаний, как у Turn Usage: Σ +6 п. за 3 ход., turn-meter на них Σ 5.6 % → turn-meter −7 % (не в счёт: через сброс окна 1, без показаний 1)',
      '- прирост окна: +16 п. → turn-meter −58 %',
      '- на ход, turn-meter минус Turn Usage: медиана −0.10 п., половина ходов от −0.35 до +0.10',
    ].join('\n'),
  )
})

test('no turns yet, and no readings of the window', () => {
  expect(formatComparison(compareTurns([emptyLog('s')], NOW))).toBe('сравнение с Turn Usage за сутки, окно 5ч: ходов ещё не записано')
  expect(windowRise([emptyLog('s')], 'five_hour', NOW - DAY_MS, NOW)).toBeUndefined()
})

test('turns older than the keep limit are pruned with the rest', () => {
  const log = dayOfTurns()
  prune(log, NOW + 8 * 24 * H - 1.5 * H)
  expect(log.turns?.map((r) => r[0])).toEqual([NOW - H, NOW - H / 2])
})

test('spend between turns: what the ledger grew by from the start of the first turn that no turn claimed', () => {
  const log = emptyLog('s')
  // $5 before the journal of turns began is not counted; the first turn ends at $6 having spent $1.
  log.spend.push([NOW - 30 * H, 0], [NOW - 3 * H, 6], [NOW - 2 * H - 1, 6.5], [NOW - 2 * H, 7.5], [NOW - H, 9])
  addTurn(log, [NOW - 3 * H, 1, 2.5, 0.35, 10, 13, 'opus-5-5', [100_000, 2000, 10, 500]])
  // $0.50 of a background subagent between the turns, the second turn's $1, then $1.50 after it.
  addTurn(log, [NOW - 2 * H, 1, 2.5, 0.35, 13, 16, 'opus-5-5'])
  // A journal of turns that holds none in the span adds nothing.
  const quiet = emptyLog('q')
  quiet.spend.push([NOW - 5 * H, 1], [NOW - H, 4])
  const c = compareTurns([log, quiet], NOW)
  // From $5 at the first turn's start to $9: $4, of which the turns claimed $2.
  expect(c.ledger).toEqual({ usd: 4, between: 2 })
  expect(formatComparison(c).split('\n')[4]).toBe('- траты между ходами, мимо строк: \\$2.00 из \\$4.00 (50 %)')
})
