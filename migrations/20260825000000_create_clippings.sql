CREATE TABLE clippings (
    id BIGSERIAL PRIMARY KEY,
    book TEXT NOT NULL,
    metadata TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (book, metadata, content)
);
