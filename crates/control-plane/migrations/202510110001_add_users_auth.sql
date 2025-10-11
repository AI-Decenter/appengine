-- Up
CREATE TABLE IF NOT EXISTS users (
	id UUID PRIMARY KEY,
	name TEXT NULL,
	role TEXT NOT NULL CHECK (role IN ('admin','reader')),
	token_hash TEXT UNIQUE NULL,
	created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Down
-- DROP TABLE IF EXISTS users;
