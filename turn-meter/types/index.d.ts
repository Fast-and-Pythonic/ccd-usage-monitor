/** One finished turn: the reply's final text and the line drawn under it. */
export type TurnNote = { answer: string; line: string }

declare module 'claude-code' {
  interface PluginState {
    'turn-meter': { notes: TurnNote[] }
  }
}
