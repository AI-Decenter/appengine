-- Migration: add artifacts table for Issue 02
CREATE TABLE IF NOT EXISTS artifacts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    app_id UUID NULL REFERENCES applications(id) ON DELETE SET NULL,
    digest CHAR(64) NOT NULL UNIQUE,
    size_bytes BIGINT NOT NULL,
    signature TEXT NULL,
    sbom_url VARCHAR(1024) NULL,
    manifest_url VARCHAR(1024) NULL,
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_artifacts_digest ON artifacts(digest);