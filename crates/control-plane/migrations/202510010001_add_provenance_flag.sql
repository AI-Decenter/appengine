-- Migration: add provenance_present column to artifacts table for deterministic tracking
ALTER TABLE artifacts ADD COLUMN IF NOT EXISTS provenance_present BOOLEAN NOT NULL DEFAULT FALSE;