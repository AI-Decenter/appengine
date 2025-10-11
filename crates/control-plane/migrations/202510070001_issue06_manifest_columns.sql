-- Issue 06 Phase 3: manifest + SBOM validation columns
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS manifest_digest TEXT;
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS sbom_manifest_digest TEXT;
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS sbom_validated BOOLEAN NOT NULL DEFAULT FALSE;
