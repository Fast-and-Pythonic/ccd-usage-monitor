import { expect, test } from 'claude-code/testing'

import { exeFromStatusLine, payload } from './register'

// The figures of `SAMPLE` in src/state.rs, as the engine hands them over: the
// bridge's own test parses that JSON, so matching it here closes the loop.
const context = { tokens: 380000, window: 1000000, percent: 38 }
const rateLimits = [
  { kind: 'five_hour', percentUsed: 23.5, resetsAt: '2025-02-01T16:00:00.000Z' },
  { kind: 'seven_day', percentUsed: 41.2, resetsAt: '2025-02-06T16:00:00.000Z' },
]
const SAMPLE = {
  session_id: 'abc',
  model: { id: 'claude-opus-5' },
  context_window: { used_percentage: 38, context_window_size: 1000000, total_input_tokens: 380000 },
  rate_limits: {
    five_hour: { used_percentage: 23.5, resets_at: 1738425600 },
    seven_day: { used_percentage: 41.2, resets_at: 1738857600 },
  },
}

test('the payload is the status line JSON the bridge parses', () => {
  expect(payload('abc', 'claude-opus-5', context, rateLimits)).toEqual(SAMPLE)
})

test('a figure the engine does not have is left out, never zeroed', () => {
  expect(payload('abc', 'm', { window: 200000 }, [])).toEqual({
    session_id: 'abc',
    model: { id: 'm' },
    context_window: { context_window_size: 200000 },
  })
})

test('the executable is read out of a bridge status line, quoted or not', () => {
  expect(exeFromStatusLine('C:/x/ccd-usage-monitor.exe --bridge')).toBe('C:/x/ccd-usage-monitor.exe')
  expect(exeFromStatusLine('"C:/Program Files/c/ccd.exe" --bridge')).toBe('C:/Program Files/c/ccd.exe')
  expect(exeFromStatusLine('npx ccstatusline')).toBe(undefined)
})

test('a measurement in the desktop app feeds the bridge, then pins the line', { options: { exe: 'C:/x/ccd.exe', showLine: true } }, async ($, on) => {
  const runs: { argv: readonly string[]; stdin?: string }[] = []
  const pinned: (string | undefined)[] = []
  on('session.surfaces', () => ({ value: ['desktop'] }))
  on('session.id', () => ({ value: 'abc' }))
  on('session.model', () => ({ value: 'claude-opus-5' }))
  on('process.run', ($, e) => {
    runs.push({ argv: e.argv, stdin: e.init?.stdin })
    const stdout = e.argv.includes('--line') ? '24% ▼1.1× 3:37 | ctx 38%\r\n' : ''
    return { value: { exitCode: 0, stdout, stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('ui.status', ($, e) => {
    pinned.push(e.text)
    return { value: undefined }
  })
  on('session.measure', ($, e) => ({ changed: e.changed }))
  await $.session.measure({ context, rateLimits, changed: ['context', 'rateLimits'] })
  expect(runs.map(r => r.argv)).toEqual([
    ['C:/x/ccd.exe', '--bridge'],
    ['C:/x/ccd.exe', '--line', '--session', 'abc'],
  ])
  expect(JSON.parse(runs[0]?.stdin ?? '')).toEqual(SAMPLE)
  expect(pinned).toEqual(['24% ▼1.1× 3:37 | ctx 38%'])
})

test('in a terminal the status line already does all of it, so nothing runs', { options: { exe: 'C:/x/ccd.exe' } }, async ($, on) => {
  let ran = 0
  on('session.surfaces', () => ({ value: ['terminal'] }))
  on('process.run', () => {
    ran += 1
    return { value: { exitCode: 0, stdout: '', stderr: '', isStdoutTruncated: false, isStderrTruncated: false } }
  })
  on('session.measure', ($, e) => ({ changed: e.changed }))
  await $.session.measure({ context, rateLimits, changed: ['context'] })
  expect(ran).toBe(0)
})
