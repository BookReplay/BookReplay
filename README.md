# BookReplay

BookReplay is a self-hosted Kindle highlights app. It imports `My Clippings.txt`, organizes highlights by book, enriches books with Open Library metadata, and lets you review highlights on a spaced schedule.

## What It Does

- Import Kindle clippings from `My Clippings.txt`
- Browse highlights grouped by book
- Search and match books against Open Library
- Review highlights with a simple scheduling flow
- Authenticate with a single owner account

## Repository Layout

- `src/main.rs` - Rust entrypoint for the API server
- `crates/api` - Axum API, auth, books, clippings, and review logic
- `crates/core` - Shared domain types
- `crates/kindle` - Kindle clippings parsing
- `crates/openlibrary` - Open Library search and enrichment
- `app/web` - SvelteKit frontend
- `migrations` - Database schema migrations
- `compose.yaml` - Local Docker setup with Postgres

## Requirements

- Rust 1.88 or newer
- Node.js 22 or newer
- PostgreSQL 17
- `npm` for the web app

## Local Development

Start the API from the repository root:

```sh
cargo run
```

In a second terminal, run the frontend:

```sh
cd app/web
npm ci
npm run dev
```

The frontend proxies `/api` requests to `http://localhost:3000`.

## Docker

The easiest way to run the full stack locally is:

```sh
docker compose up --build
```

That starts the API and a Postgres database.

## Environment Variables

The Docker setup uses:

- `DATABASE_URL`
- `OPEN_LIBRARY_CONTACT_EMAIL`
- `SESSION_COOKIE_SECURE`

The API may also need additional configuration depending on your deployment environment.

## Web App

The frontend has its own README with app-specific details:

- [app/web/README.md](app/web/README.md)
