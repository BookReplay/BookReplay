ALTER TABLE clippings ADD COLUMN user_id SMALLINT REFERENCES owner (id);

UPDATE clippings SET user_id = 1;

ALTER TABLE clippings ALTER COLUMN user_id SET NOT NULL;
ALTER TABLE clippings DROP CONSTRAINT clippings_book_id_metadata_content_key;
ALTER TABLE clippings ADD UNIQUE (user_id, book_id, metadata, content);

CREATE INDEX clippings_user_book_id_idx ON clippings (user_id, book_id);
CREATE INDEX clippings_user_due_review_idx
    ON clippings (user_id, next_review_at, id)
    WHERE archived_at IS NULL AND next_review_at IS NOT NULL;
CREATE INDEX clippings_user_new_review_idx
    ON clippings (user_id, id)
    WHERE archived_at IS NULL AND review_count = 0;
