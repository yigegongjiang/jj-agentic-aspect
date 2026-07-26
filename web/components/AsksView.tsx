'use client';

import { useMemo, useState } from 'react';

import { fmtTime } from '@/lib/format';
import type { Ask } from '@/lib/types';

interface Props {
  asks: Ask[] | null;
  onEdit: (ask: Ask) => void;
  onDelete: (ask: Ask) => void;
}

export default function AsksView({ asks, onEdit, onDelete }: Props) {
  if (asks === null) {
    return (
      <section>
        <div className="text-xs text-zinc-500">loading…</div>
      </section>
    );
  }

  if (asks.length === 0) {
    return (
      <section>
        <div className="text-sm text-zinc-400 italic px-4 py-8 text-center">
          (no asks)
        </div>
      </section>
    );
  }

  return (
    <section className="grid gap-2 [grid-template-columns:repeat(auto-fill,minmax(min(20rem,100%),1fr))]">
      {asks.map((ask) => (
        <AskCard
          key={ask.id}
          ask={ask}
          onEdit={() => onEdit(ask)}
          onDelete={() => onDelete(ask)}
        />
      ))}
    </section>
  );
}

function AskCard({
  ask,
  onEdit,
  onDelete,
}: {
  ask: Ask;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  // 仅当内容可能超出 clamp 时才显示 expand 按钮; 短内容隐去, 避免噪声
  const mayOverflow = useMemo(() => bodyMayOverflow(ask.body), [ask.body]);

  return (
    <div className="w-full min-w-0 rounded-lg border border-zinc-800 bg-zinc-950 hover:border-zinc-600 transition p-3 flex flex-col gap-2">
      <div
        className={
          expanded
            ? 'text-sm leading-snug text-zinc-100 whitespace-pre-wrap break-words'
            : 'text-sm leading-snug text-zinc-100 whitespace-pre-wrap break-words line-clamp-6'
        }
      >
        {ask.body}
      </div>
      {mayOverflow && (
        <button
          onClick={() => setExpanded((v) => !v)}
          className="self-start text-[11px] text-zinc-500 hover:text-zinc-200 transition"
        >
          {expanded ? '× collapse' : '▾ expand'}
        </button>
      )}
      <div className="mt-auto pt-1.5 flex items-center justify-between gap-2 border-t border-zinc-900">
        <span className="text-[11px] text-zinc-400 font-mono truncate">
          updated {fmtTime(ask.updated_at)}
        </span>
        <div className="flex items-center gap-0.5 shrink-0">
          <button
            onClick={onEdit}
            className="px-1.5 py-0.5 text-[10px] rounded text-zinc-500 hover:text-zinc-200 hover:bg-zinc-800 transition"
          >
            edit
          </button>
          <button
            onClick={onDelete}
            className="px-1.5 py-0.5 text-[10px] rounded text-zinc-500 hover:text-red-400 hover:bg-red-950/40 transition"
          >
            delete
          </button>
        </div>
      </div>
    </div>
  );
}

// Heuristic: 真正超出 line-clamp-6 时才暴露 expand. 字数/换行任一超阈值即视为可能溢出
function bodyMayOverflow(body: string): boolean {
  if (body.length > 180) return true;
  const lines = body.split('\n').length;
  return lines > 6;
}
