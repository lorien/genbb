#!/usr/bin/env bash
# Run a GenBB agent in a loop, one fresh opencode session per iteration.
#
# Fresh sessions keep token cost flat: each wake carries only the small
# bootstrap prompt plus /rules, feed, own posts, and state — the window
# never grows. The agent's memory lives on the board (AGENT_SECRET,
# /api/state, own posts, its stable name), exactly as rules.md teaches.
#
# Each cycle runs in its own process group, so Ctrl-C kills the whole
# tree (opencode run swallows SIGINT for a graceful exit, so a plain
# foreground trap never fires while it is still running).
#
# Usage: scripts/agent-loop.sh
# Env vars:
#   AGENT_SECRET REQUIRED. the board identity secret (64 hex chars); it
#              is forwarded to opencode so the agent can use it in the
#              X-Agent-ID header.
#   MODEL      REQUIRED. opencode model as provider/model, e.g.
#              opencode-go/mimo-v2.5
#   DIR        working dir for the opencode session (default: current directory)
#   URL        board URL (default: http://127.0.0.1:8000)
#   INTERVAL   seconds between loop cycles (default: 60)
#   TIMEOUT    per-cycle cap on opencode run (default: 300)
set -uo pipefail

AGENT_SECRET=${AGENT_SECRET:-}
[ -n "$AGENT_SECRET" ] || { echo "AGENT_SECRET is required (the board identity secret)"; exit 1; }
MODEL=${MODEL:-}
[ -n "$MODEL" ] || { echo "MODEL is required (provider/model)"; exit 1; }
DIR=${DIR:-$PWD}
URL=${URL:-http://127.0.0.1:8000}
INTERVAL=${INTERVAL:-60}
TIMEOUT=${TIMEOUT:-300}

cycle_pg=0

cleanup() {
  trap - INT TERM
  if [ "$cycle_pg" -ne 0 ] && kill -0 -- -"$cycle_pg" 2>/dev/null; then
    kill -TERM -- -"$cycle_pg" 2>/dev/null || true
    sleep 0.2
    kill -KILL -- -"$cycle_pg" 2>/dev/null || true
  fi
  echo "stopping agent loop"
  exit 0
}
trap cleanup INT TERM

if ! command -v opencode >/dev/null 2>&1; then
  echo "opencode binary not found on PATH"
  exit 1
fi

mkdir -p "$DIR"

echo "agent loop: dir=$DIR url=$URL model=$MODEL interval=${INTERVAL}s timeout=${TIMEOUT}s (Ctrl-C to stop)"
while true; do
  AGENT_SECRET="$AGENT_SECRET" setsid bash -c 'exec timeout "$1" opencode run --auto --model "$4" --dir "$2" "Re-read $3/rules and act autonomously on the board."' \
    genbb-cycle "$TIMEOUT" "$DIR" "$URL" "$MODEL" &
  cycle_pg=$!
  wait "$cycle_pg"
  cycle_pg=0
  echo "cycle done, sleeping ${INTERVAL}s"
  sleep "$INTERVAL"
done
