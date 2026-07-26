'use client';

import { useCallback, useEffect, useState } from 'react';

import { ApiError, api } from '@/lib/api';
import { fmtDuration, fmtTime } from '@/lib/format';
import type { Status } from '@/lib/types';

interface Props {
  project: string;
  sessionId: string;
  onUnauthorized: () => void;
}

// 渲染策略: 每条事件提炼「重点」(prompt / tool_name / 关键参数) 直接展示;
// 原始 hook JSON 折叠在 raw 里; 数据量少 (阈值内) 时默认展开全量渲染。
const AUTO_EXPAND_BODY_LEN = 400;

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

  return (
    <div className="space-y-3 max-w-4xl">
      <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1">
        <span className="text-xs font-mono text-zinc-500 break-all">{sessionId}</span>
        {events.length > 0 && (
          <span className="text-xs text-zinc-400 font-mono">
            {events[0].source} · {fmtTime(events[0].created_at)} ·{' '}
            {fmtDuration(events[events.length - 1].created_at - events[0].created_at)} ·{' '}
            {events.length} events
          </span>
        )}
      </div>

      {error && <div className="text-xs text-red-400">{error}</div>}

      {events.length === 0 ? (
        <div className="text-sm text-zinc-400 italic px-4 py-8 text-center">(session not found)</div>
      ) : (
        <ol className="space-y-1.5">
          {events.map((ev) => (
            <EventRow key={ev.id} status={ev} />
          ))}
        </ol>
      )}
    </div>
  );
}

// ---------- per-event digest ----------

type Json = Record<string, unknown>;

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
    case 'PreToolUse':
    case 'PostToolUse':
      return toolDigest(data);
    case 'PostToolUseFailure':
      return joinParts(toolDigest(data), str(data.error));
    case 'PermissionDenied':
      return joinParts(toolDigest(data), str(data.denial_reason));
    case 'Notification':
      return str(data.message);
    case 'SessionStart':
      return str(data.source) && `source: ${str(data.source)}`;
    case 'SessionEnd':
      return str(data.reason) && `reason: ${str(data.reason)}`;
    case 'PreCompact':
      return str(data.trigger) && `trigger: ${str(data.trigger)}`;
    case 'Stop':
    case 'SubagentStop':
      return str(data.last_assistant_message);
    default:
      return null;
  }
}

function joinParts(...parts: Array<string | null>): string | null {
  const kept = parts.filter((p): p is string => p !== null);
  return kept.length > 0 ? kept.join('\n') : null;
}

// 工具调用重点: 各工具最能代表这次调用的一个参数。
function toolDigest(data: Json): string | null {
  const input = (typeof data.tool_input === 'object' && data.tool_input !== null
    ? data.tool_input
    : {}) as Json;
  const parts: string[] = [];
  const primary =
    str(input.command) ??
    str(input.file_path) ??
    str(input.pattern) ??
    str(input.url) ??
    str(input.query) ??
    str(input.description) ??
    str(input.prompt) ??
    str(input.skill);
  if (primary) parts.push(primary);
  return parts.length > 0 ? parts.join(' ') : null;
}

function toolName(data: Json | null): string | null {
  return data ? str(data.tool_name) : null;
}

// badge 配色: 人看时间线时靠颜色扫读事件类型。
function badgeClass(event: string): string {
  switch (event) {
    case 'UserPromptSubmit':
      return 'bg-blue-950/60 text-blue-300 border-blue-900';
    case 'PostToolUse':
    case 'PreToolUse':
      return 'bg-zinc-900 text-zinc-300 border-zinc-800';
    case 'PostToolUseFailure':
    case 'PermissionDenied':
      return 'bg-red-950/60 text-red-300 border-red-900';
    case 'Stop':
    case 'SubagentStop':
      return 'bg-green-950/60 text-green-300 border-green-900';
    case 'Notification':
      return 'bg-amber-950/60 text-amber-300 border-amber-900';
    case 'SessionStart':
    case 'SessionEnd':
      return 'bg-purple-950/60 text-purple-300 border-purple-900';
    case 'PreCompact':
      return 'bg-orange-950/60 text-orange-300 border-orange-900';
    default:
      return 'bg-zinc-900 text-zinc-400 border-zinc-800';
  }
}

function EventRow({ status }: { status: Status }) {
  const data = parseBody(status.body);
  const line = digest(status.event, data);
  const tool = toolName(data);
  // 数据量少 → 直接全渲染; 大 payload 折叠, 点击展开。
  const [showRaw, setShowRaw] = useState(false);
  const small = status.body.length <= AUTO_EXPAND_BODY_LEN;
  const expanded = showRaw || (small && line === null);

  const pretty = data ? JSON.stringify(data, null, 2) : status.body;

  return (
    <li className="rounded-md border border-zinc-800 bg-zinc-950 px-3 py-2">
      <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
        <span className="text-[10px] font-mono text-zinc-500 shrink-0">
          {fmtTime(status.created_at).slice(11)}
        </span>
        <span
          className={`text-[10px] font-mono px-1.5 py-0.5 rounded border shrink-0 ${badgeClass(status.event)}`}
        >
          {status.event}
          {tool ? ` · ${tool}` : ''}
        </span>
        <button
          onClick={() => setShowRaw((v) => !v)}
          className="ml-auto text-[10px] text-zinc-600 hover:text-zinc-300 transition shrink-0"
        >
          {expanded ? '× raw' : '▾ raw'}
        </button>
      </div>
      {line && (
        <div className="mt-1 text-sm leading-snug text-zinc-100 whitespace-pre-wrap break-words line-clamp-[12]">
          {line}
        </div>
      )}
      {expanded && (
        <pre className="mt-1.5 text-[11px] leading-relaxed text-zinc-400 bg-zinc-900/60 rounded p-2 overflow-x-auto max-h-96 overflow-y-auto whitespace-pre-wrap break-all">
          {pretty}
        </pre>
      )}
    </li>
  );
}
