-- Additive: new `statuses` table. Append-only log of Claude Code hook events
-- per session, ingested by the jj-status CLI. Rows are immutable (events
-- happened; they don't get edited), so there is no updated_at. body holds the
-- raw hook JSON, truncated client-side to fit the shared body limit.

CREATE TABLE statuses (
  id         TEXT PRIMARY KEY,
  project_id TEXT NOT NULL REFERENCES projects(name) ON DELETE CASCADE,
  session_id TEXT NOT NULL,
  event      TEXT NOT NULL,
  body       TEXT NOT NULL,
  created_at INTEGER NOT NULL
);

-- Every read path is either "sessions of a project" (GROUP BY session_id) or
-- "events of one session" — both served by this composite index.
CREATE INDEX idx_statuses_project_session ON statuses(project_id, session_id);
