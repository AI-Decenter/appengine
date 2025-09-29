ALTER TABLE deployments ADD COLUMN last_transition_at timestamptz DEFAULT now();
UPDATE deployments SET last_transition_at = created_at;
