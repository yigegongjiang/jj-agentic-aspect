-- Additive: statuses.source names the agent that emitted the hook event
-- (claude-code today; codex etc. later). Existing rows were all ingested by
-- Claude Code, so the DEFAULT backfills them correctly.

ALTER TABLE statuses ADD COLUMN source TEXT NOT NULL DEFAULT 'claude-code';
