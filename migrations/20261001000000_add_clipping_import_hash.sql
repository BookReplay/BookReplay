-- Duplicate detection uses the imported text, so editing a highlight no longer
-- makes the next import of the same Kindle file insert it again. Hashing also
-- keeps long highlights within the btree index row limit.
ALTER TABLE clippings ADD COLUMN import_hash TEXT;

UPDATE clippings SET import_hash = md5(content);

ALTER TABLE clippings ALTER COLUMN import_hash SET NOT NULL;
ALTER TABLE clippings DROP CONSTRAINT clippings_user_id_book_id_metadata_content_key;
ALTER TABLE clippings
    ADD CONSTRAINT clippings_import_key UNIQUE (user_id, book_id, metadata, import_hash);

-- Superseded by clippings_user_due_review_idx and clippings_user_new_review_idx.
DROP INDEX clippings_due_review_idx;
DROP INDEX clippings_new_review_idx;
