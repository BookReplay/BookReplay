#!/usr/bin/env python3
"""Exercise P0 2 over verified local TLS using disposable Compose services.

Usage: python3 scripts/smoke-security.py localhost/bookreplay:dev
Requires Docker Compose >= 2.24.4 and Python 3. Never reads the operator's .env.
"""

from concurrent.futures import ThreadPoolExecutor
import http.client
import json
import os
from pathlib import Path
import secrets
import socket
import ssl
import subprocess
import sys
import tempfile
import time


def main():
    root = Path(__file__).resolve().parents[1]
    secret, password, database_password = (secrets.token_hex(32) for _ in range(3))
    with socket.socket() as reservation:
        reservation.bind(("127.0.0.1", 0))
        port = reservation.getsockname()[1]
    origin = f"https://localhost:{port}"
    project = "bookreplay-security-" + secrets.token_hex(4)
    env = dict(os.environ, BOOKREPLAY_IMAGE=sys.argv[1], POSTGRES_PASSWORD=database_password,
               DATABASE_URL=f"postgresql://bookreplay:{database_password}@postgres:5432/bookreplay",
               SETUP_SECRET="", APP_ORIGIN=origin, SESSION_COOKIE_SECURE="true",
               OPEN_LIBRARY_CONTACT_EMAIL="", GOOGLE_BOOKS_API_KEY="", RUST_LOG="trace")
    with tempfile.TemporaryDirectory(prefix=project) as directory:
        directory = Path(directory)
        override = directory / "compose.yaml"
        override.write_text(f"""services:
  bookreplay:
    ports: !override
      - "127.0.0.1:{port}:2665"
  caddy:
    image: caddy:2.11.2-alpine
    network_mode: service:bookreplay
    environment:
      BOOKREPLAY_DOMAIN: {origin}
    volumes:
      - {root / 'deploy/Caddyfile'}:/etc/caddy/Caddyfile:ro,Z
      - caddy_data:/data
    depends_on:
      - bookreplay
volumes:
  caddy_data:
""")
        command = ["docker", "compose", "--env-file", "/dev/null", "-p", project,
                   "-f", str(root / "compose.yaml"), "-f", str(override)]

        def compose(*args, input=None, check=True):
            result = subprocess.run(command + list(args), env=env, input=input, text=True,
                                    stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            if check and result.returncode:
                raise RuntimeError(f"Compose {args[0]} failed:\n{result.stderr}")
            return result

        def request(path, data=None, *, method=None, cookie=None, request_origin=origin, headers=None):
            request_headers = dict(headers or {})
            if request_origin is not None:
                request_headers["Origin"] = request_origin
            if cookie:
                request_headers["Cookie"] = cookie
            if isinstance(data, dict):
                data = json.dumps(data)
                request_headers["Content-Type"] = "application/json"
            elif data is not None:
                request_headers["Content-Type"] = "text/plain"
            connection = http.client.HTTPSConnection("localhost", port, context=context, timeout=20)
            try:
                connection.request(method or ("POST" if data is not None else "GET"), path,
                                   data, request_headers)
                response = connection.getresponse()
                return response.status, {name.lower(): value for name, value in response.getheaders()}, response.read()
            finally:
                connection.close()

        def expect(status, *args, **kwargs):
            result = request(*args, **kwargs)
            assert result[0] == status, (args[0], status, result[0], result[2][:200])
            return result

        def ready():
            for _ in range(90):
                try:
                    if request("/")[0] == 303:
                        return
                except (OSError, http.client.HTTPException):
                    pass
                time.sleep(1)
            raise AssertionError("application did not become ready")

        def login(current_password=password):
            _, headers, _ = expect(204, "/api/auth/login", {"email": "owner@example.invalid", "password": current_password})
            cookie = headers["set-cookie"]
            assert "secure" in cookie.lower() and "httponly" in cookie.lower()
            assert "samesite=strict" in cookie.lower()
            return cookie.split(";", 1)[0]

        try:
            compose("up", "-d", "--wait", "--wait-timeout", "90")
            certificate = directory / "root.crt"
            for _ in range(30):
                if compose("cp", "caddy:/data/caddy/pki/authorities/local/root.crt", str(certificate), check=False).returncode == 0:
                    break
                time.sleep(1)
            context = ssl.create_default_context(cafile=str(certificate))
            ready()
            # Only TLS is published; the HTTP backend and PostgreSQL have no host ports here.
            services = json.loads(compose("config", "--format", "json").stdout)["services"]
            assert all(int(p["target"]) == port and p["host_ip"] == "127.0.0.1" for p in services["bookreplay"]["ports"])
            assert not services["postgres"].get("ports")
            registration = {"email": "owner@example.invalid", "name": "Synthetic owner", "password": password}
            expect(403, "/api/auth/register", dict(registration, setup_secret=secret))
            env["SETUP_SECRET"] = secret
            # Remove the namespace-sharing proxy before replacing its backend container.
            compose("rm", "--stop", "--force", "caddy")
            compose("up", "-d", "--force-recreate", "bookreplay")
            compose("up", "-d", "caddy")
            ready()
            expect(401, "/api/clippings")
            expect(403, "/api/auth/register", registration)
            expect(403, "/api/auth/register", dict(registration, setup_secret="wrong"))
            registration["setup_secret"] = secret
            mutations = [("POST", "/api/auth/register"), ("POST", "/api/auth/login"),
                         ("POST", "/api/auth/logout"), ("POST", "/api/auth/password"),
                         ("POST", "/api/clippings/import"), ("PATCH", "/api/clippings/1"),
                         ("PUT", "/api/books/1/identification"), ("POST", "/api/reviews/highlights/1")]
            for method, path in mutations:
                for bad_origin in (None, "null", "https://attacker.invalid"):
                    expect(403, path, {}, method=method, request_origin=bad_origin,
                           headers={"X-Forwarded-Host": "localhost", "X-Forwarded-For": "127.0.0.1"})
            # Racing registrations must still produce just one owner.
            with ThreadPoolExecutor(max_workers=2) as workers:
                statuses = sorted(workers.map(lambda _: request("/api/auth/register", registration)[0], range(2)))
            assert statuses == [201, 409], statuses
            expect(409, "/api/auth/register", registration)
            first_cookie, second_cookie = login(), login()
            highlight = "PRIVATE-HIGHLIGHT-" + secrets.token_hex(12)
            clipping = f"Synthetic security test (Example Author)\n- Highlight at location 1\n\n{highlight}\n==========\n"
            expect(201, "/api/clippings/import", clipping, cookie=first_cookie)
            rows = json.loads(expect(200, "/api/clippings", cookie=first_cookie)[2])
            clipping_id = rows[0]["id"]
            book_id = json.loads(expect(200, "/api/books", cookie=first_cookie)[2])[0]["id"]
            for method, path in mutations:
                expect(403, path, {}, method=method, cookie=first_cookie, request_origin="https://attacker.invalid")
            for path in ("/api/clippings", "/api/books", "/api/books/search?q=private", "/api/reviews/highlights/session"):
                expect(401, path)
            for method, path in mutations[2:]:
                expect(401, path, {}, method=method)
            expect(200, f"/api/clippings/{clipping_id}", {"content": highlight + " edited"}, method="PATCH", cookie=first_cookie)
            # PostgreSQL rejects NUL text; the error must not echo the submitted highlight.
            failure = expect(500, f"/api/clippings/{clipping_id}", {"content": highlight + "\u0000"}, method="PATCH", cookie=first_cookie)
            assert highlight.encode() not in failure[2]
            expect(200, f"/api/books/{book_id}/identification", {"provider": "open_library", "provider_id": "/works/OL1W", "title": "Synthetic", "authors": [], "cover_url": None, "first_publish_year": None, "edition_count": None, "isbns": []}, method="PUT", cookie=first_cookie)
            google_candidate = {"provider": "google_books", "provider_id": "synthetic_1", "title": "Synthetic Google", "authors": [], "cover_url": "https://books.google.com/books/content?id=synthetic_1&img=1", "first_publish_year": None, "edition_count": None, "isbns": []}
            expect(200, f"/api/books/{book_id}/identification", google_candidate, method="PUT", cookie=first_cookie)
            expect(400, f"/api/books/{book_id}/identification", dict(google_candidate, provider_id="../invalid"), method="PUT", cookie=first_cookie)
            expect(400, f"/api/books/{book_id}/identification", dict(google_candidate, cover_url="https://attacker.invalid/cover.jpg"), method="PUT", cookie=first_cookie)
            expect(200, f"/api/reviews/highlights/{clipping_id}", {"rating": "LATER"}, cookie=first_cookie)
            expect(413, "/api/clippings/import", "x" * (2 * 1024 * 1024 + 1), cookie=first_cookie)
            def library_snapshot():
                return compose("exec", "-T", "postgres", "psql", "-U", "bookreplay", "-d", "bookreplay", "-Atc",
                               "SELECT row_to_json(c) FROM clippings c ORDER BY id; SELECT row_to_json(r) FROM highlight_reviews r ORDER BY id; SELECT open_library_key, title FROM books ORDER BY id;").stdout
            before_recovery = library_snapshot()
            print("PASS: HTTPS setup, registration race, access and CSRF checks, import/edit/review, upload bound.", flush=True)
            # The global limiter includes malformed/unauthenticated password-change attempts above.
            compose("stop", "caddy")
            compose("restart", "bookreplay")
            compose("start", "caddy")
            ready()
            expect(401, "/api/auth/password", {"current_password": "wrong", "new_password": "replacement-password"}, cookie=first_cookie)
            expect(204, "/api/auth/password", {"current_password": password, "new_password": "replacement-password"}, cookie=first_cookie)
            for cookie in (first_cookie, second_cookie):
                expect(401, "/api/clippings", cookie=cookie)
            expect(401, "/api/auth/login", {"email": registration["email"], "password": password})
            third_cookie = login("replacement-password")
            expect(204, "/api/auth/logout", {}, cookie=third_cookie)
            expect(401, "/api/clippings", cookie=third_cookie)
            third_cookie, fourth_cookie = login("replacement-password"), login("replacement-password")
            # Operator command reads stdin and changes only the existing owner's password.
            compose("exec", "-T", "bookreplay", "bookreplay", "reset-owner-password", input="recovered-password\n")
            for cookie in (third_cookie, fourth_cookie):
                expect(401, "/api/clippings", cookie=cookie)
            expect(401, "/api/auth/login", {"email": registration["email"], "password": "replacement-password"})
            recovered_cookie = login("recovered-password")
            assert json.loads(expect(200, "/api/clippings", cookie=recovered_cookie)[2])[0]["content"] == highlight + " edited"
            assert library_snapshot() == before_recovery, "password recovery changed library data"
            compose("exec", "-T", "bookreplay", "bookreplay", "reset-owner-password", input="recovered-password\n")
            expect(401, "/api/clippings", cookie=recovered_cookie)
            login("recovered-password")
            expect(409, "/api/auth/register", registration)
            # Fill the shared ten-attempt window; forwarded IP changes cannot evade it.
            statuses = [request("/api/auth/login", {"email": registration["email"], "password": "wrong"}, headers={"X-Forwarded-For": f"192.0.2.{i}"})[0] for i in range(11)]
            assert 429 in statuses and statuses[-1] == 429, statuses
            expect(429, "/api/auth/register", registration)
            logs = compose("logs", "--no-color", "bookreplay").stdout
            for private_value in (secret, password, database_password, highlight, first_cookie.split("=", 1)[1], "replacement-password", "recovered-password"):
                assert private_value not in logs, "private value found in application logs"
            print("PASS: verified HTTPS, setup race/secret, cookies, CSRF on all mutations, access control, uploads, password change/recovery, session revocation, rate limits, log privacy.", flush=True)
        finally:
            compose("down", "--volumes", check=False)


if __name__ == "__main__":
    main()
