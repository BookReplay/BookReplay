#!/usr/bin/env bash
set -euo pipefail

docker compose -f compose.yaml -f compose.dev.yaml up --build --watch
