'use client';

import { useEffect, useState, type ReactNode } from 'react';

// Exported so other entry points (e.g. an ask-search result click) can preselect
// the tab this component reads on mount.
export const PROJECT_TAB_STORAGE_KEY = 'jj-agentic-aspect_project_tab';
const STORAGE_KEY = PROJECT_TAB_STORAGE_KEY;
type Tab = 'plans' | 'asks' | 'sessions';
const DEFAULT_TAB: Tab = 'plans';

interface Props {
  askCount: number;
  specCount: number;
  taskCount: number;
  sessionCount: number;
  asks: ReactNode;
  plans: ReactNode;
  sessions: ReactNode;
}

export default function ProjectTabs({
  askCount,
  specCount,
  taskCount,
  sessionCount,
  asks,
  plans,
  sessions,
}: Props) {
  const [tab, setTab] = useState<Tab>(DEFAULT_TAB);
  const [hydrated, setHydrated] = useState(false);

  useEffect(() => {
    if (typeof window === 'undefined') return;
    const saved = window.localStorage.getItem(STORAGE_KEY);
    if (saved === 'plans' || saved === 'asks' || saved === 'sessions') setTab(saved);
    setHydrated(true);
  }, []);

  useEffect(() => {
    if (!hydrated) return;
    window.localStorage.setItem(STORAGE_KEY, tab);
  }, [tab, hydrated]);

  return (
    <div className="flex flex-col">
      <div
        role="tablist"
        className="-mx-3 sm:-mx-4 px-3 sm:px-4 flex items-center gap-4 sm:gap-6 border-b border-zinc-800"
      >
        <TabButton
          active={tab === 'plans'}
          onClick={() => setTab('plans')}
          label="PLANS"
          counts={`${specCount} · ${taskCount}`}
          fullCounts={`${specCount} specs · ${taskCount} tasks`}
        />
        <TabButton
          active={tab === 'asks'}
          onClick={() => setTab('asks')}
          label="ASKS"
          counts={`${askCount}`}
          fullCounts={`${askCount}`}
        />
        <TabButton
          active={tab === 'sessions'}
          onClick={() => setTab('sessions')}
          label="SESSIONS"
          counts={`${sessionCount}`}
          fullCounts={`${sessionCount}`}
        />
      </div>
      <div className="pt-3">
        {tab === 'plans' ? plans : tab === 'asks' ? asks : sessions}
      </div>
    </div>
  );
}

interface TabButtonProps {
  active: boolean;
  onClick: () => void;
  label: string;
  counts: string;
  fullCounts: string;
}

function TabButton({
  active,
  onClick,
  label,
  counts,
  fullCounts,
}: TabButtonProps) {
  return (
    <button
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={
        'flex items-baseline gap-2 px-1 py-1.5 sm:py-2 -mb-px border-b-2 transition-colors ' +
        (active
          ? 'border-zinc-100 text-zinc-100'
          : 'border-transparent text-zinc-500 hover:text-zinc-300')
      }
    >
      <span
        className={
          'text-sm uppercase tracking-wider ' +
          (active ? 'font-semibold' : 'font-medium')
        }
      >
        {label}
      </span>
      <span className="text-xs text-zinc-500 font-mono">
        <span className="sm:hidden">{counts}</span>
        <span className="hidden sm:inline">{fullCounts}</span>
      </span>
    </button>
  );
}
