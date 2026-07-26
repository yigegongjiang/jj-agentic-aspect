'use client';

import { fmtTime } from '@/lib/format';
import type { Project } from '@/lib/types';

interface Props {
  projects: Project[];
  onOpen: (name: string) => void;
  onRename: (project: Project) => void;
  onDelete: (project: Project) => void;
}

export default function ProjectsList({ projects, onOpen, onRename, onDelete }: Props) {
  if (projects.length === 0) {
    return (
      <div className="text-center text-zinc-500 py-16 text-sm">
        (no projects)
      </div>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 2xl:grid-cols-6">
      {projects.map((p) => {
        const taskCount = p.specs.reduce((n, s) => n + s.tasks.length, 0);
        const askCount = p.asks_count;
        return (
          <button
            key={p.name}
            onClick={() => onOpen(p.name)}
            className="group text-left rounded-lg border border-zinc-800 bg-zinc-950 hover:border-zinc-700 hover:bg-zinc-900/60 transition p-4 flex flex-col gap-1"
          >
            <div className="font-medium truncate" title={p.name}>
              {p.name}
            </div>
            <div className="text-xs text-zinc-500">
              {p.specs.length} {p.specs.length === 1 ? 'plan' : 'plans'} ·{' '}
              {taskCount} {taskCount === 1 ? 'task' : 'tasks'} ·{' '}
              {askCount} {askCount === 1 ? 'ask' : 'asks'} ·{' '}
              {p.sessions_count} {p.sessions_count === 1 ? 'session' : 'sessions'}
            </div>
            <div className="mt-1 flex items-center justify-between gap-2">
              <span className="text-[11px] text-zinc-400 font-mono truncate">
                updated {fmtTime(p.updated_at)}
              </span>
              <div className="flex items-center gap-1 shrink-0">
                <span
                  role="button"
                  tabIndex={0}
                  onClick={(e) => {
                    e.stopPropagation();
                    onRename(p);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.stopPropagation();
                      onRename(p);
                    }
                  }}
                  className="px-2 py-0.5 text-[11px] rounded text-zinc-500 hover:text-blue-400 hover:bg-blue-950/40 transition cursor-pointer"
                  aria-label={`rename project ${p.name}`}
                >
                  rename
                </span>
                <span
                  role="button"
                  tabIndex={0}
                  onClick={(e) => {
                    e.stopPropagation();
                    onDelete(p);
                  }}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.stopPropagation();
                      onDelete(p);
                    }
                  }}
                  className="px-2 py-0.5 text-[11px] rounded text-zinc-500 hover:text-red-400 hover:bg-red-950/40 transition cursor-pointer"
                  aria-label={`delete project ${p.name}`}
                >
                  delete
                </span>
              </div>
            </div>
          </button>
        );
      })}
    </div>
  );
}
