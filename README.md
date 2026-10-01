# BookReplay

BookReplay is a self-hosted Kindle highlights app. Import `My Clippings.txt`, browse highlights by book, match books with Open Library and optional Google Books metadata, and review highlights on a spaced schedule. Each instance has one owner account.

The source is available under the [PolyForm Noncommercial License 1.0.0](LICENSE): you may run, study and modify it for noncommercial purposes. That is not an OSI-approved open-source licence, and commercial use is not permitted.

## Installation status

The first prebuilt release has **not been published yet**. The production configuration targets `ghcr.io/loukag/bookreplay:v0.1.0`; that tag is a planned release, not an available download. Until it is published, use the source-build instructions below. The repository and its packages may require GitHub access; maintainers must make the release package public before advertising anonymous installation.

The initial container target is **Linux x86-64 (`linux/amd64`) only**. ARM64, including Raspberry Pi and native Apple Silicon, is not yet supported or tested. Use one application instance with PostgreSQL 17. Deployment needs Docker Engine and Docker Compose 2.24.4 or newer; running a prebuilt image needs neither Rust nor Node.js.

## Quick start

1. Obtain `compose.yaml` and `.env.example` from the same release into a dedicated directory, or clone this repository to build from source. Run all Compose commands from that directory.
2. Configure the instance:

   ```sh
   cp .env.example .env
   chmod 600 .env
   openssl rand -hex 32
   ```

   Edit `.env` and paste the generated value into `POSTGRES_PASSWORD`. Generate a **second** value with `openssl rand -hex 32` for `SETUP_SECRET`; registration is disabled without it. Leave the `${POSTGRES_PASSWORD}` reference in `DATABASE_URL`; Compose expands it automatically. Set `BOOKREPLAY_IMAGE` to the published version you selected (or an immutable `image@sha256:…` digest). An empty database password or URL makes Compose fail before starting containers. Optionally set `OPEN_LIBRARY_CONTACT_EMAIL` to your contact address.
3. For a published release:

   ```sh
   docker compose pull
   docker compose up -d --wait
   docker compose logs --tail=50 bookreplay
   ```

   **Until the first release is published**, build the checked-out source instead:

   ```sh
   docker compose -f compose.yaml -f compose.dev.yaml up -d --build --wait
   docker compose logs --tail=50 bookreplay
   ```

   Use both `-f` arguments for subsequent commands on a source-built stack. The development override also exposes PostgreSQL on `127.0.0.1:5432`.
4. Open **http://localhost:2665**. `docker compose up -d --wait` returns once migrations have run and the application answers its health probe (`/healthz`, which needs no login and checks database access). For a remote server, run this on your own computer and use the same browser URL:

   ```sh
   ssh -N -L 2665:127.0.0.1:2665 user@your-server
   ```

   Compose publishes the application only on the server's loopback interface. Register with the setup secret, name, email, and a password of 12–128 bytes, then log in. Remove `SETUP_SECRET` from `.env` after registration and recreate the app with `docker compose up -d --force-recreate bookreplay`. For remote access, follow the tested [HTTPS setup and owner recovery guide](deploy/SECURITY.md), setting both `APP_ORIGIN=https://your-domain` and `SESSION_COOKIE_SECURE=true` before exposing the instance.
5. Open **Import**, select your Kindle's `documents/My Clippings.txt` (up to 16 MiB), and import it. Importing the same file again later adds only new highlights, including ones you have edited since. Return to the library to verify the highlights. Metadata enrichment happens in the background and needs outbound access to Open Library.
6. Verify persistence:

   ```sh
   docker compose restart
   ```

   After the app is listening again, refresh the library and confirm the highlights remain. For source builds, include both `-f` arguments as above.

## Storage, shutdown, and restart

The named volume **`bookreplay_postgres_data`** stores the owner, password hash, highlights, book metadata, review state, and sessions. Inside PostgreSQL it is mounted at `/var/lib/postgresql/data`. Docker owns the host path; inspect it with `docker volume inspect bookreplay_postgres_data`. The name changes if you override the Compose project name. Keep the project name stable when replacing containers. The application has no persistent filesystem volume; the original Kindle file stays on your own device. Preserve `.env` separately as private configuration.

On `docker compose stop` the application stops accepting connections and finishes the requests already in progress, including a running import, before exiting. Docker still kills it after its stop timeout (10 seconds by default) if a request takes longer.

```sh
docker compose stop       # Stop without removing containers or data.
docker compose start      # Resume a stopped stack.
docker compose down       # Remove containers/network; preserve the database volume.
docker compose up -d      # Recreate containers using the same database volume.
```

**`docker compose down -v` deletes the database volume and your library.** Take a backup first; see [Backups, restore, and upgrades](#backups-restore-and-upgrades). Replacing containers or pulling an image does not delete it. Changing `POSTGRES_PASSWORD` in `.env` does not change the password in an already initialized database; change the PostgreSQL role password and URL together. Do not delete the volume to resolve a credential mismatch.

Both services use `restart: unless-stopped`. Enable Docker at boot using your host's service manager. A running stack should return when Docker starts; containers explicitly stopped stay stopped. After installing, reboot the server during a suitable maintenance window, reconnect, run `docker compose ps`, and check your highlights. A real host-reboot test remains a release acceptance step; a container restart alone does not verify it. See [Docker's restart policy documentation](https://docs.docker.com/engine/containers/start-containers-automatically/).

## Environment variables

Compose reads `.env` beside `compose.yaml`. The Rust process does **not** load `.env` by itself.

| Variable | Default / required value |
| --- | --- |
| `BOOKREPLAY_IMAGE` | Compose default: `ghcr.io/loukag/bookreplay:v0.1.0` (pending first publication). Select a published version tag or digest; avoid `latest`. The development override always builds `localhost/bookreplay:dev`. |
| `POSTGRES_PASSWORD` | Required in Compose, no default. Generate a random hex password. Used by PostgreSQL only when initializing an empty volume. |
| `DATABASE_URL` | Required, no API default. PostgreSQL URI such as `postgresql://bookreplay:PASSWORD@postgres:5432/bookreplay`. The example expands `POSTGRES_PASSWORD`; its database/user must match Compose. For arbitrary passwords, percent-encode reserved URI characters in the URL, while leaving the PostgreSQL password literal. Hex avoids both URI escaping and Compose `$` interpolation issues. |
| `OPEN_LIBRARY_CONTACT_EMAIL` | Optional; default empty. A contact email included in the HTTP User-Agent of metadata requests. |
| `GOOGLE_BOOKS_API_KEY` | Optional; default empty. Enables Google Books as a second metadata source; see [Book metadata sources](#book-metadata-sources). |
| `METADATA_ENRICHMENT` | Exactly `true` or `false`; default `true`. `false` stops the background metadata worker and the **Identify book** search, so no book title or author leaves the instance. Books keep their Kindle title and have no cover. Books imported meanwhile stay queued and are looked up if you turn it back on. |
| `CLIENT_IP_HEADER` | Optional; default empty. Name of the header in which your reverse proxy reports the client address, e.g. `X-Forwarded-For` with the supplied Caddyfile. Set it only when the application is reachable through that proxy alone; see the [operator guide](deploy/SECURITY.md#request-protection-and-limits). |
| `SESSION_COOKIE_SECURE` | Exactly `true` or `false`; default `false`. Use `false` for localhost or SSH-tunnel HTTP and `true` for HTTPS. A secure cookie cannot authenticate an ordinary HTTP connection. |
| `APP_ORIGIN` | Default `http://localhost:2665`. Exact browser origin, no trailing slash. Required on all writes via the `Origin` header. Remote origins require HTTPS and secure cookies. For Vite use `http://localhost:5173`. |
| `SETUP_SECRET` | Required only to create the first owner; no default. Generate with `openssl rand -hex 32` (32–128 bytes). Empty disables setup. Remove after setup and recreate the app. Registration remains closed once an owner exists. |
| `BIND_ADDR` | Listening address. The binary defaults to `127.0.0.1:2665`; the container image sets `0.0.0.0:2665` and Compose publishes that port on the host's loopback interface only. Change it only when running the binary outside the supplied image. |
| `RUST_LOG` | Default `bookreplay_api=info`. Application tracing filter, e.g. `warn,bookreplay_api=info,bookreplay_openlibrary=info`; levels include `off`, `error`, `warn`, `info`, `debug`, `trace`. An invalid filter falls back to the API default. Dependency events are suppressed to protect session and request data. |

The production database has no published host port. It is accessible to the application as `postgres:5432` on the Compose network. Use `docker compose exec postgres psql -U bookreplay -d bookreplay` for operator access, or the development override for a local database client.

## Backups, restore, and upgrades

Everything worth keeping is in PostgreSQL: the owner account, highlights and your edits, book matches, review history and sessions. Keep `.env` as well; it holds the database password. Covers and the web UI are rebuildable. A backup is private library data: store it off the server and protect it like the instance itself.

Back up with PostgreSQL's own tool while the stack is running. Copying the volume's files from a live database is not a reliable backup.

```sh
docker compose exec -T postgres pg_dump -U bookreplay -d bookreplay -Fc > bookreplay-$(date +%F).dump
```

Restore into the same or a new instance. On a new machine, copy `compose.yaml` and `.env`, use the same PostgreSQL major version (17) and an application version at least as new as the one that made the backup, then:

```sh
docker compose up -d --wait postgres
docker compose stop bookreplay
docker compose exec -T postgres pg_restore -U bookreplay -d bookreplay --clean --if-exists < bookreplay-2026-10-01.dump
docker compose up -d --wait
```

The dump includes login sessions that were valid when it was taken. To sign every browser out after a restore, run `docker compose exec postgres psql -U bookreplay -d bookreplay -c 'TRUNCATE tower_sessions.session'`.

To upgrade:

1. Read the release notes for the version you are moving to.
2. Take a backup as above. Database migrations run automatically when the new version starts and are not reversible.
3. Set `BOOKREPLAY_IMAGE` in `.env` to the new version, then run `docker compose pull` and `docker compose up -d --wait`.
4. Check `docker compose ps` shows the application as healthy and that your library opens.

If the new version fails to start, `docker compose logs bookreplay` names the failing step. To go back, restore the pre-upgrade backup and set `BOOKREPLAY_IMAGE` to the previous version; anything written after the backup is lost. Moving to a new PostgreSQL major version is a separate operation: dump with the old version, start an empty volume on the new one, and restore.

## Resources and installation checks

The repeatable installation check is:

```sh
docker compose -f compose.yaml -f compose.dev.yaml build
python3 scripts/smoke-install.py localhost/bookreplay:dev
```

It starts an isolated project with a random password, an ephemeral localhost port, and a disposable database volume. It verifies the web UI, owner creation/login, 1,000 synthetic highlights, session and highlight persistence after restarting and replacing both containers, and duplicate import. Each service is limited to 0.5 CPU and 256 MiB RAM. It reports import time, sampled memory use, and database size, then removes only its own test volume. It never reads your `.env` or imports personal clippings. Python 3 is needed for this check only.

Measured on 2026-09-21 using Linux amd64, rootless Podman with the Docker-compatible API, and Docker Compose 5.5.1:

| Measurement | Result |
| --- | --- |
| Tested service budget | 1 CPU total; 512 MiB RAM total (0.5 CPU / 256 MiB per service, enforced) |
| Import | 1,000 synthetic highlights in one book, 0.27 seconds |
| Memory sampled after replacement | Application 22.54 MiB; PostgreSQL 25.89 MiB (not peak usage) |
| PostgreSQL data directory after the test | 46.43 MiB |
| Application image, uncompressed | 98.93 MiB |

For an initial small-library deployment, provision at least 1 vCPU, 1 GiB host RAM, and 2 GiB free disk **as an estimate**, plus space for your library and backups. The tested container budget above is not a measured minimum for an entire host; allow additional memory and disk for the operating system, Docker, and image downloads. This one-book fixture does not establish large-library limits. A clean Docker host installation, whole-host minimum resource measurement, and host-reboot validation remain pending in [the checklist](SELF_HOSTING_CHECKLIST.md). Source compilation requires substantially more resources than running a prebuilt image.

## Local development

Install Rust 1.88+, Node.js 22+, npm, and Docker with Compose. Prepare `.env` as above, then start just the database with the development port override:

```sh
docker compose -f compose.yaml -f compose.dev.yaml up -d --wait postgres
```

From the repository root, replace `YOUR_HEX_PASSWORD` with the same password from `.env`, using **127.0.0.1** as the database host:

```sh
export DATABASE_URL='postgresql://bookreplay:YOUR_HEX_PASSWORD@127.0.0.1:5432/bookreplay'
export SESSION_COOKIE_SECURE=false
export APP_ORIGIN=http://localhost:5173
export SETUP_SECRET='YOUR_SEPARATE_RANDOM_SETUP_SECRET'
cargo run --locked
```

In another terminal:

```sh
cd app/web
npm ci
npm run dev
```

Open the URL printed by Vite (normally http://localhost:5173). Vite proxies `/api` to http://localhost:2665. `cargo run` listens on `127.0.0.1:2665` only; set `BIND_ADDR` to change that.

Before opening a pull request, run the checks CI runs: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked -- --include-ignored` (with `DATABASE_URL` pointing at a disposable PostgreSQL server whose role may create databases), and `npm run check && npm run build` in `app/web`. Stop a containerized app before running `cargo run` on the same port. The containerized database persists between development runs.

For a complete source build with automatic rebuilds instead:

```sh
docker compose -f compose.yaml -f compose.dev.yaml up --build --watch
```

The override preserves host-network builds for development. Production uses only a prebuilt image, without watch settings or a host database port.

## Publishing an image

[The release workflow](.github/workflows/release-image.yml) builds `linux/amd64`, runs the installation/persistence check, and only then pushes that same image to `ghcr.io/<owner>/<repository>:<tag>` on `v*` tag pushes. A manual workflow run tests without publishing. Publish a tag matching the application version, e.g. `v0.1.0`, after reviewing the release. GitHub Actions needs package-write permission. Do not move published version tags. For anonymous installation, make the GHCR package public and verify `docker compose pull` with no registry credentials on a clean machine before announcing the release. This workflow follows Docker's [test-before-push approach](https://docs.docker.com/build/ci/github-actions/test-before-push/).

## Repository layout

- `src/main.rs`: Rust entrypoint
- `crates/api`: Axum API, owner authentication, books, highlights, reviews
- `crates/core`, `crates/kindle`, `crates/openlibrary`: shared types, parsing, metadata
- `app/web`: SvelteKit frontend ([frontend README](app/web/README.md))
- `migrations`: database migrations, applied automatically at application startup
- `compose.yaml`: production configuration; `compose.dev.yaml`: source build/watch and local database access

## Book metadata sources

New imports automatically search Open Library using title and author, retrying with
the title alone and without the subtitle when needed. Candidates are ranked by
title wording, author similarity, and available metadata; Open Library wins ties.
Titles are searched in their original language. The best available result is
selected automatically, so use **Identify book** to correct an incorrect match.

Optionally enable Google Books by enabling the Books API in your Google Cloud
project, creating an API key, and setting `GOOGLE_BOOKS_API_KEY` in `.env`.
Google [documents API keys for public requests](https://developers.google.com/books/docs/v1/using).
Compose passes this key only to the server. Recreate the application container
after changing it (`docker compose up -d --force-recreate bookreplay`).
Without a key, only Open Library is used. With a key, Google is searched when
Open Library fails, has an imperfect title match, or lacks a cover or authors.
Manual identification searches both enabled sources and labels each result.

Existing completed books are not requeued; existing pending imports continue
processing. Manual identification also uses the upgraded search. Missing fields
preserve stored values; candidates supplement one another only with a shared ISBN.
Google edition dates are never stored as first publication years.

What leaves your instance: the server sends each imported book's title and author to Open Library (and to Google Books when a key is set), and your browser loads cover images directly from `covers.openlibrary.org`, `archive.org` and Google Books, which reveals your IP address to those hosts. Highlight text is never sent anywhere. Set `METADATA_ENRICHMENT=false` to stop the title and author lookups; covers already stored for identified books are still loaded by the browser.

A lookup that keeps failing is retried with growing delays up to eight times, then left for **Identify book**. A provider outage pauses the queue for a minute at a time (or as long as the provider asks, at most an hour) rather than holding every other book behind one failure.

To run metadata database checks against a disposable PostgreSQL server, set
`DATABASE_URL` to a role allowed to create test databases and run
`cargo test --workspace --locked -- --include-ignored`. Provider tests use local
mock HTTP responses and need no Google key.
