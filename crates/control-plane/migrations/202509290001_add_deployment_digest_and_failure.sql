-- Migration: add digest + failure_reason columns to deployments and deployment_events audit table
ALTER TABLE deployments ADD COLUMN IF NOT EXISTS digest CHAR(64) NULL;
ALTER TABLE deployments ADD COLUMN IF NOT EXISTS failure_reason TEXT NULL;

CREATE INDEX IF NOT EXISTS idx_deployments_status ON deployments(status);

-- Basic audit log for rollouts / failures
CREATE TABLE IF NOT EXISTS deployment_events (
  id BIGSERIAL PRIMARY KEY,
  deployment_id UUID NOT NULL REFERENCES deployments(id) ON DELETE CASCADE,
  event_type TEXT NOT NULL,
  message TEXT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_deployment_events_deployment ON deployment_events(deployment_id);
