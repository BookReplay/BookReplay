ALTER TABLE clippings
    ADD COLUMN first_seen_at TIMESTAMPTZ,
    ADD COLUMN last_reviewed_at TIMESTAMPTZ,
    ADD COLUMN next_review_at TIMESTAMPTZ,
    ADD COLUMN review_count INTEGER NOT NULL DEFAULT 0 CHECK (review_count >= 0),
    ADD COLUMN current_interval_days INTEGER NOT NULL DEFAULT 0 CHECK (current_interval_days >= 0),
    ADD COLUMN archived_at TIMESTAMPTZ;

CREATE TABLE highlight_reviews (
    id BIGSERIAL PRIMARY KEY,
    highlight_id BIGINT NOT NULL REFERENCES clippings (id) ON DELETE CASCADE,
    reviewed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    rating TEXT NOT NULL CHECK (rating IN ('SOON', 'LATER', 'MUCH_LATER', 'ARCHIVE')),
    previous_interval_days INTEGER NOT NULL CHECK (previous_interval_days >= 0),
    next_interval_days INTEGER CHECK (next_interval_days >= 0)
);

CREATE INDEX clippings_due_review_idx
    ON clippings (next_review_at, id)
    WHERE archived_at IS NULL AND next_review_at IS NOT NULL;

CREATE INDEX clippings_new_review_idx
    ON clippings (id)
    WHERE archived_at IS NULL AND review_count = 0;

CREATE INDEX highlight_reviews_highlight_idx
    ON highlight_reviews (highlight_id, reviewed_at DESC);
