# Setup and access security

BookReplay supports one application process and one owner per database. Use local HTTP only on loopback (including an SSH tunnel); use HTTPS for remote browser access. Do not expose port 3000 or PostgreSQL publicly.

## First owner

Generate **separate** random values for `POSTGRES_PASSWORD` and `SETUP_SECRET` with `openssl rand -hex 32`. Set them in your private `.env` (`chmod 600 .env`). `SETUP_SECRET` must be 32–128 bytes; an empty or absent value disables registration. The API hashes the configured secret with Argon2 before comparing incoming requests.

Start the stack, open the configured browser origin, and enter the setup secret, name, email, and a 12–128-byte password at `/register/`. Then log in at `/login/`. The database enforces a single owner even when registrations race. After setup, registration stays closed regardless of whether the secret is still configured. Remove `SETUP_SECRET` from `.env` and run `docker compose up -d --force-recreate bookreplay` to remove it from the running container. Never put the secret in a URL or share a rendered `docker compose config` containing it.

For local access use `APP_ORIGIN=http://localhost:3000` and `SESSION_COOKIE_SECURE=false`. A remote operator can reach this origin privately with `ssh -N -L 3000:127.0.0.1:3000 user@server`. Other users on the server must be trusted: Docker/database access is operator access.

## HTTPS with Caddy on the host

Use Caddy 2.11.2 or later on the **same host** as Compose. Point a domain's DNS records at the host, allow inbound TCP 80 and 443, and set:

```dotenv
APP_ORIGIN=https://books.example.com
SESSION_COOKIE_SECURE=true
```

Replace the domain with your own. `APP_ORIGIN` is the exact browser origin: scheme, hostname, and non-default port if applicable, with **no trailing slash or path**. Recreate the app after changing these settings:

```sh
docker compose up -d --force-recreate bookreplay
```

Use [Caddyfile](Caddyfile) as `/etc/caddy/Caddyfile`. Either replace `{$BOOKREPLAY_DOMAIN}` with your domain, or set `BOOKREPLAY_DOMAIN=books.example.com` in the Caddy service's environment. After replacing the placeholder, validate and reload it using your service manager, for example:

```sh
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

Caddy obtains and renews the public certificate and redirects HTTP to HTTPS. Preserve Caddy's certificate storage. Compose publishes the backend on **127.0.0.1:3000 only**, and PostgreSQL has no host port. Keep these defaults; allow only SSH and 80/443 in your host/network firewall. Do not use the development Compose override for a public deployment. Apply the same restrictions to IPv6 if your domain has an IPv6 record. From another machine, verify HTTPS works and direct access to `http://SERVER_IP:3000` and port 5432 fails.

The proxy and API cap imports at **2 MiB (2,097,152 bytes)**; authentication JSON is capped at 4 KiB. An oversized import returns HTTP 413. Split larger Kindle exports at complete `==========` entry boundaries. Avoid request-body/header logging at the proxy; the supplied Caddyfile does not enable access logs. See Caddy's [reverse proxy](https://caddyserver.com/docs/caddyfile/directives/reverse_proxy) and [request-body limit](https://caddyserver.com/docs/caddyfile/directives/request_body) documentation.

Verify login through HTTPS and inspect the session cookie: `Secure`, `HttpOnly`, `SameSite=Strict`, and `Path=/`. A secure cookie will not authenticate an HTTP backend URL. A 403 on writes usually means the browser URL and `APP_ORIGIN` differ; use the matching origin or recreate the app with the correct setting.

## Request protection and limits

Every unsafe HTTP method requires exactly one `Origin` header matching `APP_ORIGIN`, including registration, login, logout, password changes, imports, highlight edits, book identification, and reviews. Missing, `null`, duplicate, and foreign origins are rejected before authentication or body processing. This follows the [OWASP origin-verification approach](https://cheatsheetseries.owasp.org/cheatsheets/Cross-Site_Request_Forgery_Prevention_Cheat_Sheet.html#verifying-the-origin-with-standard-headers); SameSite cookies provide another layer. CLI API clients must also send `Origin: <APP_ORIGIN>` and authenticate normally. No wildcard CORS is enabled.

Login, registration, and password changes share a **10-attempt sliding 60-second window per process**, including invalid requests. HTTP 429 includes `Retry-After`; wait a minute before retrying. At most **two password hashing/verifying jobs** run concurrently, without a waiting queue; excess requests receive 429. A cancelled request keeps its slot until the blocking password work finishes. Limits reset when the application restarts. This shared limit also lets an attacker temporarily deny new logins; existing library sessions and logout remain available. Use network access restrictions if targeted abuse is a concern. Multiple application replicas are unsupported.

The application does **not trust proxy-provided client IP, host, or scheme headers** for authentication, rate limits, or origin validation. `X-Forwarded-For` cannot bypass the global limit; the allowed origin comes only from configuration.

## Change the owner password

While logged in, choose **Change password** in the footer or visit `/password/`. Enter your current password and the new password twice. All existing sessions, including the current browser, become unusable on their next request; sign in again. Passwords are 12–128 UTF-8 bytes, so non-ASCII characters may take more than one byte each.

## Operator-only recovery

Recovery requires shell access and permission to execute a command in the application container. There is no public recovery endpoint or email delivery. Keep the owner row and database volumes intact.

From the deployment directory, run this with Python 3 in an interactive terminal. It prompts without echo, confirms the password, and passes it through stdin rather than shell arguments, environment variables, or shell history:

```sh
python3 - <<'PY'
import getpass
import subprocess

password = getpass.getpass('New owner password (12–128 bytes): ')
if password != getpass.getpass('Confirm new password: '):
    raise SystemExit('Passwords do not match')
if not 12 <= len(password.encode()) <= 128:
    raise SystemExit('Password must be 12–128 bytes')
subprocess.run(
    ['docker', 'compose', 'exec', '-T', 'bookreplay', 'bookreplay', 'reset-owner-password'],
    input=password + '\n', text=True, check=True,
)
print('Password reset. Sign in again; all previous sessions are invalid.')
PY
```

For a source-built stack, add `'-f', 'compose.yaml', '-f', 'compose.dev.yaml'` after `'compose'` in the command list. For a local binary, use `bookreplay reset-owner-password` with `DATABASE_URL` set and the same stdin procedure. Never pass the password as an argument. If the login email is forgotten, an operator can read it with `docker compose exec postgres psql -U bookreplay -d bookreplay -c 'SELECT email FROM owner;'`.

The command updates only the existing owner's salted password hash. It cannot register a new owner or delete highlights, metadata, edits, or review history. Sessions are bound to the password hash, so **even resetting to the same password invalidates every old cookie**. Stored session records expire normally; they cannot authenticate after a reset. Requests already executing when a password changes may finish. Recovery does not require a restart.

## Logs and repeatable checks

Application logs record operation names, numeric IDs, counts, and timing, without passwords, setup secrets, cookies, database URLs, highlight text, or metadata search strings. Raw database/upstream errors are omitted because they can contain private values. `RUST_LOG` filters application events only; dependency tracing is suppressed even with `RUST_LOG=trace` because session/HTTP dependencies may log secrets. Startup failure output is generic; inspect configuration privately and check database health. Database server logs and custom proxy logs must also be treated as private operator data.

```sh
python3 scripts/smoke-security.py localhost/bookreplay:dev
```

The test uses an isolated database and the supplied Caddyfile with a local certificate whose CA is explicitly trusted by the client (TLS verification stays enabled). It checks disabled setup, wrong secrets, racing registrations, HTTPS cookies, mutation origins, unauthenticated access, upload limits, password change/recovery, replay of old cookies, preserved library/review data, rate limiting despite spoofed proxy headers, and log privacy at trace level. It removes only its disposable volumes. Public DNS/ACME issuance and firewall rules must be checked on each deployed host; they are not simulated here. The release workflow runs this test before publishing.
