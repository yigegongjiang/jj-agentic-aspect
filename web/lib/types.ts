export const SPEC_STATUSES = ['active', 'done'] as const;
export const TASK_STATUSES = ['todo', 'doing', 'done', 'blocked'] as const;

export type SpecStatus = (typeof SPEC_STATUSES)[number];
export type TaskStatus = (typeof TASK_STATUSES)[number];

export const MAX_TITLE_LEN = 200;
export const MAX_BODY_LEN = 65536;
export const MAX_PROJECT_NAME_LEN = 128;

export interface Task {
  id: string;
  spec_id: string;
  title: string;
  body: string;
  status: TaskStatus;
  prev_id: string | null;
  created_at: number;
  updated_at: number;
}

export interface Spec {
  id: string;
  project_id: string;
  title: string;
  body: string;
  status: SpecStatus;
  prev_id: string | null;
  created_at: number;
  updated_at: number;
  tasks: Task[];
}

export interface Project {
  name: string;
  created_at: number;
  updated_at: number;
  specs: Spec[];
  asks_count: number;
  sessions_count: number;
}

export const ASK_LIMIT_DEFAULT = 3;
export const ASK_LIMIT_MAX = 100;

// Cross-project ask search (GET /asks?q=). Mirrors the worker's search bounds.
export const ASK_SEARCH_LIMIT_DEFAULT = 50;
export const ASK_SEARCH_LIMIT_MAX = 200;

export interface Ask {
  id: string;
  project_id: string;
  body: string;
  created_at: number;
  updated_at: number;
}

// Claude Code hook session events (GET /projects/:name/sessions[...]).
export const SESSION_LIST_LIMIT_DEFAULT = 50;
export const SESSION_LIST_LIMIT_MAX = 200;

export interface SessionSummary {
  session_id: string;
  events_count: number;
  first_at: number;
  last_at: number;
  first_prompt: string | null;
}

export interface Status {
  id: string;
  project_id: string;
  session_id: string;
  event: string;
  body: string;
  created_at: number;
}
