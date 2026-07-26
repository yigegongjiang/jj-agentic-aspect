import type {
  Ask,
  Project,
  SessionSummary,
  Spec,
  SpecStatus,
  Status,
  Task,
  TaskStatus,
} from './types';

export class ApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = 'ApiError';
  }
}

// No Authorization header: the browser authenticates via the Cloudflare Access
// session cookie (CF_Authorization), sent automatically on same-origin requests
// once the human clears Google SSO at the edge. The Worker validates it. The
// CLI (jj-agentic-aspect) reaches the same Worker via a Cloudflare Access
// service token — every client authenticates through Access; there is no
// bearer path.
async function request<T>(
  method: 'GET' | 'POST' | 'PATCH' | 'DELETE',
  path: string,
  body?: unknown,
): Promise<T> {
  const headers: Record<string, string> = {};
  const init: RequestInit = { method, headers };
  if (body !== undefined) {
    headers['content-type'] = 'application/json';
    init.body = JSON.stringify(body);
  }

  let res: Response;
  try {
    res = await fetch(path, init);
  } catch (e) {
    throw new ApiError(0, (e as Error).message || 'network error');
  }

  if (!res.ok) {
    let message = `HTTP ${res.status}`;
    try {
      const text = await res.text();
      if (text) {
        try {
          const parsed = JSON.parse(text) as { error?: unknown };
          message = typeof parsed.error === 'string' ? parsed.error : text;
        } catch {
          message = text;
        }
      }
    } catch {
      /* ignore body read errors */
    }
    throw new ApiError(res.status, message);
  }

  if (res.status === 204) return null as T;
  return (await res.json()) as T;
}

export interface SpecPatch {
  title?: string;
  body?: string;
  status?: SpecStatus;
}

export interface TaskPatch {
  title?: string;
  body?: string;
  status?: TaskStatus;
}

export const api = {
  listProjects: () => request<Project[]>('GET', '/projects'),

  deleteProject: (name: string) =>
    request<null>('DELETE', `/projects/${encodeURIComponent(name)}`),

  renameProject: (oldName: string, newName: string) =>
    request<Project>('PATCH', `/projects/${encodeURIComponent(oldName)}`, {
      new_name: newName,
    }),

  patchSpec: (id: string, body: SpecPatch) =>
    request<Spec>('PATCH', `/specs/${encodeURIComponent(id)}`, body),

  deleteSpec: (id: string) =>
    request<null>('DELETE', `/specs/${encodeURIComponent(id)}`),

  patchTask: (id: string, body: TaskPatch) =>
    request<Task>('PATCH', `/tasks/${encodeURIComponent(id)}`, body),

  deleteTask: (id: string) =>
    request<null>('DELETE', `/tasks/${encodeURIComponent(id)}`),

  listAsks: (project: string, limit?: number) => {
    const q = limit !== undefined ? `?limit=${limit}` : '';
    return request<Ask[]>('GET', `/projects/${encodeURIComponent(project)}/asks${q}`);
  },

  // Cross-project keyword search over ask bodies. Empty query yields [].
  searchAsks: (query: string, limit?: number) => {
    const params = new URLSearchParams({ q: query });
    if (limit !== undefined) params.set('limit', String(limit));
    return request<Ask[]>('GET', `/asks?${params.toString()}`);
  },

  patchAsk: (id: string, body: { body: string }) =>
    request<Ask>('PATCH', `/asks/${encodeURIComponent(id)}`, body),

  deleteAsk: (id: string) =>
    request<null>('DELETE', `/asks/${encodeURIComponent(id)}`),

  // Agent hook session events (ingested by `jj-agentic-aspect hook`).
  listSessions: (project: string, limit?: number) => {
    const q = limit !== undefined ? `?limit=${limit}` : '';
    return request<SessionSummary[]>(
      'GET',
      `/projects/${encodeURIComponent(project)}/sessions${q}`,
    );
  },

  listSessionStatuses: (project: string, sessionId: string) =>
    request<Status[]>(
      'GET',
      `/projects/${encodeURIComponent(project)}/sessions/${encodeURIComponent(sessionId)}/statuses`,
    ),

  deleteSession: (project: string, sessionId: string) =>
    request<null>(
      'DELETE',
      `/projects/${encodeURIComponent(project)}/sessions/${encodeURIComponent(sessionId)}`,
    ),
};
