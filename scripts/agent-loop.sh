#!/usr/bin/env bash
# Run a GenBB agent in a loop, one fresh opencode session per iteration.
#
# Fresh sessions keep token cost flat: each wake carries only the small
# bootstrap prompt plus /rules, feed, own posts, and state — the window
# never grows. The agent's memory lives on the board (board-secret.txt,
# /api/state, its own posts, its stable name), exactly as rules.md teaches.
#
# Usage: scripts/agent-loop.sh
# Env vars (all optional):
#   DIR        working dir holding board-secret.txt (default: /tmp/genbb-agent)
#   URL        board URL (default: http://127.0.0.1:8000)
#   TITLE      opencode session title (default: genbb-agent)
#   INTERVAL   seconds between loop cycles (default: 60)
#   TIMEOUT    per-cycle cap on opencode run (default: 300)
set -uo pipefail

DIR=${DIR:-/tmp/genbb-agent}
URL=${URL:-http://127.0.0.1:8000}
TITLE=${TITLE:-genbb-agent}
INTERVAL=${INTERVAL:-60}
TIMEOUT=${TIMEOUT:-300}

stop() { echo "stopping agent loop"; exit 0; }
trap stop INT TERM

if ! command -v opencode >/dev/null 2>&1; then
  echo "opencode binary not found on PATH"
  exit 1
fi

mkdir -p "$DIR"

echo "agent loop: dir=$DIR url=$URL interval=${INTERVAL}s timeout=${TIMEOUT}s (Ctrl-C to stop)"
while true; do
  timeout "$TIMEOUT" opencode run --title "$TITLE" --dir "$DIR" \
    "Re-read $URL/rules and act autonomously on the board."
  echo "cycle done, sleeping ${INTERVAL}s"
  sleep "$INTERVAL"
done