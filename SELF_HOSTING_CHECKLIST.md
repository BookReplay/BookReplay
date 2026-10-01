# BookReplay self-hosting checklist

Goal: someone can install BookReplay on their own server, keep their highlights private, upgrade safely, and recover their library without reading the source code.

This checklist is based on a source review of the current repository. Existing capabilities below are present in code; deployment and recovery have not been verified by this review. Unchecked items describe work to implement or validate.

## Existing foundation

- A [multi-stage Docker build](Dockerfile) bundles the web app and API and runs as a non-root user.
- [Docker Compose](compose.yaml) provides PostgreSQL, a persistent database volume, and a database health check.
- The [API startup](crates/api/src/lib.rs) applies migrations and stores sessions in PostgreSQL.
- [Owner authentication](crates/api/src/features/auth/mod.rs) uses password hashing, HttpOnly cookies, and a single-owner registration flow.
- [Clipping imports](crates/api/src/features/clippings/model.rs) use a transaction and skip exact duplicates.
- [Metadata enrichment](crates/openlibrary/src/lib.rs) has request pacing, retries, and database coordination for the background worker.

## P0 — Before recommending deployment to other people

### 1. Make installation predictable

- [ ] Provide a production Compose configuration with a versioned, prebuilt application image. Keep source builds and development watch settings available separately. **Configuration and [development override](compose.dev.yaml) implemented; first image publication remains pending.** The [release workflow](.github/workflows/release-image.yml) tests before publishing version tags. `v0.1.0` is explicitly documented as a planned image, not an available release.
- [x] Publish and test images for `linux/amd64` and `linux/arm64`, or clearly document the architectures actually supported. The initial supported target is explicitly **`linux/amd64` only**; a local amd64 image passed the installation check. ARM64 is not advertised as supported.
- [x] Add a root [.env.example](.env.example) with every required setting and clearly marked placeholders. Document `DATABASE_URL`, `OPEN_LIBRARY_CONTACT_EMAIL`, `SESSION_COOKIE_SECURE`, and `RUST_LOG`, including defaults and valid values.
- [x] Remove the hardcoded `bookreplay` database password from the production setup. Require an operator-provided password and explain how the database URL must match it.
- [x] Exclude local secrets, database backups, and personal `My Clippings.txt` files from Git and the Docker build context. Verified root and nested exclusions, with migrations retained.
- [x] Remove PostgreSQL's published host port from production defaults. Document a development override if direct database access is needed.
- [ ] Add restart policies for the application and database, and verify recovery after a host reboot. **Both policies implemented; container restart and replacement passed. Actual host reboot remains unverified.**
- [ ] Write a quick start covering configuration, startup, the browser URL, first-owner creation, first import, and safe shutdown. Include measured minimum resource requirements and persistent storage paths. **[Quick start](README.md#quick-start), storage paths, and measured container resource budget documented; whole-host minimum requirements and a clean Docker-host installation still need validation.**
- [x] Correct the README's local development instructions: `cargo run` requires a running database and `DATABASE_URL`; explain how to supply both.

Validation on 2026-09-21: production and development Compose configuration checks passed, including rejection of missing credentials and expansion of the example database URL. The Dockerfile built successfully on Linux amd64 using rootless Podman. [The repeatable smoke test](scripts/smoke-install.py), run with Docker Compose 5.5.1 against Podman's Docker-compatible API, created an owner, imported 1,000 synthetic highlights, and verified session and data persistence through restart and replacement of both containers. Duplicate import and subsequent owner login passed. Enforced limits were 0.5 CPU and 256 MiB RAM per service; import took 0.27 seconds, sampled memory was 22.54 MiB for the application and 25.89 MiB for PostgreSQL, and PostgreSQL storage was 46.43 MiB. These are a small-fixture baseline, not whole-host minima or peak-memory measurements. No release was published and the host was not rebooted.

Done when: a new user can deploy from the documentation on a clean supported host, import highlights, restart the stack, and find their data intact.

### 2. Secure first setup and normal access

- [x] Protect initial owner registration with a one-time setup secret or a documented private setup procedure. Registration requires `SETUP_SECRET`; an empty value disables setup. Owner uniqueness closes registration after the first success, including racing requests. Remove the secret and recreate the app after setup.
- [x] Document and test an HTTPS reverse-proxy setup, including `SESSION_COOKIE_SECURE=true`, upload limits, and how to prevent direct public access to the HTTP backend. [Caddy configuration](deploy/Caddyfile) and [operator guide](deploy/SECURITY.md) cover host TLS termination, loopback-only backend access, firewall checks, and a 16 MiB upload limit.
- [x] Make the intended deployment modes explicit: local HTTP for development and HTTPS for remote access. `APP_ORIGIN` selects the exact browser origin; remote origins require HTTPS and secure cookies. Local HTTP remains supported for development and SSH tunnels.
- [x] Add login and registration rate limits, including bounds on concurrent password hashing. Login, registration, and password changes share ten attempts per sliding minute per process; at most two password jobs run, including cancelled requests' unfinished blocking work. Since 2026-10-01 the ten attempts are counted per client address with a 60-per-minute ceiling across all addresses; a proxy header is used for the address only when the operator sets `CLIENT_IP_HEADER`. The single-process scope and remaining denial-of-service limits are documented.
- [x] Validate protection against cross-site state-changing requests, including login, registration, logout, imports, edits, and reviews. All unsafe methods require one exact matching Origin before authentication/body processing; missing, null, duplicate, and foreign origins are rejected. SameSite=Strict cookies remain enabled.
- [x] Add a documented owner password change and an operator-only password recovery procedure. [/password/](app/web/src/routes/password/+page.svelte) requires the current password; `bookreplay reset-owner-password` reads the replacement from stdin using the [operator procedure](deploy/SECURITY.md#operator-only-recovery). Old sessions fail after either operation, including a same-password reset.
- [x] Verify that unauthenticated requests cannot read or modify library data, and that registration remains closed after setup. Covered over HTTPS before setup, after setup, and after recovery; the library and review data survive recovery unchanged.
- [x] Review logs and error responses so passwords, cookies, database credentials, and highlight contents are not exposed. Raw database/upstream errors and private metadata query fields are removed; dependency tracing is suppressed. The HTTPS test scans application logs at trace level and exercises a database error containing private submitted text.

Validation on 2026-09-21: all 45 Rust tests, workspace Clippy with warnings denied, formatting, frontend type checks, and the Linux amd64 image build passed. [The HTTPS security check](scripts/smoke-security.py) passed against PostgreSQL 17 and Caddy 2.11.2 using rootless Podman's Docker-compatible API and Docker Compose, with certificate verification enabled using the test CA. It covered disabled/incorrect setup secrets, simultaneous registrations, cookie flags, all mutation origins, unauthenticated reads/writes, upload limits, password changes, operator recovery, old-cookie replay, unchanged library/review data, rate limiting with forged forwarding headers, and log privacy. The installation/persistence smoke test also passed with 1,000 synthetic highlights. Both checks run before image publication. Public DNS/ACME issuance and firewall reachability remain per-deployment operator checks; no public service was deployed. Tests removed only their own disposable data.

Done when: an unrelated visitor cannot claim a deployed instance or access its library, and the owner can recover access without deleting their data.

### 3. Make backups and recovery usable

- [x] Document exactly what must be preserved: PostgreSQL data, deployment configuration, and secrets. Explain which state is rebuildable and whether sessions should be invalidated after restoration. See [Backups, restore, and upgrades](README.md#backups-restore-and-upgrades).
- [x] Provide copyable backup and restore commands using PostgreSQL-aware tools. Do not present a copy of live database files as a complete backup procedure. `pg_dump -Fc` and `pg_restore --clean --if-exists` were rehearsed against a populated PostgreSQL 17 database on 2026-10-01; the full Compose restore drill below is still open.
- [ ] Provide a simple scheduled-backup example with retention, failure reporting, and storage on another machine or storage service. Protect backups as private library data.
- [ ] Document restoration into an empty database, including compatible application and PostgreSQL versions.
- [ ] Run a restore drill on a separate instance and verify the owner account, highlights, edits, book matches, and review history.
- [x] Explain that `docker compose down -v` removes the database volume, while ordinary container replacement should preserve it.

Done when: a fresh machine can recover a usable library from the documented backup without access to the original server.

### 4. Make upgrades safe

- [ ] Resolve the existing-library migration path in [the owner migration](migrations/20260828010000_create_owner.sql) and [the clipping ownership migration](migrations/20260828020000_add_clipping_owner.sql). The first creates an empty owner table; the second assigns existing clippings to owner `1`. For a database containing clippings but no owner, this conflicts with the foreign key. Define a safe bootstrap or ownership-claim flow and verify it with existing data.
- [ ] Test migrations against both an empty database and a populated database from the previous supported release.
- [x] Document the upgrade sequence: read release notes, back up, select the target image version, restart, and verify application readiness and library data.
- [x] Explain that migrations run automatically on startup. Document expected downtime and recovery steps for migration failure. Startup now reports the failing step instead of one generic message.
- [ ] Define rollback support per release. Where a schema change prevents running the old image, document restoration of the matching pre-upgrade backup and the loss of any subsequent writes.
- [ ] Treat PostgreSQL major-version upgrades as a separate documented operation; changing the image tag alone is not the upgrade procedure.
- [ ] Publish release notes covering configuration changes, migrations, compatibility, and known issues.

Done when: a populated instance can upgrade through the supported path, and the documented recovery procedure works after a failed upgrade.

## P1 — Make routine operation dependable

### 5. Expose health and handle interruptions

- [x] Add a lightweight liveness endpoint and a readiness endpoint that checks database access and completed startup. Return minimal information and make them usable without an owner session or login redirect. `/healthz` answers 204 once migrations have run and the database responds, 503 otherwise.
- [ ] Add an application health check to Compose using a probe available in the runtime image. Document that an unhealthy status alone does not restart a container. **Health check implemented with `bookreplay healthcheck`; the restart caveat is not yet documented.**
- [ ] Handle termination signals with graceful HTTP shutdown and a bounded shutdown period for background work. **HTTP shutdown on SIGTERM/SIGINT implemented; the enrichment worker is still stopped without a drain period.**
- [ ] Exercise database unavailability at startup and during requests. Ensure readiness reflects failures and the application recovers after the database returns.
- [ ] Detect and recover from stopped background tasks. **Session cleanup now retries after a failure; the enrichment worker is not yet supervised.**
- [ ] Verify that imports, browsing, and reviews remain usable when Open Library is unavailable; surface delayed enrichment without blocking the library.

### 6. Keep resource use and failures understandable

- [ ] Set and document intentional import size limits and request timeouts. Return understandable errors for oversized or malformed input, and report skipped entries. **16 MiB import limit set in the API and Caddyfile, with a specific message in the import page and a count of skipped duplicates; request timeouts remain open.**
- [x] Add pagination or bounded loading for the all-clippings endpoint, which currently fetches the entire library. Replaced by `GET /api/books/{id}/clippings`, which returns one book's highlights.
- [ ] Measure memory, CPU, disk growth, and import time with a representative large library. Use results to document practical limits and hosting requirements.
- [ ] Add request status and duration logging without logging private request bodies. Document useful `RUST_LOG` settings for both API and enrichment modules.
- [ ] Make pending or failed metadata enrichment visible and provide a way to retry persistent failures. **Lookups now stop after eight failed attempts and one failure no longer stalls the queue; surfacing the state in the UI remains open.**
- [ ] Add troubleshooting steps for database credentials, failed migrations, volume permissions, failed login behind HTTPS, rejected uploads, and metadata outages.

### 7. Preserve user control over data

- [ ] Add a documented export format containing highlights, book information, edits, and review state. Explain how portable exports differ from full instance backups.
- [ ] Provide deletion for highlights and books with clear confirmation and defined effects on review history.
- [ ] Document outbound network activity, including metadata queries and browser cover requests. Explain what information leaves the instance.
- [ ] Provide a setting to disable external enrichment and remote cover loading, and verify core library features work with outbound access blocked. **`METADATA_ENRICHMENT=false` stops all title lookups; covers already stored are still loaded by the browser, and a run with outbound access blocked is still open.**
- [x] Verify that importing the same Kindle file again preserves review history. Define the expected behavior after editing a highlight, because current duplicate detection includes the editable content. Duplicate detection now uses a hash of the imported text, so an edited highlight is still recognised and is not imported again.

## P2 — Make releases easy to trust and maintain

- [ ] Add CI for Rust formatting, linting, tests, frontend checks, frontend builds, and container builds. **[CI workflow](.github/workflows/ci.yml) covers everything except the container build, which still runs only in the release workflow.**
- [ ] Add a container smoke test with PostgreSQL covering startup, owner registration, login, import, review, logout, and persistence after restart.
- [x] Maintain synthetic import fixtures that always run in CI. The parser test now reads a committed [synthetic export](crates/kindle/src/fixtures/clippings.txt).
- [ ] Add automated upgrade coverage from a populated previous schema and a repeatable backup/restore check.
- [ ] Automate dependency and container vulnerability checks, with a process for shipping fixes. **`cargo audit` runs in CI and Dependabot watches cargo, npm, Docker and Actions; container image scanning remains open.**
- [ ] Include application version and source revision in image metadata and diagnostic output so operators can report their installed release.
- [ ] Add a license, support instructions, a private security-reporting channel, and a supported-version policy. **Licence stated in the README; [SECURITY.md](SECURITY.md) adds the reporting channel and supported-version policy. Enable private vulnerability reporting in the GitHub repository settings; support instructions remain open.**
- [ ] Document the supported topology initially as a single application instance with PostgreSQL. Add more deployment options only when there is a concrete need and validation for them.

## First release acceptance checklist

- [ ] A new user installs successfully using only the quick start.
- [ ] Initial setup, HTTPS login, and owner recovery work as documented.
- [ ] Imports and review state survive container replacement and host reboot.
- [ ] A backup restores correctly on another instance.
- [ ] A populated previous release upgrades successfully; failed upgrades have a tested recovery path.
- [ ] Database and Open Library interruptions produce understandable behavior and recover cleanly.
- [ ] The release image passes the automated checks on every advertised architecture.

Complete P0 first, then prioritize P1 using the failures observed during these acceptance checks.
