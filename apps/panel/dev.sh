#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STATE_DIR="$ROOT/.rupicola"
DATA_DIR="$STATE_DIR/dev-data"
PORT="${RUPICOLA_DEV_PORT:-4731}"
SERVER_URL="${RUPICOLA_DEV_SERVER_URL:-http://127.0.0.1:4730}"
PID_FILE="$STATE_DIR/panel-dev.pid"
LOG_FILE="$STATE_DIR/panel-dev.log"
DEV_UNIT="rupicola-dev.service"

stop_dev() {
  if command -v systemctl >/dev/null 2>&1; then
    systemctl --user stop "$DEV_UNIT" 2>/dev/null || true
    systemctl --user reset-failed "$DEV_UNIT" 2>/dev/null || true
  fi
  if [[ -f "$PID_FILE" ]]; then
    local pid
    pid="$(cat "$PID_FILE" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      for _ in $(seq 1 20); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.1
      done
      kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$PID_FILE"
  fi
}

wait_ready() {
  for _ in $(seq 1 30); do
    if (echo >/dev/tcp/127.0.0.1/"$PORT") 2>/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  echo "panel-dev failed to listen on 127.0.0.1:$PORT" >&2
  tail -n 20 "$LOG_FILE" >&2 || true
  return 1
}

cmd="${1:-reload}"
case "$cmd" in
  reload)
    mkdir -p "$DATA_DIR"
    cargo build --manifest-path "$ROOT/Cargo.toml" --target-dir "$ROOT/target" -p rupicola-panel --quiet
    BIN="$ROOT/target/debug/rupicola-panel"
    stop_dev
    if command -v systemd-run >/dev/null 2>&1 && systemctl --user status >/dev/null 2>&1; then
      systemd-run --user --unit="$DEV_UNIT" \
        --description="Rupicola panel (dev)" \
        "$BIN" --port "$PORT" --server-url "$SERVER_URL" >/dev/null
    else
      nohup "$BIN" --port "$PORT" --server-url "$SERVER_URL" >"$LOG_FILE" 2>&1 < /dev/null &
      echo $! >"$PID_FILE"
    fi
    wait_ready
    echo "panel-dev running at http://127.0.0.1:$PORT (data: $DATA_DIR, backend: $SERVER_URL)"
    ;;
  stop)
    stop_dev
    echo "panel-dev stopped"
    ;;
  reset)
    stop_dev
    rm -rf "$DATA_DIR"
    echo "panel-dev data reset ($DATA_DIR)"
    ;;
  status)
    if command -v systemctl >/dev/null 2>&1 && systemctl --user is-active --quiet "$DEV_UNIT" 2>/dev/null; then
      echo "panel-dev: active ($DEV_UNIT, port $PORT)"
    elif [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
      echo "panel-dev: running (pid $(cat "$PID_FILE"), port $PORT)"
    else
      echo "panel-dev: stopped"
    fi
    ;;
  *)
    echo "usage: $0 {reload|stop|reset|status}" >&2
    exit 2
    ;;
esac
