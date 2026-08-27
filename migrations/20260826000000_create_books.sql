CREATE TABLE books (
    id BIGSERIAL PRIMARY KEY,
    kindle_title TEXT NOT NULL UNIQUE,
    title TEXT NOT NULL,
    authors TEXT[] NOT NULL DEFAULT '{}',
    open_library_key TEXT,
    cover_url TEXT,
    first_publish_year INTEGER,
    edition_count INTEGER,
    isbns TEXT[] NOT NULL DEFAULT '{}',
    metadata_checked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO books (kindle_title, title, authors)
SELECT DISTINCT
    book,
    CASE
        WHEN book ~ ' \([^()]*\)$' THEN regexp_replace(book, ' \([^()]*\)$', '')
        ELSE book
    END,
    CASE
        WHEN book !~ ' \([^()]*\)$' OR book ~ ' \(Unknown\)$' THEN '{}'
        ELSE regexp_split_to_array(substring(book FROM ' \(([^()]*)\)$'), '\s*;\s*')
    END
FROM clippings;

ALTER TABLE clippings ADD COLUMN book_id BIGINT REFERENCES books (id);

UPDATE clippings
SET book_id = books.id
FROM books
WHERE clippings.book = books.kindle_title;

ALTER TABLE clippings ALTER COLUMN book_id SET NOT NULL;
ALTER TABLE clippings DROP CONSTRAINT clippings_book_metadata_content_key;
ALTER TABLE clippings DROP COLUMN book;
ALTER TABLE clippings ADD UNIQUE (book_id, metadata, content);

CREATE INDEX clippings_book_id_idx ON clippings (book_id);
