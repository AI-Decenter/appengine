-- Migration: Add storage_key and status columns for S3-based registry integration
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS storage_key TEXT NULL;
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS status VARCHAR(20) NOT NULL DEFAULT 'stored'; -- existing rows treated as already stored

-- Optional future index for querying pending artifacts
CREATE INDEX IF NOT EXISTS idx_artifacts_status ON artifacts(status);