'use client';

import { fmtDuration, fmtRelative } from '@/lib/format';
import { STATE_STYLE, sessionState } from '@/lib/session';
import type { SessionSummary } from '@/lib/types';

interface Props {
  sessions: SessionSummary[] | null;
  // 项目全量 session 数 (来自 /projects 的 COUNT); 列表接口有 limit 上限, 用于截断提示
  total: number;
  onOpen: (sessionId: string) => void;
  onDelete: (session: SessionSummary) => void;
}

export default function SessionsView({ sessions, total, onOpen, onDelete }: Props) {
  if (sessions === null) {
    return (
      <section>
        <div className="text-xs text-zinc-500">loading…</div>
      </section>
    );
  }

  if (sessions.length === 0) {
    return (
      <section>
        <div className="text-sm text-zinc-400 italic px-4 py-8 text-center">
          (no sessions — 配置 agent hooks 指向 `jj-agentic-aspect hook` 后自动记录)
        </div>
      </section>
    );
  }

  return (
    <section>
      <div className="grid gap-2 [grid-template-columns:repeat(auto-fill,minmax(min(20rem,100%),1fr))]">
        {sessions.map((s) => (
          <SessionCard
            key={s.session_id}
            session={s}
            onOpen={() => onOpen(s.session_id)}
            onDelete={() => onDelete(s)}
          />
        ))}
      </div>
      {total > sessions.length && (
        <div className="pt-3 text-center text-[11px] text-zinc-500 font-mono">
          showing latest {sessions.length} of {total}
        </div>
      )}
    </section>
  );
}

function SessionCard({
  session,
  onOpen,
  onDelete,
}: {
  session: SessionSummary;
  onOpen: () => void;
  onDelete: () => void;
}) {
  const state = STATE_STYLE[sessionState(session.last_event, session.last_at)];
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') onOpen();
      }}
      title="open session timeline"
      className="w-full min-w-0 text-left rounded-lg border border-zinc-800 bg-zinc-950 hover:border-zinc-500 hover:bg-zinc-900/50 transition p-3 flex flex-col gap-2 cursor-pointer"
    >
      <div className="text-sm leading-snug text-zinc-100 whitespace-pre-wrap break-words line-clamp-4">
        {session.first_prompt ?? <span className="text-zinc-500 italic">(no prompt)</span>}
      </div>
      <div className="mt-auto pt-1.5 flex items-center justify-between gap-2 border-t border-zinc-900">
        <span className="text-[11px] text-zinc-400 font-mono truncate flex items-center gap-1.5 min-w-0">
          <span className={`w-1.5 h-1.5 rounded-full shrink-0 ${state.dot}`} />
          <span className={`${state.text} shrink-0`}>{state.label}</span>
          <span className="truncate">
            · {fmtRelative(session.last_at)} · <span className="text-zinc-500">{session.source}</span> ·{' '}
            {fmtDuration(session.last_at - session.first_at)} · {session.events_count} ev
          </span>
        </span>
        <div className="flex items-center gap-0.5 shrink-0">
          <span className="text-[10px] text-zinc-600 font-mono" title={session.session_id}>
            {session.session_id.slice(0, 8)}
          </span>
          <span
            role="button"
            tabIndex={0}
            onClick={(e) => {
              e.stopPropagation();
              onDelete();
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.stopPropagation();
                onDelete();
              }
            }}
            className="px-1.5 py-0.5 text-[10px] rounded text-zinc-500 hover:text-red-400 hover:bg-red-950/40 transition cursor-pointer"
            aria-label={`delete session ${session.session_id}`}
          >
            delete
          </span>
        </div>
      </div>
    </div>
  );
}
