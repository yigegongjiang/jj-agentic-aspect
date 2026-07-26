// Session 状态推断: 由「最后一条事件 + 距今时长」判定, 卡片列表与详情页共用。
// running = 5min 内仍在产出事件; idle = 回复完成/等人输入; ended = SessionEnd;
// error = 以失败事件收尾且已停止产出。
export type SessionStateKind = 'running' | 'idle' | 'ended' | 'error';

const FRESH_MS = 5 * 60 * 1000;
const FAILURE_EVENTS = new Set(['StopFailure', 'PostToolUseFailure', 'PermissionDenied']);
// 这些事件后 agent 不在干活: 回复已给出 / 会话刚开 / 等人批准
const RESTING_EVENTS = new Set(['Stop', 'SubagentStop', 'SessionStart']);
const NEEDS_HUMAN_EVENTS = new Set(['PermissionRequest', 'Notification']);

export function sessionState(
  lastEvent: string,
  lastAt: number,
  now: number = Date.now(),
): SessionStateKind {
  if (lastEvent === 'SessionEnd') return 'ended';
  if (NEEDS_HUMAN_EVENTS.has(lastEvent) || RESTING_EVENTS.has(lastEvent)) return 'idle';
  if (now - lastAt < FRESH_MS) return 'running';
  return FAILURE_EVENTS.has(lastEvent) ? 'error' : 'idle';
}

export const STATE_STYLE: Record<SessionStateKind, { dot: string; text: string; label: string }> = {
  running: { dot: 'bg-green-400 animate-pulse', text: 'text-green-300', label: 'running' },
  idle: { dot: 'bg-amber-400', text: 'text-amber-300', label: 'idle' },
  ended: { dot: 'bg-zinc-600', text: 'text-zinc-500', label: 'ended' },
  error: { dot: 'bg-red-400', text: 'text-red-300', label: 'error' },
};
