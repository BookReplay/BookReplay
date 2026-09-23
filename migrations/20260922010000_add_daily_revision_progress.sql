ALTER TABLE owner
    ADD COLUMN daily_revision_count INTEGER NOT NULL DEFAULT 0 CHECK (daily_revision_count >= 0),
    ADD COLUMN daily_revision_date DATE;
