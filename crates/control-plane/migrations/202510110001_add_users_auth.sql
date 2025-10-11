-- Users table for token-based auth (Issue 10)
CREATE EXTENSION IF NOT EXISTS pgcrypto;
CREATE TABLE IF NOT EXISTS users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email TEXT,
  token_hash TEXT UNIQUE NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('admin','user')),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
