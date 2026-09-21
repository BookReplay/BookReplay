#!/usr/bin/env python3
"""Test a locally available image with disposable Compose data, never the user's .env.

Usage: python3 scripts/smoke-install.py localhost/bookreplay:dev
Requires Docker Compose >= 2.24.4 and Python 3. Cleans up only its own test volume.
"""

import http.cookiejar
import json
import os
from pathlib import Path
import secrets
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request


def main():
    image = sys.argv[1]
    root = Path(__file__).resolve().parents[1]
    password = secrets.token_hex(24)
    env = dict(os.environ, BOOKREPLAY_IMAGE=image, POSTGRES_PASSWORD=password,
               DATABASE_URL=f"postgresql://bookreplay:{password}@postgres:5432/bookreplay",
               SESSION_COOKIE_SECURE="false", OPEN_LIBRARY_CONTACT_EMAIL="",
               RUST_LOG="bookreplay_api=info")
    project = f"bookreplay-smoke-{secrets.token_hex(4)}"
    with tempfile.TemporaryDirectory(prefix=project) as directory:
        override = Path(directory) / "compose.test.yaml"
        override.write_text("""services:
  bookreplay:
    ports: !override
      - "127.0.0.1::3000"
    cpus: 0.5
    mem_limit: 256m
  postgres:
    cpus: 0.5
    mem_limit: 256m
""")
        command = ["docker", "compose", "--env-file", "/dev/null", "-p", project,
                   "-f", str(root / "compose.yaml"), "-f", str(override)]

        def compose(*args, capture=False):
            result = subprocess.run(command + list(args), env=env, text=True,
                                    stdout=subprocess.PIPE if capture else None, check=True)
            return result.stdout.strip() if capture else None

        client = urllib.request.build_opener(
            urllib.request.HTTPCookieProcessor(http.cookiejar.CookieJar()))

        def request(path, data=None):
            headers = {}
            if isinstance(data, dict):
                data = json.dumps(data).encode()
                headers["Content-Type"] = "application/json"
            elif isinstance(data, str):
                data = data.encode()
                headers["Content-Type"] = "text/plain"
            with client.open(urllib.request.Request(base + path, data, headers), timeout=20) as response:
                return response.status, response.read()

        def wait_ready():
            deadline = time.monotonic() + 90
            while time.monotonic() < deadline:
                try:
                    status, body = request("/")
                    if status == 200 and b"<html" in body:
                        return
                except (OSError, urllib.error.URLError):
                    pass
                time.sleep(1)
            raise RuntimeError("Application did not serve its web UI within 90 seconds")

        try:
            compose("up", "-d", "--wait", "--wait-timeout", "90")
            base = "http://" + compose("port", "bookreplay", "3000", capture=True)
            wait_ready()
            credentials = {"email": "smoke@example.invalid", "password": password}
            assert request("/api/auth/register", dict(credentials, name="Smoke test"))[0] == 201
            assert request("/api/auth/login", credentials)[0] == 204
            count = 1000
            contents = {f"Synthetic highlight {i}." for i in range(count)}
            clippings = "".join(
                f"Synthetic installation test (Example Author)\n- Highlight at location {i + 1}\n\n"
                f"Synthetic highlight {i}.\n==========\n" for i in range(count))
            start = time.monotonic()
            status, body = request("/api/clippings/import", clippings)
            assert status == 201 and json.loads(body)["inserted"] == count, body
            print(f"Imported {count} synthetic highlights in {time.monotonic() - start:.2f}s", flush=True)
            before = json.loads(request("/api/clippings")[1])
            assert len(before) == count and {row["content"] for row in before} == contents
            ids = {row["id"] for row in before}

            for args in [("restart",), ("up", "-d", "--force-recreate", "--wait", "--wait-timeout", "90")]:
                compose(*args)
                base = "http://" + compose("port", "bookreplay", "3000", capture=True)
                wait_ready()
                # The existing cookie must still work after both services are replaced.
                rows = json.loads(request("/api/clippings")[1])
                assert len(rows) == count and {row["id"] for row in rows} == ids
                assert {row["content"] for row in rows} == contents

            assert json.loads(request("/api/clippings/import", clippings)[1])["inserted"] == 0
            assert request("/api/auth/logout", {})[0] == 204
            assert request("/api/auth/login", credentials)[0] == 204
            subprocess.run(["docker", "stats", "--no-stream"] +
                           compose("ps", "-q", capture=True).split(), env=env, check=True)
            compose("exec", "-T", "postgres", "du", "-sk", "/var/lib/postgresql/data")
            print("PASS: owner login, import, session and data persistence, duplicate import.", flush=True)
        finally:
            compose("down", "--volumes")


if __name__ == "__main__":
    main()
