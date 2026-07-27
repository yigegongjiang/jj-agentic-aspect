'use client';

import { useCallback, useEffect, useState } from 'react';

import Markdown from '@/components/Markdown';
import { ApiError, api } from '@/lib/api';
import { fmtDuration, fmtRelative, fmtTime } from '@/lib/format';
import { looksLikeMarkdown } from '@/lib/markdown';
import { STATE_STYLE, sessionState } from '@/lib/session';
import type { Status } from '@/lib/types';

interface Props {
  project: string;
  sessionId: string;
  onUnauthorized: () => void;
}

// 渲染策略: 信息分层, 而非平铺原始事件。
// - 按 turn 分组: UserPromptSubmit 开新轮, prompt(蓝) 与 assistant 回复(绿) 是重点, 大块展示
// - MessageDisplay (assistant 中间进度文本) 按 message_id 合并成淡色块; 与 Stop 最终回复重复的丢弃
// - Pre/PostToolUse 按 tool_use_id 配对合并成一行: 工具名 + 关键参数 + 时长 + 成败
// - 错误 (PostToolUseFailure/PermissionDenied/StopFailure) 红色常显, 不折叠
// - 顶部概览条: 状态 / model / 时长 / 轮次 / 工具数 / 错误数, 一眼知进度
// - 任何一行点击可展开原始 hook JSON
export default function SessionDetail({ project, sessionId, onUnauthorized }: Props) {
  const [events, setEvents] = useState<Status[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      const data = await api.listSessionStatuses(project, sessionId);
      setEvents(data);
      setError(null);
    } catch (e) {
      const err = e as ApiError;
      if (err.status === 401) {
        onUnauthorized();
      } else if (err.status === 404) {
        setEvents([]);
      } else {
        setError(err.message);
      }
    }
  }, [project, sessionId, onUnauthorized]);

  // Initial fetch + the same 5s visible-tab tick the rest of the dashboard uses,
  // so a live session's events stream in while you watch.
  useEffect(() => {
    setEvents(null);
    void load();
    const tick = () => {
      if (document.visibilityState !== 'visible') return;
      void load();
    };
    const intervalId = window.setInterval(tick, 5000);
    document.addEventListener('visibilitychange', tick);
    return () => {
      window.clearInterval(intervalId);
      document.removeEventListener('visibilitychange', tick);
    };
  }, [load]);

  if (events === null) {
    return <div className="text-xs text-zinc-500">loading…</div>;
  }

  if (events.length === 0) {
    return (
      <div className="space-y-3">
        <span className="text-xs font-mono text-zinc-500 break-all">{sessionId}</span>
        {error && <div className="text-xs text-red-400">{error}</div>}
        <div className="text-sm text-zinc-400 italic px-4 py-8 text-center">(session not found)</div>
      </div>
    );
  }

  const parsed = events.map((s) => ({ status: s, data: parseBody(s.body) }));
  const { turns, toolCount, errorCount } = buildTimeline(parsed);
  const promptTurns = turns.filter((t) => t.prompt !== null).length;
  const first = events[0];
  const last = events[events.length - 1];
  const state = sessionState(last.event, last.created_at);
  const stateStyle = STATE_STYLE[state];
  const model = parsed.map((p) => str(p.data?.model)).find((m) => m !== null) ?? null;
  let promptNo = 0;

  return (
    <div className="space-y-3">
      {/* 概览条: 进度与关键指标一眼可见 */}
      <div className="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-[11px] font-mono">
        <span className={`flex items-center gap-1.5 ${stateStyle.text}`}>
          <span className={`w-2 h-2 rounded-full ${stateStyle.dot}`} />
          {stateStyle.label}
        </span>
        <span className="text-zinc-400">{first.source}</span>
        {model && <span className="text-zinc-400">{model}</span>}
        <span className="text-zinc-500">{fmtTime(first.created_at)}</span>
        <span className="text-zinc-500">{fmtDuration(last.created_at - first.created_at)}</span>
        <span className="text-zinc-400">
          {promptTurns} {promptTurns === 1 ? 'turn' : 'turns'} · {toolCount} tools ·{' '}
          {events.length} events
        </span>
        {errorCount > 0 && (
          <span className="text-red-300 font-semibold">{errorCount} errors</span>
        )}
        {state === 'running' && (
          <span className="text-green-400/80">active {fmtRelative(last.created_at)}</span>
        )}
        <span className="ml-auto text-zinc-700 break-all" title={sessionId}>
          {sessionId.slice(0, 8)}
        </span>
      </div>

      {error && <div className="text-xs text-red-400">{error}</div>}

      <div className="space-y-4">
        {turns.map((turn, ti) => {
          if (turn.prompt) promptNo += 1;
          if (!turn.prompt && turn.items.length === 0) return null;
          return (
            <TurnBlock
              key={turn.prompt?.status.id ?? `pre-${ti}`}
              turn={turn}
              no={turn.prompt ? promptNo : null}
              isLast={ti === turns.length - 1}
              live={state === 'running'}
            />
          );
        })}
      </div>
    </div>
  );
}

// ---------- timeline model ----------

type Json = Record<string, unknown>;

interface PE {
  status: Status;
  data: Json | null;
}

interface ToolItem {
  kind: 'tool';
  key: string;
  pre: PE | null;
  post: PE | null; // PostToolUse / PostToolUseFailure / PermissionDenied
  failed: boolean;
  at: number;
  durationMs: number | null;
}

interface EventItem {
  kind: 'event';
  key: string;
  at: number;
  ev: PE;
}

// MessageDisplay 流式分片按 message_id 合并成一个进度块
interface MsgItem {
  kind: 'msg';
  key: string;
  at: number;
  messageId: string | null;
  chunks: { index: number | null; delta: string; pe: PE }[];
  final: boolean;
}

type Item = ToolItem | EventItem | MsgItem;

// 分片按 index 排序拼全文 (hook 异步执行, 到达顺序可能乱序)
function msgText(item: MsgItem): string {
  return [...item.chunks]
    .sort((a, b) => (a.index ?? Number.MAX_SAFE_INTEGER) - (b.index ?? Number.MAX_SAFE_INTEGER))
    .map((c) => c.delta)
    .join('');
}

// CLI 截断标记会插在文本尾部; 去重比较时剥掉再取前缀
function normForCompare(s: string): string {
  return s.replace(/…\[truncated, \d+ chars total\]/g, '').trim().slice(0, 300);
}

interface Turn {
  prompt: PE | null; // UserPromptSubmit; null = 首个 prompt 前的开场事件
  items: Item[];
}

function buildTimeline(events: PE[]): { turns: Turn[]; toolCount: number; errorCount: number } {
  const turns: Turn[] = [];
  let current: Turn = { prompt: null, items: [] };
  turns.push(current);
  // tool_use_id → 未完成的调用; Post 到达时配对合并
  const pending = new Map<string, ToolItem>();
  let toolCount = 0;
  let errorCount = 0;

  for (const pe of events) {
    const e = pe.status.event;
    if (e === 'UserPromptSubmit') {
      current = { prompt: pe, items: [] };
      turns.push(current);
      continue;
    }
    if (e === 'PreToolUse') {
      const item: ToolItem = {
        kind: 'tool',
        key: pe.status.id,
        pre: pe,
        post: null,
        failed: false,
        at: pe.status.created_at,
        durationMs: null,
      };
      toolCount += 1;
      const tuid = str(pe.data?.tool_use_id);
      if (tuid) pending.set(tuid, item);
      current.items.push(item);
      continue;
    }
    if (e === 'MessageDisplay') {
      const mid = str(pe.data?.message_id);
      const deltaRaw = pe.data?.delta;
      const delta = typeof deltaRaw === 'string' ? deltaRaw : '';
      const idxRaw = pe.data?.index;
      const idx = typeof idxRaw === 'number' ? idxRaw : null;
      const isFinal = pe.data?.final === true;
      const last = current.items[current.items.length - 1];
      if (last && last.kind === 'msg' && mid !== null && last.messageId === mid) {
        last.chunks.push({ index: idx, delta, pe });
        last.final = last.final || isFinal;
      } else {
        current.items.push({
          kind: 'msg',
          key: pe.status.id,
          at: pe.status.created_at,
          messageId: mid,
          chunks: [{ index: idx, delta, pe }],
          final: isFinal,
        });
      }
      continue;
    }
    if (e === 'Stop') {
      // 末条进度块 = 最终回复本身 (Stop 绿块已展示) → 去重丢弃
      const finalText = str(pe.data?.last_assistant_message);
      for (let i = current.items.length - 1; i >= 0; i--) {
        const it = current.items[i];
        if (it.kind !== 'msg') continue;
        if (finalText !== null) {
          const a = normForCompare(msgText(it));
          const b = normForCompare(finalText);
          if (a.length > 0 && (a.startsWith(b) || b.startsWith(a))) current.items.splice(i, 1);
        }
        break;
      }
      current.items.push({ kind: 'event', key: pe.status.id, at: pe.status.created_at, ev: pe });
      continue;
    }
    if (e === 'PostToolUse' || e === 'PostToolUseFailure' || e === 'PermissionDenied') {
      const failed = e !== 'PostToolUse';
      if (failed) errorCount += 1;
      const tuid = str(pe.data?.tool_use_id);
      const open = tuid ? pending.get(tuid) : undefined;
      if (open && open.post === null) {
        open.post = pe;
        open.failed = failed;
        const d = pe.data?.duration_ms;
        open.durationMs =
          typeof d === 'number' ? d : pe.status.created_at - open.at;
        if (tuid) pending.delete(tuid);
      } else {
        // 无配对 Pre (旧数据 / 缺 hook): 独立成行
        toolCount += 1;
        const d = pe.data?.duration_ms;
        current.items.push({
          kind: 'tool',
          key: pe.status.id,
          pre: null,
          post: pe,
          failed,
          at: pe.status.created_at,
          durationMs: typeof d === 'number' ? d : null,
        });
      }
      continue;
    }
    if (e === 'StopFailure') errorCount += 1;
    current.items.push({ kind: 'event', key: pe.status.id, at: pe.status.created_at, ev: pe });
  }
  return { turns, toolCount, errorCount };
}

// ---------- per-event digest ----------

function parseBody(body: string): Json | null {
  try {
    const v = JSON.parse(body) as unknown;
    return typeof v === 'object' && v !== null && !Array.isArray(v) ? (v as Json) : null;
  } catch {
    return null;
  }
}

function str(v: unknown): string | null {
  return typeof v === 'string' && v.length > 0 ? v : null;
}

// 每类事件的「重点」: 一句话 digest。取不到关键字段时回退 raw JSON。
// 字段兼容: UserPromptSubmit 新版 user_input / 旧版 prompt。
function digest(event: string, data: Json | null): string | null {
  if (!data) return null;
  switch (event) {
    case 'UserPromptSubmit':
      return str(data.prompt) ?? str(data.user_input);
    case 'Notification':
      return str(data.message);
    case 'PermissionRequest':
      return joinParts(toolName(data), toolDigest(data));
    case 'SessionStart':
      return joinParts(str(data.source) && `source: ${str(data.source)}`, str(data.model));
    case 'SessionEnd':
      return str(data.reason) && `reason: ${str(data.reason)}`;
    case 'PreCompact':
    case 'PostCompact':
      return str(data.trigger) && `trigger: ${str(data.trigger)}`;
    case 'SubagentStart':
      return str(data.agent_type) ?? str(data.description) ?? str(data.prompt);
    case 'Stop':
    case 'SubagentStop':
      return str(data.last_assistant_message);
    case 'StopFailure':
      return str(data.error) ?? str(data.reason) ?? str(data.message);
    case 'UserPromptExpansion':
      return str(data.command) ?? str(data.prompt);
    case 'CwdChanged':
      return str(data.new_cwd) ?? str(data.cwd);
    case 'WorktreeCreate':
    case 'WorktreeRemove':
      return str(data.worktree_path) ?? str(data.path);
    case 'TaskCreated':
    case 'TaskCompleted':
      return str(data.title) ?? str(data.description) ?? str(data.task_id);
    default:
      // 新增/未知事件的通用兜底: 常见字段里挑一个可读的, 取不到再回退 raw JSON
      return (
        str(data.message) ??
        str(data.title) ??
        str(data.file_path) ??
        str(data.path) ??
        str(data.command) ??
        str(data.reason) ??
        null
      );
  }
}

function joinParts(...parts: Array<string | null>): string | null {
  const kept = parts.filter((p): p is string => p !== null);
  return kept.length > 0 ? kept.join(' · ') : null;
}

// 工具调用重点: 各工具最能代表这次调用的一个参数。
function toolDigest(data: Json | null): string | null {
  if (!data) return null;
  const input = (typeof data.tool_input === 'object' && data.tool_input !== null
    ? data.tool_input
    : {}) as Json;
  return (
    str(input.command) ??
    str(input.file_path) ??
    str(input.pattern) ??
    str(input.url) ??
    str(input.query) ??
    str(input.description) ??
    str(input.prompt) ??
    str(input.skill)
  );
}

function toolName(data: Json | null): string | null {
  return data ? str(data.tool_name) : null;
}

function toolError(data: Json | null): string | null {
  if (!data) return null;
  return str(data.error) ?? str(data.denial_reason) ?? str(data.message);
}

function fmtMs(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 10000) return `${(ms / 1000).toFixed(1)}s`;
  return fmtDuration(ms);
}

function clock(ms: number): string {
  return fmtTime(ms).slice(11);
}

function prettyJson(pe: PE): string {
  return pe.data ? JSON.stringify(pe.data, null, 2) : pe.status.body;
}

// ---------- rendering ----------

// 单个 turn 内工具行超过此数且非最后一轮 → 默认折叠 (prompt/回复/错误仍常显)
const TURN_COLLAPSE_THRESHOLD = 20;

// 折叠态下仍必须可见的重要事件
function isKeyEvent(item: Item): boolean {
  if (item.kind === 'tool') return item.failed;
  if (item.kind === 'msg') return false;
  const e = item.ev.status.event;
  return e === 'Stop' || e === 'StopFailure' || e === 'PermissionDenied';
}

// 活跃指示文案: 由最后一个 item 推断当前阶段 (thinking 无 hook 事件, 只能推断)
function liveLabel(items: Item[]): string {
  const last = items[items.length - 1];
  if (!last) return 'thinking…';
  if (last.kind === 'msg') return last.final ? 'thinking…' : 'writing…';
  if (last.kind === 'tool') return last.post === null ? 'working…' : 'thinking…';
  return 'working…';
}

function TurnBlock({
  turn,
  no,
  isLast,
  live,
}: {
  turn: Turn;
  no: number | null;
  isLast: boolean;
  live: boolean;
}) {
  const collapsible = !isLast && turn.items.length > TURN_COLLAPSE_THRESHOLD;
  const [open, setOpen] = useState(!collapsible);
  const shown = open ? turn.items : turn.items.filter(isKeyEvent);
  const hidden = turn.items.length - shown.length;
  const turnEnd = turn.items.length > 0 ? turn.items[turn.items.length - 1].at : null;
  const turnStart = turn.prompt?.status.created_at ?? turn.items[0]?.at ?? null;

  return (
    <section>
      {turn.prompt && (
        <PromptBlock
          pe={turn.prompt}
          no={no ?? 0}
          duration={
            turnStart !== null && turnEnd !== null && turnEnd > turnStart
              ? fmtDuration(turnEnd - turnStart)
              : null
          }
        />
      )}
      {(turn.items.length > 0 || (turn.prompt && isLast)) && (
        <div className={`space-y-0.5 ${turn.prompt ? 'mt-1.5 ml-1.5 pl-3 border-l border-zinc-800/80' : ''}`}>
          {collapsible && (
            <button
              onClick={() => setOpen((v) => !v)}
              className="text-[11px] font-mono text-zinc-500 hover:text-zinc-300 transition py-0.5"
            >
              {open ? '▾' : '▸'} {turn.items.length} steps
              {!open && hidden > 0 ? ` (${hidden} hidden)` : ''}
            </button>
          )}
          {shown.map((item) =>
            item.kind === 'tool' ? (
              <ToolRow key={item.key} item={item} live={live && isLast} />
            ) : item.kind === 'msg' ? (
              <MsgBlock key={item.key} item={item} />
            ) : (
              <EventBlock key={item.key} pe={item.ev} />
            ),
          )}
          {/* 最后一轮还没有 Stop → 正在工作 */}
          {isLast &&
            live &&
            !turn.items.some(
              (it) => it.kind === 'event' && it.ev.status.event === 'Stop',
            ) && (
              <div className="flex items-center gap-2 py-1 text-[11px] font-mono text-green-400/80">
                <span className="w-1.5 h-1.5 rounded-full bg-green-400 animate-pulse" />
                {liveLabel(turn.items)}
              </div>
            )}
        </div>
      )}
    </section>
  );
}

// 用户 prompt: 蓝色重点块
function PromptBlock({ pe, no, duration }: { pe: PE; no: number; duration: string | null }) {
  const text = digest('UserPromptSubmit', pe.data);
  return (
    <ExpandableBlock
      pe={pe}
      text={text}
      clampClass="line-clamp-6"
      className="border-l-2 border-blue-500 bg-blue-950/25 rounded-r-md"
      header={
        <>
          <span className="text-blue-300 font-semibold">#{no} prompt</span>
          <span className="text-zinc-500">{clock(pe.status.created_at)}</span>
          {duration && <span className="text-zinc-500">→ {duration}</span>}
        </>
      }
      textClass="text-zinc-50"
    />
  );
}

// 工具调用: 一行 = 时间 + 成败 + 工具名 + 关键参数 + 时长; 点击展开 pre/post raw
function ToolRow({ item, live }: { item: ToolItem; live: boolean }) {
  const [open, setOpen] = useState(false);
  const data = item.pre?.data ?? item.post?.data ?? null;
  const name = toolName(data) ?? 'tool';
  const line = (toolDigest(data) ?? '').split('\n')[0];
  const running = item.post === null;
  const err = item.failed ? toolError(item.post?.data ?? null) : null;

  return (
    <div>
      <div
        role="button"
        tabIndex={0}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') setOpen((v) => !v);
        }}
        className="group flex items-baseline gap-2 py-px px-1 -mx-1 rounded cursor-pointer hover:bg-zinc-900/70 transition min-w-0"
      >
        <span className="text-[10px] font-mono text-zinc-600 shrink-0">{clock(item.at)}</span>
        {running ? (
          <span
            className={`w-1.5 h-1.5 rounded-full shrink-0 self-center ${live ? 'bg-cyan-400 animate-pulse' : 'bg-zinc-700'}`}
          />
        ) : item.failed ? (
          <span className="text-[11px] font-mono text-red-400 shrink-0 font-bold">✗</span>
        ) : (
          <span className="text-[11px] font-mono text-zinc-600 shrink-0">✓</span>
        )}
        <span
          className={`text-xs font-mono shrink-0 ${item.failed ? 'text-red-300' : 'text-zinc-300'}`}
        >
          {name}
        </span>
        <span className="text-xs text-zinc-500 truncate min-w-0 flex-1">{line}</span>
        {item.durationMs !== null && item.durationMs >= 0 && (
          <span className="text-[10px] font-mono text-zinc-600 shrink-0">
            {fmtMs(item.durationMs)}
          </span>
        )}
        <span className="text-[10px] font-mono text-zinc-600 group-hover:text-zinc-300 transition shrink-0">
          {open ? '▾' : '▸'}
        </span>
      </div>
      {err && (
        <div className="ml-6 text-xs text-red-300/90 whitespace-pre-wrap break-words line-clamp-4">
          {err}
        </div>
      )}
      {open && (
        <div className="ml-6 my-1 space-y-1">
          {item.pre && <RawPre label="PreToolUse" pe={item.pre} />}
          {item.post && <RawPre label={item.post.status.event} pe={item.post} />}
        </div>
      )}
    </div>
  );
}

// 非工具事件: Stop=assistant 回复(绿), StopFailure(红), 开场/收尾=灰单行, 其余=badge 行
function EventBlock({ pe }: { pe: PE }) {
  const e = pe.status.event;
  if (e === 'Stop') {
    return (
      <ExpandableBlock
        pe={pe}
        text={digest(e, pe.data)}
        clampClass="line-clamp-[12]"
        mdClampClass="max-h-80"
        markdown
        className="border-l-2 border-green-500 bg-green-950/20 rounded-r-md my-1.5"
        header={
          <>
            <span className="text-green-300 font-semibold">assistant</span>
            <span className="text-zinc-500">{clock(pe.status.created_at)}</span>
          </>
        }
        textClass="text-zinc-100"
      />
    );
  }
  if (e === 'StopFailure') {
    return (
      <ExpandableBlock
        pe={pe}
        text={digest(e, pe.data)}
        clampClass="line-clamp-[12]"
        className="border-l-2 border-red-500 bg-red-950/25 rounded-r-md my-1.5"
        header={
          <>
            <span className="text-red-300 font-semibold">stop failure</span>
            <span className="text-zinc-500">{clock(pe.status.created_at)}</span>
          </>
        }
        textClass="text-red-200"
      />
    );
  }
  return <MinorEventRow pe={pe} />;
}

// badge 配色: 人扫时间线时靠颜色识别事件类型。
function badgeClass(event: string): string {
  switch (event) {
    case 'PermissionDenied':
      return 'bg-red-950/60 text-red-300 border-red-900';
    case 'SubagentStop':
      return 'bg-green-950/60 text-green-300 border-green-900';
    case 'Notification':
    case 'PermissionRequest':
    case 'Elicitation':
    case 'TeammateIdle':
      return 'bg-amber-950/60 text-amber-300 border-amber-900';
    case 'SessionStart':
    case 'SessionEnd':
      return 'bg-zinc-900 text-zinc-500 border-zinc-800';
    case 'SubagentStart':
    case 'WorktreeCreate':
    case 'WorktreeRemove':
      return 'bg-cyan-950/60 text-cyan-300 border-cyan-900';
    case 'TaskCreated':
      return 'bg-violet-950/60 text-violet-300 border-violet-900';
    case 'TaskCompleted':
      return 'bg-green-950/60 text-green-300 border-green-900';
    case 'UserPromptExpansion':
      return 'bg-blue-950/60 text-blue-300 border-blue-900';
    case 'PreCompact':
    case 'PostCompact':
      return 'bg-orange-950/60 text-orange-300 border-orange-900';
    default:
      return 'bg-zinc-900 text-zinc-400 border-zinc-800';
  }
}

function MinorEventRow({ pe }: { pe: PE }) {
  const [open, setOpen] = useState(false);
  const e = pe.status.event;
  const line = digest(e, pe.data);
  return (
    <div>
      <div
        role="button"
        tabIndex={0}
        onClick={() => setOpen((v) => !v)}
        onKeyDown={(ev) => {
          if (ev.key === 'Enter' || ev.key === ' ') setOpen((v) => !v);
        }}
        className="group flex items-baseline gap-2 py-px px-1 -mx-1 rounded cursor-pointer hover:bg-zinc-900/70 transition min-w-0"
      >
        <span className="text-[10px] font-mono text-zinc-600 shrink-0">
          {clock(pe.status.created_at)}
        </span>
        <span
          className={`text-[10px] font-mono px-1.5 py-0.5 rounded border shrink-0 ${badgeClass(e)}`}
        >
          {e}
        </span>
        <span className="text-xs text-zinc-400 truncate min-w-0 flex-1">
          {line?.split('\n')[0] ?? ''}
        </span>
        <span className="text-[10px] font-mono text-zinc-600 group-hover:text-zinc-300 transition shrink-0">
          {open ? '▾' : '▸'}
        </span>
      </div>
      {open && (
        <div className="ml-6 my-1">
          <RawPre label={e} pe={pe} />
        </div>
      )}
    </div>
  );
}

// assistant 中间进度文本 (MessageDisplay 分片合并): 淡色块, 与最终回复(绿)区分
function MsgBlock({ item }: { item: MsgItem }) {
  const text = msgText(item);
  return (
    <ExpandableBlock
      rawText={JSON.stringify(
        item.chunks.map((c) => c.pe.data ?? c.pe.status.body),
        null,
        2,
      )}
      text={text.length > 0 ? text : null}
      clampClass="line-clamp-6"
      mdClampClass="max-h-44"
      markdown
      className="border-l-2 border-zinc-700 bg-zinc-900/40 rounded-r-md my-1"
      header={
        <>
          <span className="text-zinc-400 font-semibold">assistant · progress</span>
          <span className="text-zinc-600">{clock(item.at)}</span>
          {!item.final && <span className="text-zinc-500">streaming…</span>}
        </>
      }
      textClass="text-zinc-400"
    />
  );
}

// 重点内容块 (prompt / assistant 回复 / 进度): 默认 clamp, 点击展开全文, raw 按钮看原始 JSON。
// markdown=true 时内容命中 markdown 构造则默认渲染, `↔ text` 切回原始文本。
function ExpandableBlock({
  pe,
  rawText,
  text,
  clampClass,
  mdClampClass = 'max-h-64',
  markdown = false,
  className,
  header,
  textClass,
}: {
  pe?: PE;
  rawText?: string;
  text: string | null;
  clampClass: string;
  mdClampClass?: string;
  markdown?: boolean;
  className: string;
  header: React.ReactNode;
  textClass: string;
}) {
  const [full, setFull] = useState(false);
  const [raw, setRaw] = useState(false);
  const [asText, setAsText] = useState(false);
  const isMd = markdown && text !== null && looksLikeMarkdown(text);
  const renderMd = isMd && !asText;
  const clampable = text !== null && (text.length > 400 || text.split('\n').length > 6);
  return (
    <div className={`px-3 py-2 ${className}`}>
      <div className="flex items-baseline gap-2 text-[10px] font-mono">
        {header}
        <span className="ml-auto flex items-baseline gap-2 shrink-0">
          {isMd && (
            <button
              onClick={() => setAsText((v) => !v)}
              title={asText ? 'render as markdown' : 'show raw markdown source'}
              className="text-zinc-600 hover:text-zinc-300 transition"
            >
              {asText ? '↔ md' : '↔ text'}
            </button>
          )}
          <button
            onClick={() => setRaw((v) => !v)}
            title="hook event JSON"
            className="text-zinc-600 hover:text-zinc-300 transition"
          >
            {raw ? '× raw' : '▾ raw'}
          </button>
        </span>
      </div>
      {text !== null && (
        <>
          {renderMd ? (
            // markdown 是块级布局, line-clamp 会破坏它 → 改用 max-height 截断
            <div
              className={`mt-1 ${textClass} ${full || !clampable ? '' : `${mdClampClass} overflow-hidden`}`}
            >
              <Markdown text={text} />
            </div>
          ) : (
            <div
              role={clampable ? 'button' : undefined}
              tabIndex={clampable ? 0 : undefined}
              onClick={clampable ? () => setFull((v) => !v) : undefined}
              onKeyDown={
                clampable
                  ? (e) => {
                      if (e.key === 'Enter' || e.key === ' ') setFull((v) => !v);
                    }
                  : undefined
              }
              className={`mt-1 text-sm leading-snug whitespace-pre-wrap break-words ${textClass} ${full ? '' : clampClass} ${clampable ? 'cursor-pointer' : ''}`}
            >
              {text}
            </div>
          )}
          {/* 明示可展开: 隐形点击区之外给一个常显按钮 */}
          {clampable && (
            <button
              onClick={() => setFull((v) => !v)}
              className="mt-1 text-[10px] font-mono text-zinc-500 hover:text-zinc-200 transition"
            >
              {full ? '▴ less' : '▾ more'}
            </button>
          )}
        </>
      )}
      {(raw || text === null) && (
        <pre className="mt-1.5 text-[11px] leading-relaxed text-zinc-400 bg-zinc-900/60 rounded p-2 overflow-x-auto max-h-96 overflow-y-auto whitespace-pre-wrap break-all">
          {rawText ?? (pe ? prettyJson(pe) : '')}
        </pre>
      )}
    </div>
  );
}

function RawPre({ label, pe }: { label: string; pe: PE }) {
  return (
    <div>
      <div className="text-[10px] font-mono text-zinc-600">{label}</div>
      <pre className="text-[11px] leading-relaxed text-zinc-400 bg-zinc-900/60 rounded p-2 overflow-x-auto max-h-96 overflow-y-auto whitespace-pre-wrap break-all">
        {prettyJson(pe)}
      </pre>
    </div>
  );
}
