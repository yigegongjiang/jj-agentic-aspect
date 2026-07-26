'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';

import { ApiError, api } from '@/lib/api';
import {
  ASK_LIMIT_MAX,
  SESSION_LIST_LIMIT_MAX,
  SPEC_STATUSES,
  TASK_STATUSES,
  type Ask,
  type Project,
  type SessionSummary,
  type Spec,
  type SpecStatus,
  type Task,
  type TaskStatus,
} from '@/lib/types';
import AskEditDialog from './AskEditDialog';
import AskSearch from './AskSearch';
import AsksView from './AsksView';
import ConfirmDialog from './ConfirmDialog';
import EditDialog, { type EditDraft } from './EditDialog';
import ProjectsList from './ProjectsList';
import RenameProjectDialog from './RenameProjectDialog';
import SessionDetail from './SessionDetail';
import SessionsView from './SessionsView';
import SpecDetail from './SpecDetail';
import ProjectTabs from './ProjectTabs';
import SpecsView from './SpecsView';

type EditTarget =
  | { kind: 'spec'; spec: Spec }
  | { kind: 'task'; task: Task }
  | { kind: 'ask'; ask: Ask };

type DeleteTarget =
  | { kind: 'project'; name: string; specCount: number; taskCount: number }
  | { kind: 'spec'; id: string; title: string; taskCount: number }
  | { kind: 'task'; id: string; title: string }
  | { kind: 'ask'; id: string }
  | { kind: 'session'; project: string; sessionId: string; eventsCount: number };

interface RouteHome {
  kind: 'home';
}
interface RouteProject {
  kind: 'project';
  project: string;
}
interface RouteSpec {
  kind: 'spec';
  project: string;
  specId: string;
}
interface RouteSession {
  kind: 'session';
  project: string;
  sessionId: string;
}
type Route = RouteHome | RouteProject | RouteSpec | RouteSession;

function readRoute(): Route {
  if (typeof window === 'undefined') return { kind: 'home' };
  const params = new URLSearchParams(window.location.search);
  const p = params.get('p');
  const s = params.get('s');
  const sess = params.get('sess');
  if (p && sess) return { kind: 'session', project: p, sessionId: sess };
  if (p && s) return { kind: 'spec', project: p, specId: s };
  if (p) return { kind: 'project', project: p };
  return { kind: 'home' };
}

function routeToUrl(route: Route): string {
  if (route.kind === 'home') return './';
  const params = new URLSearchParams();
  params.set('p', route.project);
  if (route.kind === 'spec') params.set('s', route.specId);
  if (route.kind === 'session') params.set('sess', route.sessionId);
  return `./?${params.toString()}`;
}

export default function Dashboard() {
  const [hydrated, setHydrated] = useState(false);
  const [projects, setProjects] = useState<Project[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [edit, setEdit] = useState<EditTarget | null>(null);
  const [editError, setEditError] = useState<string | null>(null);
  const [del, setDel] = useState<DeleteTarget | null>(null);
  const [rename, setRename] = useState<Project | null>(null);
  const [renameError, setRenameError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [route, setRoute] = useState<Route>({ kind: 'home' });
  // Asks lazy-loaded per active project; reset on navigation to avoid stale flashes.
  const [asks, setAsks] = useState<Ask[] | null>(null);
  const [asksProject, setAsksProject] = useState<string | null>(null);
  // Sessions mirror the asks pattern.
  const [sessions, setSessions] = useState<SessionSummary[] | null>(null);
  const [sessionsProject, setSessionsProject] = useState<string | null>(null);

  // Hydrate the route from the URL once on mount. Auth is enforced at the edge
  // by Cloudflare Access (Google SSO); the browser carries the Access session
  // cookie automatically, so there is no token to read and no login form.
  // 自制 SPA 路由: 浏览器不会自动管 scrollY, 用 history.state 自己存,
  // popstate 时双 rAF 等新视图 layout 完成再 scrollTo, 否则会被 clamp.
  useEffect(() => {
    if (typeof window === 'undefined') return;
    setRoute(readRoute());
    setHydrated(true);
    const prevRestoration = window.history.scrollRestoration;
    window.history.scrollRestoration = 'manual';
    const onPop = (e: PopStateEvent) => {
      const target =
        (e.state as { scrollY?: number } | null)?.scrollY ?? 0;
      setRoute(readRoute());
      requestAnimationFrame(() => {
        requestAnimationFrame(() => window.scrollTo(0, target));
      });
    };
    window.addEventListener('popstate', onPop);
    return () => {
      window.removeEventListener('popstate', onPop);
      window.history.scrollRestoration = prevRestoration;
    };
  }, []);

  const navigate = useCallback((next: Route) => {
    if (typeof window !== 'undefined') {
      // 当前 scrollY 写回当前 entry, popstate 回到这里时能读出来恢复.
      const current = (window.history.state as object | null) ?? {};
      window.history.replaceState(
        { ...current, scrollY: window.scrollY },
        '',
      );
      window.history.pushState({ scrollY: 0 }, '', routeToUrl(next));
      window.scrollTo(0, 0);
    }
    setRoute(next);
  }, []);

  // A 401 means the Cloudflare Access session lapsed (or Access is not yet
  // configured). There is no in-app login to fall back to — a full page reload
  // re-triggers Google SSO at the edge — so surface a banner and clear data
  // instead of silently retrying. Also wipes the asks cache so a later refetch
  // does not show the previous session's data.
  const onUnauthorized = useCallback(() => {
    setError(
      '未授权 (401): 会话可能已过期。请刷新页面重新通过 Google 登录; 若持续出现请检查 Cloudflare Access 配置。',
    );
    setProjects(null);
    setEdit(null);
    setDel(null);
    setRename(null);
    setRenameError(null);
    setAsks(null);
    setAsksProject(null);
    setSessions(null);
    setSessionsProject(null);
  }, []);

  // silent=true: background auto-refresh path. Skip the loading/error UI so a
  // 5s tick does not flash spinners or red banners on transient hiccups, but
  // still react to 401 (a lapsed Access session must surface).
  const load = useCallback(
    async (silent = false) => {
      if (!silent) {
        setLoading(true);
        setError(null);
      }
      try {
        const data = await api.listProjects();
        setProjects(data);
      } catch (e) {
        const err = e as ApiError;
        if (err.status === 401) {
          onUnauthorized();
        } else if (silent) {
          console.warn('[auto-refresh]', err);
        } else {
          setError(err.message);
        }
      } finally {
        if (!silent) setLoading(false);
      }
    },
    [onUnauthorized],
  );

  useEffect(() => {
    if (hydrated) void load();
  }, [hydrated, load]);

  // Silent fetch; routes share the projects-list 5s tick.
  const loadAsks = useCallback(
    async (project: string) => {
      try {
        const data = await api.listAsks(project, ASK_LIMIT_MAX);
        setAsks(data);
        setAsksProject(project);
      } catch (e) {
        const err = e as ApiError;
        if (err.status === 401) {
          onUnauthorized();
        } else if (err.status === 404) {
          setAsks([]);
          setAsksProject(project);
        } else {
          console.warn('[asks-load]', err);
        }
      }
    },
    [onUnauthorized],
  );

  // Silent fetch; same error policy as loadAsks.
  const loadSessions = useCallback(
    async (project: string) => {
      try {
        const data = await api.listSessions(project, SESSION_LIST_LIMIT_MAX);
        setSessions(data);
        setSessionsProject(project);
      } catch (e) {
        const err = e as ApiError;
        if (err.status === 401) {
          onUnauthorized();
        } else if (err.status === 404) {
          setSessions([]);
          setSessionsProject(project);
        } else {
          console.warn('[sessions-load]', err);
        }
      }
    },
    [onUnauthorized],
  );

  // asksProject gate prevents refetch when bouncing project ↔ spec routes.
  useEffect(() => {
    if (!hydrated) return;
    if (route.kind === 'home') {
      setAsks(null);
      setAsksProject(null);
      setSessions(null);
      setSessionsProject(null);
      return;
    }
    const project = route.project;
    if (asksProject !== project) {
      setAsks(null);
      void loadAsks(project);
    }
    if (sessionsProject !== project) {
      setSessions(null);
      void loadSessions(project);
    }
  }, [hydrated, route, asksProject, loadAsks, sessionsProject, loadSessions]);

  // Auto-refresh every 5s so AI-side CLI changes show up without a manual
  // reload. Skips ticks when the tab is hidden, then fires immediately on
  // visibilitychange→visible so a returning user sees fresh data without
  // waiting for the next interval.
  useEffect(() => {
    if (!hydrated) return;
    if (typeof document === 'undefined') return;
    const tick = () => {
      if (document.visibilityState !== 'visible') return;
      void load(true);
      if (route.kind !== 'home') {
        void loadAsks(route.project);
        void loadSessions(route.project);
      }
    };
    const intervalId = window.setInterval(tick, 5000);
    document.addEventListener('visibilitychange', tick);
    return () => {
      window.clearInterval(intervalId);
      document.removeEventListener('visibilitychange', tick);
    };
  }, [hydrated, load, route, loadAsks, loadSessions]);

  // 当前路由对应的 project / spec 对象;若数据中已不存在,后面的渲染逻辑回退上一级
  const activeProject = useMemo(() => {
    if (!projects) return null;
    if (route.kind === 'home') return null;
    return projects.find((p) => p.name === route.project) ?? null;
  }, [projects, route]);

  const activeSpec = useMemo(() => {
    if (!activeProject || route.kind !== 'spec') return null;
    return activeProject.specs.find((s) => s.id === route.specId) ?? null;
  }, [activeProject, route]);

  // 路由指向的对象不存在时,自动回退上一级(避免空白页)
  useEffect(() => {
    if (!projects) return;
    if (route.kind === 'project' && !activeProject) {
      navigate({ kind: 'home' });
    } else if (route.kind === 'spec' && activeProject && !activeSpec) {
      navigate({ kind: 'project', project: activeProject.name });
    } else if ((route.kind === 'spec' || route.kind === 'session') && !activeProject) {
      navigate({ kind: 'home' });
    }
  }, [projects, route, activeProject, activeSpec, navigate]);

  async function saveEdit(draft: EditDraft) {
    if (!edit) return;
    setBusy(true);
    setEditError(null);
    try {
      if (edit.kind === 'spec') {
        const patch: { title?: string; body?: string; status?: SpecStatus } = {};
        if (draft.title !== undefined) patch.title = draft.title;
        if (draft.body !== undefined) patch.body = draft.body;
        if (draft.status !== undefined) patch.status = draft.status as SpecStatus;
        const updated = await api.patchSpec(edit.spec.id, patch);
        setProjects((prev) =>
          prev === null
            ? prev
            : prev.map((p) => ({
                ...p,
                specs: p.specs.map((s) =>
                  s.id === updated.id ? { ...s, ...updated, tasks: s.tasks } : s,
                ),
              })),
        );
      } else if (edit.kind === 'task') {
        const patch: { title?: string; body?: string; status?: TaskStatus } = {};
        if (draft.title !== undefined) patch.title = draft.title;
        if (draft.body !== undefined) patch.body = draft.body;
        if (draft.status !== undefined) patch.status = draft.status as TaskStatus;
        const updated = await api.patchTask(edit.task.id, patch);
        setProjects((prev) =>
          prev === null
            ? prev
            : prev.map((p) => ({
                ...p,
                specs: p.specs.map((s) => ({
                  ...s,
                  tasks: s.tasks.map((t) =>
                    t.id === updated.id ? updated : t,
                  ),
                })),
              })),
        );
      }
      setEdit(null);
    } catch (e) {
      const err = e as ApiError;
      if (err.status === 401) {
        onUnauthorized();
      } else {
        setEditError(err.message);
      }
    } finally {
      setBusy(false);
    }
  }

  async function saveAskEdit(newBody: string) {
    if (!edit || edit.kind !== 'ask') return;
    setBusy(true);
    setEditError(null);
    try {
      const updated = await api.patchAsk(edit.ask.id, { body: newBody });
      setAsks((prev) =>
        prev === null ? prev : prev.map((a) => (a.id === updated.id ? updated : a)),
      );
      setEdit(null);
    } catch (e) {
      const err = e as ApiError;
      if (err.status === 401) {
        onUnauthorized();
      } else {
        setEditError(err.message);
      }
    } finally {
      setBusy(false);
    }
  }

  async function submitRename(newName: string) {
    if (!rename) return;
    const oldName = rename.name;
    setBusy(true);
    setRenameError(null);
    try {
      await api.renameProject(oldName, newName);
      // Local state is unreliable after a merge (two projects fold into one);
      // drop the cached projects list and force a refetch instead of trying
      // to splice the result in-place.
      await load(true);
      if (route.kind !== 'home' && route.project === oldName) {
        if (route.kind === 'spec') {
          navigate({ kind: 'spec', project: newName, specId: route.specId });
        } else if (route.kind === 'session') {
          navigate({ kind: 'session', project: newName, sessionId: route.sessionId });
        } else {
          navigate({ kind: 'project', project: newName });
        }
      }
      setRename(null);
    } catch (e) {
      const err = e as ApiError;
      if (err.status === 401) {
        onUnauthorized();
      } else {
        setRenameError(err.message);
      }
    } finally {
      setBusy(false);
    }
  }

  async function confirmDelete() {
    if (!del) return;
    setBusy(true);
    setError(null);
    try {
      if (del.kind === 'project') {
        const name = del.name;
        await api.deleteProject(name);
        setProjects((prev) =>
          prev === null ? prev : prev.filter((p) => p.name !== name),
        );
      } else if (del.kind === 'spec') {
        const id = del.id;
        await api.deleteSpec(id);
        setProjects((prev) =>
          prev === null
            ? prev
            : prev.map((p) => ({
                ...p,
                specs: p.specs.filter((s) => s.id !== id),
              })),
        );
      } else if (del.kind === 'task') {
        const id = del.id;
        await api.deleteTask(id);
        setProjects((prev) =>
          prev === null
            ? prev
            : prev.map((p) => ({
                ...p,
                specs: p.specs.map((s) => ({
                  ...s,
                  tasks: s.tasks.filter((t) => t.id !== id),
                })),
              })),
        );
      } else if (del.kind === 'ask') {
        const id = del.id;
        await api.deleteAsk(id);
        setAsks((prev) => (prev === null ? prev : prev.filter((a) => a.id !== id)));
      } else {
        const { project, sessionId } = del;
        await api.deleteSession(project, sessionId);
        setSessions((prev) =>
          prev === null ? prev : prev.filter((s) => s.session_id !== sessionId),
        );
        if (route.kind === 'session' && route.sessionId === sessionId) {
          navigate({ kind: 'project', project });
        }
      }
      setDel(null);
    } catch (e) {
      const err = e as ApiError;
      if (err.status === 401) {
        onUnauthorized();
      } else {
        setError(err.message);
        setDel(null);
      }
    } finally {
      setBusy(false);
    }
  }

  if (!hydrated) return null;

  return (
    <div className="min-h-screen">
      <header className="bg-zinc-950 border-b border-zinc-800 pt-[env(safe-area-inset-top)]">
        <div className="px-3 sm:px-4 h-12 sm:h-14 flex items-center gap-3">
          <Breadcrumb route={route} activeSpecTitle={activeSpec?.title} onNavigate={navigate} />
          {loading && projects === null && (
            <span className="text-xs text-zinc-500 ml-auto">loading…</span>
          )}
        </div>
      </header>

      <main className="px-3 sm:px-4 py-3 sm:py-6 space-y-4">
        {error && (
          <div className="px-4 py-2.5 rounded-md border border-red-900 bg-red-950/40 text-red-300 text-sm flex items-start justify-between gap-4">
            <span className="break-all">{error}</span>
            <button
              onClick={() => setError(null)}
              className="text-red-400 hover:text-red-300 shrink-0 leading-none"
              aria-label="dismiss"
            >
              ×
            </button>
          </div>
        )}

        {projects === null ? (
          <div className="text-center text-zinc-500 py-12 text-sm">
            loading…
          </div>
        ) : route.kind === 'home' ? (
          <AskSearch
            onOpenProject={(name) => navigate({ kind: 'project', project: name })}
            onUnauthorized={onUnauthorized}
            fallback={
              <ProjectsList
                projects={projects}
                onOpen={(name) => navigate({ kind: 'project', project: name })}
                onRename={(p) => {
                  setRenameError(null);
                  setRename(p);
                }}
                onDelete={(p) => {
                  const taskCount = p.specs.reduce(
                    (n, s) => n + s.tasks.length,
                    0,
                  );
                  setDel({
                    kind: 'project',
                    name: p.name,
                    specCount: p.specs.length,
                    taskCount,
                  });
                }}
              />
            }
          />
        ) : route.kind === 'project' && activeProject ? (
          <ProjectTabs
            askCount={
              asksProject === activeProject.name ? (asks?.length ?? 0) : 0
            }
            specCount={activeProject.specs.length}
            taskCount={activeProject.specs.reduce(
              (n, s) => n + s.tasks.length,
              0,
            )}
            sessionCount={activeProject.sessions_count}
            sessions={
              <SessionsView
                sessions={
                  sessionsProject === activeProject.name ? sessions : null
                }
                onOpen={(sessionId) =>
                  navigate({
                    kind: 'session',
                    project: activeProject.name,
                    sessionId,
                  })
                }
                onDelete={(s) =>
                  setDel({
                    kind: 'session',
                    project: activeProject.name,
                    sessionId: s.session_id,
                    eventsCount: s.events_count,
                  })
                }
              />
            }
            asks={
              <AsksView
                asks={asksProject === activeProject.name ? asks : null}
                onEdit={(ask) => {
                  setEditError(null);
                  setEdit({ kind: 'ask', ask });
                }}
                onDelete={(ask) => setDel({ kind: 'ask', id: ask.id })}
              />
            }
            plans={
              <SpecsView
                project={activeProject}
                onOpenSpec={(specId) =>
                  navigate({
                    kind: 'spec',
                    project: activeProject.name,
                    specId,
                  })
                }
                onEditSpec={(spec) => {
                  setEditError(null);
                  setEdit({ kind: 'spec', spec });
                }}
                onDeleteSpec={(spec) =>
                  setDel({
                    kind: 'spec',
                    id: spec.id,
                    title: spec.title,
                    taskCount: spec.tasks.length,
                  })
                }
              />
            }
          />
        ) : route.kind === 'session' && activeProject ? (
          <SessionDetail
            project={route.project}
            sessionId={route.sessionId}
            onUnauthorized={onUnauthorized}
          />
        ) : route.kind === 'spec' && activeSpec ? (
          <SpecDetail
            spec={activeSpec}
            onEditSpec={(spec) => {
              setEditError(null);
              setEdit({ kind: 'spec', spec });
            }}
            onDeleteSpec={(spec) =>
              setDel({
                kind: 'spec',
                id: spec.id,
                title: spec.title,
                taskCount: spec.tasks.length,
              })
            }
            onEditTask={(task) => {
              setEditError(null);
              setEdit({ kind: 'task', task });
            }}
            onDeleteTask={(task) =>
              setDel({ kind: 'task', id: task.id, title: task.title })
            }
          />
        ) : (
          <div className="text-center text-zinc-500 py-12 text-sm">…</div>
        )}
      </main>

      {edit && edit.kind === 'ask' && (
        <AskEditDialog
          ask={edit.ask}
          busy={busy}
          errorMessage={editError}
          onSave={saveAskEdit}
          onCancel={() => {
            if (!busy) {
              setEdit(null);
              setEditError(null);
            }
          }}
        />
      )}
      {edit && edit.kind !== 'ask' && (
        <EditDialog
          target={
            edit.kind === 'spec'
              ? {
                  kind: 'spec',
                  id: edit.spec.id,
                  title: edit.spec.title,
                  body: edit.spec.body,
                  status: edit.spec.status,
                  statuses: SPEC_STATUSES,
                }
              : {
                  kind: 'task',
                  id: edit.task.id,
                  title: edit.task.title,
                  body: edit.task.body,
                  status: edit.task.status,
                  statuses: TASK_STATUSES,
                }
          }
          busy={busy}
          errorMessage={editError}
          onSave={saveEdit}
          onCancel={() => {
            if (!busy) {
              setEdit(null);
              setEditError(null);
            }
          }}
        />
      )}

      {rename && projects && (
        <RenameProjectDialog
          project={rename}
          existingNames={projects.map((p) => p.name)}
          busy={busy}
          errorMessage={renameError}
          onSubmit={submitRename}
          onCancel={() => {
            if (!busy) {
              setRename(null);
              setRenameError(null);
            }
          }}
        />
      )}

      {del && (
        <ConfirmDialog
          title={
            del.kind === 'project'
              ? `删除 project「${del.name}」?`
              : del.kind === 'spec'
                ? `删除 plan「${del.title}」?`
                : del.kind === 'task'
                  ? `删除 task「${del.title}」?`
                  : del.kind === 'ask'
                    ? `删除 ask「${del.id}」?`
                    : `删除 session「${del.sessionId.slice(0, 8)}」?`
          }
          message={
            del.kind === 'project'
              ? `连同 ${del.specCount} 个 plan, ${del.taskCount} 个 task, 以及该项目下全部 ask 和 session 一并删除,无法恢复。`
              : del.kind === 'spec'
                ? `连同 ${del.taskCount} 个 task 一并删除,无法恢复。`
                : del.kind === 'session'
                  ? `连同 ${del.eventsCount} 条事件一并删除,无法恢复。`
                  : '此操作无法恢复。'
          }
          confirmLabel="删除"
          danger
          busy={busy}
          onConfirm={confirmDelete}
          onCancel={() => {
            if (!busy) setDel(null);
          }}
        />
      )}
    </div>
  );
}

interface BreadcrumbProps {
  route: Route;
  activeSpecTitle?: string;
  onNavigate: (route: Route) => void;
}

function Breadcrumb({ route, activeSpecTitle, onNavigate }: BreadcrumbProps) {
  const segments: { label: string; route: Route | null }[] = [
    { label: 'JJ', route: { kind: 'home' } },
  ];
  if (route.kind !== 'home') {
    segments.push({
      label: route.project,
      route:
        route.kind === 'spec' || route.kind === 'session'
          ? { kind: 'project', project: route.project }
          : null,
    });
  }
  if (route.kind === 'spec') {
    segments.push({ label: activeSpecTitle ?? '…', route: null });
  }
  if (route.kind === 'session') {
    segments.push({ label: route.sessionId.slice(0, 8), route: null });
  }

  return (
    <nav className="flex items-center gap-1.5 text-sm min-w-0">
      {segments.map((seg, idx) => (
        <span key={idx} className="flex items-center gap-1.5 min-w-0">
          {idx > 0 && (
            <span className="text-zinc-500 select-none shrink-0">/</span>
          )}
          {seg.route ? (
            <button
              onClick={() => onNavigate(seg.route!)}
              className={
                idx === 0
                  ? 'font-black tracking-tight text-zinc-100 hover:text-white transition'
                  : 'text-zinc-400 hover:text-zinc-100 transition truncate'
              }
            >
              {seg.label}
            </button>
          ) : (
            <span
              className={
                idx === 0
                  ? 'font-black tracking-tight text-zinc-50'
                  : 'text-zinc-100 truncate'
              }
            >
              {seg.label}
            </span>
          )}
        </span>
      ))}
    </nav>
  );
}
