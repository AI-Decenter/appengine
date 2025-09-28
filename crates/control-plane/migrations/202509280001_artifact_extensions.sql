-- Migration: extend artifacts schema for next steps (completed_at, idempotency_key, multipart_upload_id)
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS completed_at TIMESTAMPTZ NULL;
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS idempotency_key TEXT UNIQUE NULL;
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS multipart_upload_id TEXT NULL;

-- Audit/event table
CREATE TABLE IF NOT EXISTS artifact_events (
  id BIGSERIAL PRIMARY KEY,
  artifact_id UUID NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_artifact_events_artifact ON artifact_events(artifact_id);
