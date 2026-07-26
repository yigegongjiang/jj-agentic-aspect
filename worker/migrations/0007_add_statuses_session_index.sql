-- Session affinity lookup on ingest: find a session's first event across all
-- projects. idx_statuses_project_session leads with project_id and cannot
-- serve a session_id-only probe.
CREATE INDEX idx_statuses_session ON statuses(session_id);
