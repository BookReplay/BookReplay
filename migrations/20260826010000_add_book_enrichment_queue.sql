ALTER TABLE books
    ADD COLUMN retry_count INTEGER NOT NULL DEFAULT 0 CHECK (retry_count >= 0),
    ADD COLUMN next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ADD COLUMN last_error TEXT;

CREATE INDEX books_pending_enrichment_idx
    ON books (next_attempt_at, id)
    WHERE metadata_checked_at IS NULL;
