#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
HOST="${HOST:-127.0.0.1}"
START_PORT="${FRONTEND_PORT:-5173}"

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

PORT_TO_USE="$START_PORT"
while ! port_available "$PORT_TO_USE"; do
  PORT_TO_USE=$((PORT_TO_USE + 1))
done

cd "$ROOT_DIR"
if [[ ! -x "$ROOT_DIR/node_modules/.bin/vite" ]]; then
  echo "Installing frontend dependencies..."
  npm install --no-audit --no-fund
fi

echo "Starting AI Application Factory frontend at http://${HOST}:${PORT_TO_USE}"
exec npm run dev --workspace frontend -- --host "$HOST" --port "$PORT_TO_USE"
