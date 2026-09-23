ALTER TABLE owner
    ADD COLUMN revision_streak INTEGER NOT NULL DEFAULT 0 CHECK (revision_streak >= 0),
    ADD COLUMN last_revision_date DATE;
