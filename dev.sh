#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
STATE_DIR="$ROOT/.rupicola"

SERVER_PORT="${RUPICOLA_SERVER_PORT:-4730}"
PANEL_PORT="${RUPICOLA_PANEL_PORT:-4731}"
SITE_PORT="${RUPICOLA_SITE_PORT:-4732}"
DB_PATH="${RUPICOLA_DB:-$STATE_DIR/panel.db}"
POLL_INTERVAL="${DEV_WATCH_INTERVAL:-1}"
RUST_LOG_VAL="${RUST_LOG:-info,tower_http=debug}"

usage() {
  cat <<'EOF'
Usage: ./dev.sh <command> [targets...]

Commands:
  watch [targets...]   Build, start, and auto-reload on file changes (default: server panel)
  up    [targets...]   Build and start/restart targets in the background
  down  [targets...]   Stop running dev targets (default: all)
  status               Show running status and ports for all targets
  logs  [target]       Tail log file(s) under .rupicola/
  check                Run fmt, clippy, nextest, and rupicola check

Targets:
  server               rupicola-server (port 4730)
  panel                rupicola-panel  (port 4731)
  site                 rupicola-site   (port 4732)
  all                  server + panel + site

Environment:
  RUPICOLA_SERVER_PORT (4730)   RUPICOLA_PANEL_PORT (4731)
  RUPICOLA_SITE_PORT   (4732)   RUPICOLA_DB         (.rupicola/panel.db)
  DEV_WATCH_INTERVAL   (1s)     RUST_LOG            (info,tower_http=debug)
EOF
}

pkg_for() {
  case "$1" in
    server) echo "rupicola-server" ;;
    panel)  echo "rupicola-panel" ;;
    site)   echo "rupicola-site" ;;
    *)      echo "unknown target: $1" >&2; exit 2 ;;
  esac
}

port_for() {
  case "$1" in
    server) echo "$SERVER_PORT" ;;
    panel)  echo "$PANEL_PORT" ;;
    site)   echo "$SITE_PORT" ;;
  esac
}

pid_file() { echo "$STATE_DIR/dev-$1.pid"; }
log_file() { echo "$STATE_DIR/dev-$1.log"; }

resolve_targets() {
  if [[ $# -eq 0 ]]; then
    echo "server panel"
    return
  fi
  local out=()
  for t in "$@"; do
    case "$t" in
      all) out+=(server panel site) ;;
      server|panel|site) out+=("$t") ;;
      *) echo "unknown target: $t" >&2; exit 2 ;;
    esac
  done
  printf '%s\n' "${out[@]}" | awk '!seen[$0]++'
}

stop_one() {
  local svc="$1"
  local pf
  pf="$(pid_file "$svc")"
  if [[ -f "$pf" ]]; then
    local pid
    pid="$(cat "$pf" 2>/dev/null || true)"
    if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      for _ in $(seq 1 30); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.1
      done
      kill -9 "$pid" 2>/dev/null || true
    fi
    rm -f "$pf"
  fi
}

wait_port() {
  local svc="$1" port="$2" pid="$3" lf
  lf="$(log_file "$svc")"
  for _ in $(seq 1 50); do
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "[$svc] process exited before listening on 127.0.0.1:$port" >&2
      tail -n 25 "$lf" >&2 || true
      return 1
    fi
    if (echo >/dev/tcp/127.0.0.1/"$port") 2>/dev/null; then
      return 0
    fi
    sleep 0.1
  done
  echo "[$svc] failed to listen on 127.0.0.1:$port" >&2
  tail -n 25 "$lf" >&2 || true
  return 1
}

cargo_run() {
  if [[ "${DEV_USE_SCCACHE:-0}" == "1" ]]; then
    cargo "$@"
  else
    RUSTC_WRAPPER="" CARGO_BUILD_RUSTC_WRAPPER="" cargo --config 'build.rustc-wrapper=""' "$@"
  fi
}

build_targets() {
  local pkgs=()
  for svc in "$@"; do
    pkgs+=(-p "$(pkg_for "$svc")")
  done
  echo "[build] ${*}"
  cargo_run build --manifest-path "$ROOT/Cargo.toml" --target-dir "$ROOT/target" "${pkgs[@]}"
}

start_one() {
  local svc="$1"
  local pkg bin port lf pf
  pkg="$(pkg_for "$svc")"
  bin="$ROOT/target/debug/$pkg"
  port="$(port_for "$svc")"
  lf="$(log_file "$svc")"
  pf="$(pid_file "$svc")"

  mkdir -p "$STATE_DIR"
  stop_one "$svc"

  case "$svc" in
    server)
      nohup env RUST_LOG="$RUST_LOG_VAL" "$bin" \
        --port "$port" \
        --db "$DB_PATH" \
        >"$lf" 2>&1 < /dev/null &
      ;;
    panel)
      nohup env RUST_LOG="$RUST_LOG_VAL" "$bin" \
        --port "$port" \
        --server-url "http://127.0.0.1:$SERVER_PORT" \
        >"$lf" 2>&1 < /dev/null &
      ;;
    site)
      nohup env RUST_LOG="$RUST_LOG_VAL" "$bin" \
        --port "$port" \
        --panel-url "http://127.0.0.1:$PANEL_PORT" \
        >"$lf" 2>&1 < /dev/null &
      ;;
  esac
  local pid=$!
  echo "$pid" >"$pf"
  wait_port "$svc" "$port" "$pid"
  echo "[$svc] listening on http://127.0.0.1:$port (pid $pid)"
}

snapshot_sources() {
  find "$ROOT/apps" "$ROOT/crates" "$ROOT/Cargo.toml" "$ROOT/Cargo.lock" \
    -type f \( -name '*.rs' -o -name '*.toml' -o -name '*.css' -o -name '*.js' -o -name '*.html' -o -name '*.sql' -o -name 'Cargo.lock' \) \
    -printf '%T@ %p\n' 2>/dev/null | sort
}

affected_targets() {
  local changed="$1"
  shift
  local active=("$@")

  # Shared crates or workspace manifests affect every active target.
  if echo "$changed" | grep -qE "($ROOT/crates/|$ROOT/Cargo\.(toml|lock))"; then
    printf '%s\n' "${active[@]}"
    return
  fi

  local out=()
  for svc in "${active[@]}"; do
    if echo "$changed" | grep -q "$ROOT/apps/$svc/"; then
      out+=("$svc")
    fi
  done
  if [[ ${#out[@]} -gt 0 ]]; then
    printf '%s\n' "${out[@]}"
  fi
}

cmd_up() {
  local targets=()
  mapfile -t targets < <(resolve_targets "$@")
  build_targets "${targets[@]}"
  for svc in "${targets[@]}"; do
    start_one "$svc"
  done
}

cmd_down() {
  local targets=()
  if [[ $# -eq 0 ]]; then
    targets=(site panel server)
  else
    mapfile -t targets < <(resolve_targets "$@")
  fi
  for svc in "${targets[@]}"; do
    stop_one "$svc"
    echo "[$svc] stopped"
  done
}

cmd_status() {
  for svc in server panel site; do
    local pf port
    pf="$(pid_file "$svc")"
    port="$(port_for "$svc")"
    if [[ -f "$pf" ]] && kill -0 "$(cat "$pf" 2>/dev/null || echo 0)" 2>/dev/null; then
      echo "$svc: running (pid $(cat "$pf"), http://127.0.0.1:$port)"
    else
      echo "$svc: stopped (port $port)"
    fi
  done
}

cmd_logs() {
  local files=()
  if [[ $# -eq 0 ]]; then
    for svc in server panel site; do
      [[ -f "$(log_file "$svc")" ]] && files+=("$(log_file "$svc")")
    done
  else
    for svc in $(resolve_targets "$@"); do
      files+=("$(log_file "$svc")")
    done
  fi
  if [[ ${#files[@]} -eq 0 ]]; then
    echo "no dev logs yet under $STATE_DIR"
    return 0
  fi
  tail -n 50 -f "${files[@]}"
}

cmd_watch() {
  local targets=()
  mapfile -t targets < <(resolve_targets "$@")

  cleanup() {
    echo ""
    echo "[watch] stopping dev processes..."
    for svc in "${targets[@]}"; do
      stop_one "$svc"
    done
  }
  trap cleanup EXIT INT TERM

  cmd_up "${targets[@]}"

  local prev curr changed_files
  prev="$(snapshot_sources)"
  echo "[watch] polling every ${POLL_INTERVAL}s for changes (Ctrl+C to stop)..."

  while true; do
    sleep "$POLL_INTERVAL"
    curr="$(snapshot_sources)"
    if [[ "$curr" == "$prev" ]]; then
      continue
    fi

    changed_files="$(comm -13 <(printf '%s\n' "$prev") <(printf '%s\n' "$curr") | awk '{print $2}')"
    prev="$curr"
    [[ -z "$changed_files" ]] && continue

    local to_reload=()
    mapfile -t to_reload < <(affected_targets "$changed_files" "${targets[@]}")
    if [[ ${#to_reload[@]} -eq 0 ]]; then
      continue
    fi

    echo "[watch] change detected -> reloading: ${to_reload[*]}"
    if build_targets "${to_reload[@]}"; then
      for svc in "${to_reload[@]}"; do
        start_one "$svc" || true
      done
    else
      echo "[watch] build failed; keeping previous process(es) running" >&2
    fi
  done
}

cmd_check() {
  cargo_run fmt --check
  cargo_run clippy --locked --workspace --all-targets -- -D warnings
  cargo_run nextest run --locked --workspace
  cargo_run run --locked -- check .
}

cmd="${1:-help}"
shift || true

case "$cmd" in
  watch)  cmd_watch "$@" ;;
  up|start|reload) cmd_up "$@" ;;
  down|stop) cmd_down "$@" ;;
  status) cmd_status ;;
  logs)   cmd_logs "$@" ;;
  check)  cmd_check ;;
  -h|--help|help) usage ;;
  *)
    echo "unknown command: $cmd" >&2
    usage >&2
    exit 2
    ;;
esac
