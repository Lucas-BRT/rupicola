#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/ferris-atelier"
CAPTURES="${FERRIS_DEV_CAPTURES:-$DATA_DIR/captures}"
DB="${FERRIS_DEV_DB:-$DATA_DIR/panel.db}"
PORT="${FERRIS_DEV_PORT:-4731}"
STATE_DIR="$ROOT/.ferris-atelier"
PID_FILE="$STATE_DIR/panel-dev.pid"
LOG_FILE="$STATE_DIR/panel-dev.log"
BIN="$ROOT/target/debug/ferris-atelier-panel"
if [[ ! -x "$BIN" ]] && [[ -x "$ROOT/panel/target/debug/ferris-atelier-panel" ]]; then
  BIN="$ROOT/panel/target/debug/ferris-atelier-panel"
fi
DEV_UNIT="ferris-atelier-dev.service"

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

stop_systemd() {
  if command -v systemctl >/dev/null 2>&1; then
    if systemctl --user is-active --quiet ferris-atelier.service 2>/dev/null; then
      systemctl --user stop ferris-atelier.service
    fi
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
    mkdir -p "$STATE_DIR"
    cargo build -p ferris-atelier-panel --quiet
    BIN="$ROOT/target/debug/ferris-atelier-panel"
    stop_dev
    stop_systemd
    if command -v systemd-run >/dev/null 2>&1; then
      systemd-run --user --unit="$DEV_UNIT" \
        --description="Ferris Atelier panel (dev)" \
        "$BIN" --captures "$CAPTURES" --db "$DB" --port "$PORT" >/dev/null
    else
      nohup "$BIN" --captures "$CAPTURES" --db "$DB" --port "$PORT" >"$LOG_FILE" 2>&1 < /dev/null &
      echo $! >"$PID_FILE"
    fi
    wait_ready
    echo "panel-dev running at http://127.0.0.1:$PORT"
    ;;
  stop)
    stop_dev
    echo "panel-dev stopped"
    ;;
  restore)
    stop_dev
    cargo install --path "$ROOT/panel" --locked --quiet
    systemctl --user restart ferris-atelier.service
    echo "ferris-atelier.service restored at http://127.0.0.1:4731"
    ;;
  status)
    if command -v systemctl >/dev/null 2>&1 && systemctl --user is-active --quiet "$DEV_UNIT" 2>/dev/null; then
      echo "panel-dev: active ($DEV_UNIT, port $PORT)"
    elif [[ -f "$PID_FILE" ]] && kill -0 "$(cat "$PID_FILE")" 2>/dev/null; then
      echo "panel-dev: running (pid $(cat "$PID_FILE"), port $PORT)"
    else
      echo "panel-dev: stopped"
    fi
    systemctl --user is-active ferris-atelier.service 2>/dev/null || true
    ;;
  *)
    echo "usage: $0 {reload|stop|restore|status}" >&2
    exit 2
    ;;
esac
