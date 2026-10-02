-- One row per import that reached the handler, whether or not anything was stored,
-- so the share of imports that work can be measured.
CREATE TABLE import_attempts (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    parsed INTEGER NOT NULL CHECK (parsed >= 0),
    inserted INTEGER NOT NULL CHECK (inserted >= 0),
    -- 'empty': no highlight was found in the file. 'failed': the highlights could not be stored.
    outcome TEXT NOT NULL CHECK (outcome IN ('stored', 'empty', 'failed'))
);

CREATE INDEX import_attempts_user_id_idx ON import_attempts (user_id);
