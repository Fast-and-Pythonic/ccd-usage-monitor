// limit-test: measures what one point of the five-hour (and weekly) window
// costs on each model, by spending on that model alone through subagents and
// watching the windows move; turn-meter takes the results as references (A39).

import type { EngineInterface, Register, Timer } from 'claude-code'

import {
  entriesOf,
  entryOf,
  expectedPoints,
  expectedUsd,
  formatLast,
  MAX_ERRORS,
  nextTokens,
  parseArgs,
  planOf,
  priceOf,
  referencesOf,
  refusal,
  shortModel,
  spentBetween,
  START_PRICE,
  startRun,
  STOP_AT,
  takeAnswer,
  testPrompt,
  verdict,
  WRITER_PROMPT,
  type Journal,
  type ModelPlan,
  type ModelRun,
  type ResultEntry,
} from './run'

/** The mod's own agent type: no tools, no CLAUDE.md, a one-line prompt, so a request costs little beyond its answer. */
const WRITER = 'writer'
const WRITER_TYPE = `limit-test:${WRITER}`
/** A main turn of this session is waited out in steps of this. */
const WAIT_MS = 5000
/** A subagent that has not answered by then counts as a request with no answer. */
const ANSWER_MS = 10 * 60 * 1000
/** Journals older than this are not read: turn-meter keeps eight days. */
const JOURNAL_MS = 8 * 24 * 60 * 60 * 1000

/** A subagent's answer as its turn.complete gave it. */
type Answer = { output: number; model?: string }

/** A run of `/limit-test`: the models still to measure, the one under way, the entries of the finished ones. */
type Test = {
  queue: ModelPlan[]
  isWeek: boolean
  startedAt: number
  run?: ModelRun
  entries: ResultEntry[]
  abort: AbortController
  timer?: Timer
}

/** What the module holds for its session. */
type Lab = {
  test?: Test
  /** Set by the background loop of session.start: hands it a run to drive. */
  wake?: () => void
  /** A main turn of this session is under way. */
  isMainTurn: boolean
  hasWriter: boolean
  /** Spawned subagents waiting for their answer, by agent id. */
  waiters: Map<string, (answer: Answer | undefined) => void>
  /** Answers that came before their spawn had resolved. */
  early: Map<string, Answer>
}

const label = (plan: ModelPlan) => plan.id ?? `${plan.spec} (новейшая)`
const resultsPath = ($: EngineInterface) => `${$.plugin.root}/data/results.json`
const journalDir = ($: EngineInterface) => `${$.plugin.root}/../turn-meter/data`

/** turn-meter's journals younger than it keeps them; with `exclude`, that session's left out. */
async function readJournals($: EngineInterface, exclude?: string): Promise<Journal[]> {
  const dir = journalDir($)
  if (!(await $.fs.exists(dir))) return []
  const now = await $.clock.now()
  const logs: Journal[] = []
  for (const entry of await $.fs.list(dir)) {
    if (entry.kind !== 'file' || !entry.name.endsWith('.json') || entry.name === `${exclude}.json`) continue
    if (now - entry.mtimeMs > JOURNAL_MS) continue
    try {
      const log = JSON.parse(await $.fs.read(`${dir}/${entry.name}`)) as Journal
      if (Array.isArray(log.spend)) logs.push(log)
    } catch {
      // A journal its session is writing right now: next time.
    }
  }
  return logs
}

async function readResults($: EngineInterface): Promise<ResultEntry[]> {
  try {
    if (!(await $.fs.exists(resultsPath($)))) return []
    return entriesOf(JSON.parse(await $.fs.read(resultsPath($))))
  } catch {
    return []
  }
}

async function start($: EngineInterface, lab: Lab, args: string): Promise<string> {
  if (lab.test !== undefined) return 'замер уже идёт; остановить — `/limit-test stop`'
  const { specs, isWeek } = parseArgs(args)
  const seen = new Set<string>()
  for (const log of await readJournals($)) for (const [, model] of log.bymodel ?? []) seen.add(model)
  const refs = referencesOf(await readResults($))
  const plans = specs.map((s) => planOf(s, seen, refs))
  const points = plans.reduce((s, p) => s + expectedPoints(p, isWeek), 0)
  const usd = plans.reduce((s, p) => s + expectedUsd(p, isWeek), 0)
  const { rateLimits } = await $.session.usage()
  const why = refusal(rateLimits, isWeek, points)
  if (why !== undefined) return `замер не запущен: ${why}.`
  const test: Test = { queue: plans, isWeek, startedAt: await $.clock.now(), entries: [], abort: new AbortController() }
  lab.test = test
  if (lab.wake !== undefined) lab.wake()
  // A module reloaded mid-session has no loop of session.start: the run goes on this call's clock.
  else test.timer = $.clock.after(0, () => void step($, lab))
  return [
    `замер запущен: ${plans.map(label).join(' → ')}${isWeek ? ', с неделей' : ''}.`,
    `Ожидаемо ~${Math.ceil(points)} п. окна 5ч и ~\\$${usd.toFixed(2)} по ценам API. Потолок — втрое больше на модель; срока нет, остановить можно в любой момент; на ${STOP_AT} % окна 5ч замер встаёт.`,
    'Пока идёт замер, не работайте в claude.ai и на телефоне: их траты двигают окна мимо журналов и портят цену. Ходы этой сессии замер пережидает.',
    'Остановить — `/limit-test stop`. Ход замера — строками ниже; когда придёт «замер готов», итог покажет `/limit-test last`.',
  ].join('\n\n')
}

/** The background loop of session.start: drives each run the command hands it, on its own clock. */
async function serve($: EngineInterface, lab: Lab): Promise<void> {
  for (;;) {
    await new Promise<void>((resolve) => {
      lab.wake = resolve
    })
    lab.wake = undefined
    const test = lab.test
    if (test !== undefined) test.timer = $.clock.after(0, () => void step($, lab))
  }
}

/** One subagent's request, answered or not: the answer, or why there is none. */
async function ask($: EngineInterface, lab: Lab, run: ModelRun): Promise<{ answer?: Answer; why?: string }> {
  const s = await $.agent.spawn({
    prompt: testPrompt(run.tokens),
    description: 'limit-test',
    model: run.plan.callModel,
    ...(lab.hasWriter ? { subagentType: WRITER_TYPE } : {}),
  })
  if (s.deny !== undefined) return { why: s.deny }
  // A family never seen is called by its alias: the spawn names the version it resolved to.
  if (run.plan.id === undefined && s.model.startsWith('claude-')) run.plan.id = shortModel(s.model)
  // Core always names the subagent it started. A spawn answered by a hook names none (the test
  // kit's does so by rule): then the next subagent answer on the model it named is taken.
  const key = s.agentId ?? `model:${s.model}`
  const early = lab.early.get(key)
  lab.early.delete(key)
  const answer =
    early ??
    (await new Promise<Answer | undefined>((resolve) => {
      const timer = $.clock.after(ANSWER_MS, () => {
        lab.waiters.delete(key)
        resolve(undefined)
      })
      lab.waiters.set(key, (a) => {
        timer.cancel()
        resolve(a)
      })
    }))
  if (answer === undefined) return { why: 'нет ответа' }
  if (run.plan.id === undefined && answer.model !== undefined) run.plan.id = shortModel(answer.model)
  return answer.output > 0 ? { answer } : { why: 'пустой ответ' }
}

async function step($: EngineInterface, lab: Lab): Promise<void> {
  const t = lab.test
  if (t === undefined) return
  t.timer = undefined
  if (t.abort.signal.aborted) return finish($, lab, t, 'остановлено')
  // A turn of this session spends on its own model: wait it out.
  if (lab.isMainTurn) {
    t.timer = $.clock.after(WAIT_MS, () => void step($, lab))
    return
  }
  try {
    if (t.run === undefined) {
      const plan = t.queue.shift()
      if (plan === undefined) return finish($, lab, t)
      const { rateLimits } = await $.session.usage()
      t.run = startRun(plan, await $.clock.now(), t.isWeek, rateLimits)
      $.ui.log(`${label(plan)} — начало, 5ч ${t.run.last.five_hour ?? '?'} %${t.isWeek ? `, неделя ${t.run.last.seven_day ?? '?'} %` : ''}`)
    }
    const run = t.run
    const before = (await $.session.usage()).cost?.usd
    const out = await ask($, lab, run)
    if (lab.test !== t || t.abort.signal.aborted) return finish($, lab, t, 'остановлено')
    const after = await $.session.usage()
    const now = await $.clock.now()
    const usd = before !== undefined && after.cost !== undefined ? after.cost.usd - before : 0
    if (out.answer === undefined) {
      run.errors += 1
      $.ui.log(`${label(run.plan)} · запрос без ответа (${out.why ?? '?'})`)
      if (run.errors >= MAX_ERRORS) await endModel($, t, run, `ошибки: ${out.why ?? '?'}`)
    } else if (!(usd > 0) || after.cost === undefined) {
      await endModel($, t, run, 'счёт сессии не растёт')
    } else {
      takeAnswer(run, now, usd, after.cost.usd, after.rateLimits)
      run.tokens = nextTokens(usd, out.answer.output, run.plan.usdPerPct.five_hour, run.tokens)
      const week = t.isWeek ? ` · нед ${run.last.seven_day ?? '?'} %` : ''
      $.ui.log(`${label(run.plan)} · запрос ${run.requests} · 5ч ${run.last.five_hour ?? '?'} %${week} · $${usd.toFixed(3)} (итого $${run.spent.toFixed(2)})`)
      const v = verdict(run, now, t.isWeek)
      if (v.isDone) await endModel($, t, run, v.note)
    }
  } catch (err) {
    $.ui.log(`ошибка: ${String(err)}`)
    return finish($, lab, t, 'ошибка')
  }
  if (lab.test === t) t.timer = $.clock.after(0, () => void step($, lab))
}

/** Prices a model's run from its moves, less what other sessions and the rest of this one spent meanwhile. */
async function endModel($: EngineInterface, t: Test, run: ModelRun, note?: string): Promise<void> {
  t.run = undefined
  const others = await readJournals($, await $.session.id())
  const priced = (kind: 'five_hour' | 'seven_day') => {
    const shifts = run.shifts[kind]
    const journalUsd = shifts.length < 2 ? 0 : spentBetween(others, shifts[0]!.t, shifts[shifts.length - 1]!.t)
    return priceOf(shifts, journalUsd, START_PRICE[kind])
  }
  const five = priced('five_hour')
  const week = t.isWeek ? priced('seven_day') : undefined
  const entry = entryOf(run, await $.clock.now(), t.startedAt, five, week, t.isWeek, note)
  t.entries.push(entry)
  const price = entry.usdPerPct5h === undefined ? 'мало данных' : `1 % 5ч = $${entry.usdPerPct5h.toFixed(3)}${entry.isNoisy ? ' (шумно)' : ''}`
  $.ui.log(`${label(run.plan)} — готово: ${price}${note === undefined ? '' : ` (${note})`}`)
}

/** Ends the run: the model under way priced as far as it got, every entry appended to the results. */
async function finish($: EngineInterface, lab: Lab, t: Test, note?: string): Promise<void> {
  if (lab.test !== t) return
  lab.test = undefined
  t.timer?.cancel()
  if (t.run !== undefined) await endModel($, t, t.run, note)
  if (t.entries.length === 0) {
    $.ui.log(`замер ${note ?? 'закончен'}: ни одна модель не измерена`)
    return
  }
  const all = [...(await readResults($)), ...t.entries]
  await $.fs.write(resultsPath($), `${JSON.stringify(all, null, 1)}\n`)
  $.ui.log(`замер готов${note === undefined ? '' : ` (${note})`} — \`/limit-test last\``)
}

async function stop($: EngineInterface, lab: Lab): Promise<string> {
  const t = lab.test
  if (t === undefined) return 'замер не идёт'
  t.abort.abort()
  t.timer?.cancel()
  for (const resolve of lab.waiters.values()) resolve(undefined)
  lab.waiters.clear()
  await finish($, lab, t, 'остановлено')
  return 'замер остановлен; измеренное — `/limit-test last`'
}

export const register: Register = (on) => {
  const lab: Lab = { isMainTurn: false, hasWriter: false, waiters: new Map(), early: new Map() }

  on('session.start', async ($, e, next) => {
    const r = await next(e)
    await $.command.register({
      name: 'limit-test',
      description: 'Замер цены 1 % лимита по моделям: `[модели] [--week]`, `stop`, `last` — итог последнего замера',
    })
    try {
      await $.agent.register({ name: WRITER, description: 'limit-test only: writes the long list it is asked for.', prompt: WRITER_PROMPT, tools: [], omitClaudeMd: true, maxTurns: 1 })
      lab.hasWriter = true
    } catch {
      // Without its own type a request carries the default agent's prompt: dearer, still measured.
    }
    void serve($, lab)
    return r
  })

  // The writer is the mod's own; the model is never offered it.
  on('agent.offer', { agent: 'limit-test:writer' }, () => ({ isOffered: false })).catch(() => ({ isOffered: false }))

  on('turn.start', async ($, e, next) => {
    lab.isMainTurn = true
    return next(e)
  })

  on('turn.complete', async ($, e, next) => {
    const result = await next(e)
    if (e.agentId === undefined) {
      lab.isMainTurn = false
      return result
    }
    const answer = { output: e.usage?.output_tokens ?? 0, model: e.usage?.model }
    const key = lab.waiters.has(e.agentId) ? e.agentId : `model:${e.usage?.model}`
    const waiter = lab.waiters.get(key)
    if (waiter !== undefined) {
      lab.waiters.delete(key)
      waiter(answer)
    } else if (lab.test !== undefined) {
      // Maybe ours, answered before its spawn resolved; dropped with the next request.
      lab.early.clear()
      lab.early.set(e.agentId, answer)
    }
    return result
  })

  on('command.run', { command: 'limit-test' }, async ($, e) => {
    const first = e.args.trim().split(/\s+/)[0]
    if (first === 'stop') return { text: await stop($, lab) }
    if (first === 'last') return { text: formatLast(await readResults($)) }
    return { text: await start($, lab, e.args) }
  })
}
