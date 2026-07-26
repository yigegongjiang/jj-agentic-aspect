'use client';

import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from 'react';

import { ApiError, api } from '@/lib/api';
import { fmtTime } from '@/lib/format';
import { ASK_SEARCH_LIMIT_MAX, type Ask } from '@/lib/types';
import { PROJECT_TAB_STORAGE_KEY } from './ProjectTabs';

interface Props {
  onOpenProject: (name: string) => void;
  onUnauthorized: () => void;
  // Shown when the query is empty — the normal home content (projects list).
  fallback: ReactNode;
}

// Homepage entry for cross-project ask search. Typing runs a debounced query
// against GET /asks; an empty query falls back to the projects list so the box
// is purely additive. Results render inline with the matched terms highlighted.
export default function AskSearch({
  onOpenProject,
  onUnauthorized,
  fallback,
}: Props) {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<Ask[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // True while an IME composition is in flight (typing pinyin → 汉字). Gates the
  // search so we never query on half-composed input.
  const [composing, setComposing] = useState(false);
  // Bumped on every keystroke; an in-flight fetch whose id no longer matches is
  // a stale response and gets dropped, so results never arrive out of order.
  const reqIdRef = useRef(0);

  const trimmed = query.trim();
  const terms = useMemo(
    () => (trimmed === '' ? [] : trimmed.split(/\s+/).filter(Boolean)),
    [trimmed],
  );

  useEffect(() => {
    if (trimmed === '') {
      setResults(null);
      setLoading(false);
      setError(null);
      return;
    }
    // Wait out an in-flight IME composition; the effect re-runs (composing dep)
    // when it commits, so the search fires on the finished word, not the pinyin.
    if (composing) return;
    const rid = ++reqIdRef.current;
    setLoading(true);
    setError(null);
    const handle = window.setTimeout(() => {
      api
        .searchAsks(trimmed, ASK_SEARCH_LIMIT_MAX)
        .then((data) => {
          if (rid !== reqIdRef.current) return;
          setResults(data);
          setLoading(false);
        })
        .catch((e) => {
          if (rid !== reqIdRef.current) return;
          const err = e as ApiError;
          if (err.status === 401) {
            onUnauthorized();
            return;
          }
          setError(err.message);
          setLoading(false);
        });
    }, 250);
    return () => window.clearTimeout(handle);
  }, [trimmed, composing, onUnauthorized]);

  function openProject(name: string) {
    // The match lives under the project's ASKS tab; preselect it so the click
    // lands there instead of on the (default) plans tab.
    if (typeof window !== 'undefined') {
      window.localStorage.setItem(PROJECT_TAB_STORAGE_KEY, 'asks');
    }
    onOpenProject(name);
  }

  const capped = results !== null && results.length >= ASK_SEARCH_LIMIT_MAX;

  return (
    <div className="space-y-4">
      <div className="relative">
        <span className="pointer-events-none absolute inset-y-0 left-3 flex items-center text-zinc-500">
          <SearchIcon />
        </span>
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onCompositionStart={() => setComposing(true)}
          onCompositionEnd={(e) => {
            setComposing(false);
            setQuery((e.target as HTMLInputElement).value);
          }}
          onKeyDown={(e) => {
            // Escape also cancels an IME composition; only clear the box when
            // not composing, so backing out of a half-typed word keeps the query.
            if (e.key === 'Escape' && !e.nativeEvent.isComposing) setQuery('');
          }}
          placeholder="检索所有 ask 记录 (关键词, 空格分隔可多词)"
          aria-label="搜索 ask"
          spellCheck={false}
          autoComplete="off"
          className="w-full pl-10 pr-9 py-2.5 rounded-lg bg-zinc-900 border border-zinc-800 focus:border-blue-500 focus:outline-none text-sm placeholder:text-zinc-600"
        />
        {query && (
          <button
            onClick={() => setQuery('')}
            aria-label="清空"
            className="absolute inset-y-0 right-2 flex items-center px-1.5 text-zinc-500 hover:text-zinc-200 transition"
          >
            ×
          </button>
        )}
      </div>

      {trimmed === '' ? (
        fallback
      ) : (
        <SearchResults
          loading={loading}
          error={error}
          results={results}
          terms={terms}
          capped={capped}
          onOpenProject={openProject}
        />
      )}
    </div>
  );
}

interface ResultsProps {
  loading: boolean;
  error: string | null;
  results: Ask[] | null;
  terms: string[];
  capped: boolean;
  onOpenProject: (name: string) => void;
}

function SearchResults({
  loading,
  error,
  results,
  terms,
  capped,
  onOpenProject,
}: ResultsProps) {
  if (error) {
    return (
      <div className="px-4 py-2.5 rounded-md border border-red-900 bg-red-950/40 text-red-300 text-sm break-all">
        {error}
      </div>
    );
  }
  // First query in flight (no prior results to keep on screen).
  if (results === null) {
    return <div className="text-xs text-zinc-500 px-1">searching…</div>;
  }
  if (results.length === 0) {
    return (
      <div className="text-sm text-zinc-400 italic px-4 py-8 text-center">
        no matches
      </div>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex items-center gap-2 px-1 text-[11px] text-zinc-500 font-mono">
        <span>
          {results.length} {results.length === 1 ? 'result' : 'results'}
          {capped ? '+' : ''}
        </span>
        {loading && <span className="text-zinc-600">· searching…</span>}
        {capped && (
          <span className="text-zinc-600">· 显示前 {results.length} 条, 请细化关键词</span>
        )}
      </div>
      <div className="grid gap-2 [grid-template-columns:repeat(auto-fill,minmax(min(22rem,100%),1fr))]">
        {results.map((ask) => (
          <ResultCard
            key={ask.id}
            ask={ask}
            terms={terms}
            onOpenProject={onOpenProject}
          />
        ))}
      </div>
    </div>
  );
}

function ResultCard({
  ask,
  terms,
  onOpenProject,
}: {
  ask: Ask;
  terms: string[];
  onOpenProject: (name: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const mayOverflow = useMemo(() => bodyMayOverflow(ask.body), [ask.body]);
  const content = useMemo(() => highlight(ask.body, terms), [ask.body, terms]);

  return (
    <div className="w-full min-w-0 rounded-lg border border-zinc-800 bg-zinc-950 hover:border-zinc-600 transition p-3 flex flex-col gap-2">
      <div
        className={
          expanded
            ? 'text-sm leading-snug text-zinc-100 whitespace-pre-wrap break-words'
            : 'text-sm leading-snug text-zinc-100 whitespace-pre-wrap break-words line-clamp-6'
        }
      >
        {content}
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
        <button
          onClick={() => onOpenProject(ask.project_id)}
          title={`打开 project「${ask.project_id}」`}
          className="min-w-0 px-1.5 py-0.5 text-[11px] rounded font-mono text-blue-400/90 hover:text-blue-300 hover:bg-blue-950/40 transition truncate"
        >
          {ask.project_id}
        </button>
        <span className="text-[11px] text-zinc-400 font-mono truncate shrink-0">
          {fmtTime(ask.updated_at)}
        </span>
      </div>
    </div>
  );
}

function SearchIcon() {
  return (
    <svg
      width="15"
      height="15"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <circle cx="11" cy="11" r="7" />
      <line x1="21" y1="21" x2="16.65" y2="16.65" />
    </svg>
  );
}

// Wrap every case-insensitive occurrence of any term in <mark>. Terms are
// regex-escaped, so a query like "a.b" highlights the literal text, not a
// wildcard. Built as React nodes — never dangerouslySetInnerHTML.
function highlight(text: string, terms: string[]): ReactNode {
  const escaped = terms.map(escapeRegExp).filter((t) => t.length > 0);
  if (escaped.length === 0) return text;
  const re = new RegExp(`(${escaped.join('|')})`, 'gi');
  // String.split with one capture group interleaves matches at odd indices.
  return text.split(re).map((part, i) =>
    i % 2 === 1 ? (
      <mark
        key={i}
        className="rounded-sm bg-amber-400/25 text-amber-100 px-0.5"
      >
        {part}
      </mark>
    ) : (
      part
    ),
  );
}

function escapeRegExp(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

// Mirror of AsksView: only expose expand when content likely exceeds 6 lines.
function bodyMayOverflow(body: string): boolean {
  if (body.length > 180) return true;
  return body.split('\n').length > 6;
}
