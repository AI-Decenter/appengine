-- Migration: add public_keys table for persisted signature verification
CREATE TABLE IF NOT EXISTS public_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    app_id UUID NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    public_key_hex CHAR(64) NOT NULL, -- 32-byte ed25519 key hex
    active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_public_keys_app_pub ON public_keys(app_id, public_key_hex);
CREATE INDEX IF NOT EXISTS idx_public_keys_app_active ON public_keys(app_id) WHERE active;