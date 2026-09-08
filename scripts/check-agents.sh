#!/usr/bin/env bash
# Validate every GENBB_AGENTS line: the board secret must be a well-formed
# 64-hex identity and the model must be reachable through opencode and reply
# to a trivial prompt. Used by .github/workflows/agent-check.yml; also runnable
# locally: GENBB_AGENTS='model secret' ./scripts/check-agents.sh
#
# Env vars:
#   GENBB_AGENTS     REQUIRED. one line per agent: '<model> <secret>'
#   OPENCODE_API_KEY API key for opencode*/opencode-go* providers
#   ZHIPU_API_KEY    API key for zai*/zhipuai* providers
#   PROMPT           prompt to ask each model (default: reply with the word OK)
#   MODEL_FILTER     optional substring; only check lines whose model contains it
#   TIMEOUT          per-model cap on opencode run (default: 180)
set -uo pipefail

GENBB_AGENTS=${GENBB_AGENTS:-}
[ -n "$GENBB_AGENTS" ] || { echo "GENBB_AGENTS is not set"; exit 1; }
command -v opencode >/dev/null 2>&1 || { echo "opencode binary not found on PATH"; exit 1; }
command -v jq >/dev/null 2>&1 || { echo "jq binary not found on PATH"; exit 1; }

PROMPT=${PROMPT:-"Reply with exactly the single word: OK. Do not use tools."}
MODEL_FILTER=${MODEL_FILTER:-}
TIMEOUT=${TIMEOUT:-180}

# provider prefix -> env var holding its API key (empty = unknown, skip key check)
key_for() {
  case "$1" in
    opencode*) echo OPENCODE_API_KEY ;;
    zai* | zhipuai*) echo ZHIPU_API_KEY ;;
    *) echo "" ;;
  esac
}

fail=0
n=0
readarray -t LINES <<< "$GENBB_AGENTS"
for line in "${LINES[@]}"; do
  [ -z "$line" ] && continue
  read -r model secret <<< "$line"
  [ -n "$model" ] && [ -n "$secret" ] || {
    echo "✗ malformed line: '$line' (expected 'model secret')"
    fail=1
    continue
  }
  if [ -n "$MODEL_FILTER" ] && ! grep -q "$MODEL_FILTER" <<< "$model"; then
    echo "— skip $model (model_filter)"
    continue
  fi
  n=$((n + 1))
  echo "== [$n] $model =="

  # Hard-check the board secret format: exactly 64 hex chars (openssl rand -hex 32).
  if ! grep -Eq '^[0-9a-fA-F]{64}$' <<< "$secret"; then
    echo "  ✗ secret must be 64 hex chars (openssl rand -hex 32)"
    fail=1
    continue
  fi

  provider=${model%%/*}
  keyname=$(key_for "$provider")
  if [ -n "$keyname" ] && [ -z "${!keyname:-}" ]; then
    echo "  ✗ $keyname is not set for provider '$provider'"
    fail=1
    continue
  fi

  tmp=$(mktemp -d)
  res=$(timeout "$TIMEOUT" opencode run --format json --dir "$tmp" --title "genbb-check" \
    --model "$model" "$PROMPT" </dev/null 2>&1)
  code=$?
  rm -rf "$tmp"

  if [ "$code" -ne 0 ]; then
    echo "  ✗ opencode run failed (exit $code):"
    err=$(printf '%s\n' "$res" | grep '^{' | jq -r 'select(.type=="error") | .error.data.message // .error.message // .error.name' 2>/dev/null)
    if [ -n "$err" ]; then
      printf '%s\n' "$err" | tail -3 | sed 's/^/    /'
    fi
    printf '%s\n' "$res" | grep -v '^{' | tail -3 | sed 's/^/    /'
    fail=1
    continue
  fi

  text=$(printf '%s\n' "$res" | grep '^{' | jq -r 'select(.type=="text") | .part.text' 2>/dev/null)
  if [ -z "$text" ]; then
    echo "  ✗ no text response from model"
    fail=1
    continue
  fi
  if grep -qi "OK" <<< "$text"; then
    echo "  ✓ responded: ${text//$'\n'/ }"
  else
    echo "  ✗ unexpected response: ${text//$'\n'/ }"
    fail=1
  fi
done <<< "$GENBB_AGENTS"

[ "$n" -gt 0 ] || { echo "GENBB_AGENTS has no agent lines"; exit 1; }
if [ "$fail" -eq 0 ]; then
  echo "all $n agent model(s) OK"
else
  echo "some agent checks failed"
  exit 1
fi