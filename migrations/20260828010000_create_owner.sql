CREATE TABLE owner (
    id SMALLINT PRIMARY KEY CHECK (id = 1),
    email TEXT NOT NULL,
    name TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
