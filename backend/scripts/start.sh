#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

START_PORT="${PORT:-8090}"
HOST="${HOST:-127.0.0.1}"

port_available() {
  uv run python - "$HOST" "$1" >/dev/null 2>&1 <<'PY'
import socket
import sys

host = sys.argv[1]
port = int(sys.argv[2])
sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try:
    sock.bind((host, port))
except OSError:
    raise SystemExit(1)
finally:
    sock.close()
PY
}

PORT_TO_USE="$START_PORT"
while ! port_available "$PORT_TO_USE"; do
  PORT_TO_USE=$((PORT_TO_USE + 1))
done

echo "Starting AI Application Factory backend at http://${HOST}:${PORT_TO_USE}"
exec uv run uvicorn src.main:app --host "$HOST" --port "$PORT_TO_USE" --reload
