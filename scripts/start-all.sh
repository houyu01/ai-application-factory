#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
HOST="${HOST:-127.0.0.1}"
BACKEND_START_PORT="${PORT:-8090}"

port_available() {
  python3 - "$HOST" "$1" >/dev/null 2>&1 <<'PY'
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

BACKEND_PORT="$BACKEND_START_PORT"
while ! port_available "$BACKEND_PORT"; do
  BACKEND_PORT=$((BACKEND_PORT + 1))
done

cleanup() {
  trap - INT TERM EXIT
  [[ -n "${BACKEND_PID:-}" ]] && kill "$BACKEND_PID" 2>/dev/null || true
  [[ -n "${FRONTEND_PID:-}" ]] && kill "$FRONTEND_PID" 2>/dev/null || true
  wait 2>/dev/null || true
}
trap cleanup INT TERM EXIT

PORT="$BACKEND_PORT" HOST="$HOST" "$ROOT_DIR/scripts/start-backend.sh" &
BACKEND_PID=$!
VITE_API_BASE_URL="http://${HOST}:${BACKEND_PORT}/api" HOST="$HOST" "$ROOT_DIR/scripts/start-frontend.sh" &
FRONTEND_PID=$!

echo "Backend and frontend are starting (backend: ${BACKEND_PORT}). Press Ctrl+C to stop both."

while kill -0 "$BACKEND_PID" 2>/dev/null && kill -0 "$FRONTEND_PID" 2>/dev/null; do
  sleep 1
done

exit 1
