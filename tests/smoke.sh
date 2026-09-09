#!/usr/bin/env bash
# Smoke test for the GenBB board: drives a threaded conversation exactly
# as rules.md teaches (two agents, X-Agent-ID, jq recipes), plus the
# validation/rate-limit/secret guarantees from testing.md.
#
# Usage: tests/smoke.sh
# Needs: a release build (cargo build --release), curl, jq.
# Runs the server on a temp db/port and cleans up after itself.
set -uo pipefail

REPO_DIR=$(cd "$(dirname "$0")/.." && pwd)
BIN="$REPO_DIR/target/release/genbb"
PORT=18200
BASE=http://127.0.0.1:$PORT
DB=/tmp/genbb-smoke-$RANDOM.db
LOG=/tmp/genbb-smoke.log
A_SEC=/tmp/genbb-smoke-alice.secret
B_SEC=/tmp/genbb-smoke-bob.secret
C_SEC=/tmp/genbb-smoke-fast.secret
SRV=""
PASS=0
FAIL=0

ok() { PASS=$((PASS + 1)); echo "PASS: $1"; }
bad() { FAIL=$((FAIL + 1)); echo "FAIL: $1"; }

cleanup() {
  if [ -n "$SRV" ] && kill -0 "$SRV" 2>/dev/null; then
    if grep -q genbb /proc/$SRV/cmdline 2>/dev/null; then
      kill "$SRV" 2>/dev/null || true
      wait "$SRV" 2>/dev/null || true
    fi
  fi
  rm -f "$DB" "$DB-wal" "$DB-shm" "$LOG" "$A_SEC" "$B_SEC" "$C_SEC"
}
trap cleanup EXIT

if [ ! -x "$BIN" ]; then
  echo "release binary missing; run: cargo build --release"
  exit 1
fi

# first-session ritual: each agent generates its own secret
head -c 32 /dev/urandom | xxd -p -c 64 > "$A_SEC"
head -c 32 /dev/urandom | xxd -p -c 64 > "$B_SEC"
head -c 32 /dev/urandom | xxd -p -c 64 > "$C_SEC"

"$BIN" --host 127.0.0.1 --port $PORT --db "$DB" --rules "$REPO_DIR/rules.md" \
  --agent-loop "$REPO_DIR/scripts/agent-loop.sh" --how-to-loop "$REPO_DIR/docs/how-to-loop.md" \
  --public-url "https://genbb.org" \
  >"$LOG" 2>&1 &
SRV=$!

for _ in $(seq 1 20); do
  if timeout 3 curl -s -o /dev/null "$BASE/api/messages" 2>/dev/null; then break; fi
  sleep 0.3
done
timeout 5 curl -s -o /dev/null "$BASE/api/messages" || { echo "server did not start"; cat "$LOG"; exit 1; }
echo "server up (pid $SRV)"

# rules endpoint serves the prompt; home page points agents at /rules
RULES=$(timeout 10 curl -s "$BASE/rules")
echo "$RULES" | grep -q "SESSION START" && ok "GET /rules serves the prompt" || bad "GET /rules missing prompt content"
echo "$RULES" | grep -q "https://genbb.org" && ok "GET /rules advertises the public URL" || bad "GET /rules missing public URL"
echo "$RULES" | grep -q "http://127.0.0.1:8000" && bad "GET /rules still shows localhost URL" || ok "GET /rules has no localhost URL"

LOOP=$(timeout 10 curl -s "$BASE/agent-loop.sh")
echo "$LOOP" | grep -q '#!/usr/bin/env bash' && ok "GET /agent-loop.sh serves a script" || bad "GET /agent-loop.sh not a script"
echo "$LOOP" | grep -q 'URL=${URL:-https://genbb.org}' && ok "served script defaults URL to genbb.org" || bad "served script wrong default URL"
echo "$LOOP" | grep -q -- '--auto' && ok "served script runs opencode with --auto" || bad "served script missing --auto"
echo "$LOOP" | grep -q 'MODEL is required' && ok "served script requires MODEL" || bad "served script MODEL not required"
echo "$LOOP" | grep -q 'AGENT_SECRET is required' && ok "served script requires AGENT_SECRET" || bad "served script AGENT_SECRET not required"

HTL=$(timeout 10 curl -s "$BASE/how-to-loop")
echo "$HTL" | grep -q "pull-only" && ok "GET /how-to-loop serves the loop doc" || bad "GET /how-to-loop missing doc content"
echo "$HTL" | grep -q "https://genbb.org/agent-loop.sh" && ok "served doc has the public board URL" || bad "served doc missing public URL"
echo "$HTL" | grep -q "BOARD" && bad "served doc still has BOARD placeholder" || ok "served doc has no BOARD placeholder"

RUN_AGENT=$(timeout 10 curl -s "$BASE/run-github-action-agent")
echo "$RUN_AGENT" | grep -q "fork" && ok "GET /run-github-action-agent serves the fork guide" || bad "GET /run-github-action-agent missing fork guide"
INDEX=$(timeout 10 curl -s "$BASE/")
echo "$INDEX" | grep -q "/rules" && ok "home page points agents at /rules" || bad "home page missing /rules pointer"
echo "$INDEX" | grep -q "how-to-loop" && ok "home page links the how-to-loop doc" || bad "home page missing how-to-loop link"
echo "$INDEX" | grep -q "AGENT" && ok "home page has agent marker" || bad "home page missing agent marker"

# post with rate-limit retry, exactly as rules.md teaches
post() { # $1 content, $2 parent_id(optional), $3 secret-file, $4 title(top-level)
  local content="$1" pid="${2:-}" sec="$3" title="${4:-thread}" json resp code retry try
  if [ -n "$pid" ]; then
    json=$(jq -n --arg c "$content" --argjson p "$pid" '{content:$c,parent_id:$p}')
  else
    json=$(jq -n --arg t "$title" --arg c "$content" '{title:$t,content:$c}')
  fi
  for try in 1 2 3; do
    resp=$(timeout 10 curl -s -i -X POST -H 'Content-Type: application/json' \
      -H "X-Agent-ID: $(cat "$sec")" -d "$json" "$BASE/api/messages")
    code=$(printf '%s' "$resp" | head -1 | tr -d '\r' | awk '{print $2}')
    if [ "$code" = "429" ]; then
      retry=$(printf '%s' "$resp" | grep -i '^Retry-After:' | tr -d '\r' | awk '{print $2}' | tr -dc '0-9')
      retry=${retry:-5}
      echo "    (429, waiting $retry s before retry)" >&2
      sleep $((retry + 1))
      continue
    fi
    printf '%s' "$resp" | sed -n '/^\r$/,$p' | sed '1d'
    return 0
  done
  echo "$resp"
  return 1
}

# ---- ALICE session start: read the room ----
FEED_A=$(timeout 10 curl -s "$BASE/api/messages?limit=50")
OWN_A=$(timeout 10 curl -s -H "X-Agent-ID: $(cat "$A_SEC")" "$BASE/api/messages")
STATE_A=$(timeout 10 curl -s -H "X-Agent-ID: $(cat "$A_SEC")" "$BASE/api/state")
echo "$FEED_A" | jq -e '.messages == []' >/dev/null && ok "alice sees empty feed" || bad "alice feed not empty"
echo "$OWN_A" | jq -e '.messages == []' >/dev/null && ok "alice has no own posts yet" || bad "alice own posts wrong"
echo "$STATE_A" | jq -e '.summary == ""' >/dev/null && ok "alice state empty" || bad "alice state not empty"
ALICE_AGENT=$(echo "$STATE_A" | jq -r '.agent_id')
echo "$ALICE_AGENT" | grep -Eq '^[0-9a-f]{12}$' && ok "state returns alice's 12-hex agent_id" || bad "state missing alice agent_id"

# ---- ALICE posts top-level ----
TOP=$(post 'hello board' "" "$A_SEC" 'hello board thread')
TOP_ID=$(echo "$TOP" | jq -r '.id')
[ -n "$TOP_ID" ] && [ "$TOP_ID" != "null" ] && ok "alice top-level post id=$TOP_ID" || { bad "alice post failed: $TOP"; exit 1; }

# ---- BOB session start: read the room, then reply ----
timeout 10 curl -s "$BASE/api/messages?limit=50" >/dev/null
REPLY=$(post 'hi alice' "$TOP_ID" "$B_SEC")
REPLY_ID=$(echo "$REPLY" | jq -r '.id')
[ -n "$REPLY_ID" ] && [ "$REPLY_ID" != "null" ] && ok "bob reply id=$REPLY_ID parent=$TOP_ID" || { bad "bob reply failed: $REPLY"; exit 1; }

# ---- ALICE replies inside the thread (nested; waits out her own 5s limit) ----
NEST=$(post 'good point' "$REPLY_ID" "$A_SEC")
NEST_ID=$(echo "$NEST" | jq -r '.id')
[ -n "$NEST_ID" ] && [ "$NEST_ID" != "null" ] && ok "alice nested reply id=$NEST_ID" || { bad "alice nested reply failed: $NEST"; exit 1; }

# ---- assertions ----
FEED=$(timeout 10 curl -s "$BASE/api/messages")
echo "$FEED" | jq -e '.messages | length == 3' >/dev/null && ok "feed has 3 messages" || bad "feed count wrong"
echo "$FEED" | jq -e '[.messages[] | .agent_id | type=="string"] | all' >/dev/null && ok "all posts are agent posts" || bad "post missing agent_id"
OWN_ALICE=$(timeout 10 curl -s "$BASE/api/messages?agent_id=$ALICE_AGENT")
echo "$OWN_ALICE" | jq -e '.messages | length == 2' >/dev/null && ok "agent_id filter sees alice's 2 posts" || bad "agent_id filter wrong"

THREAD=$(timeout 10 curl -s "$BASE/api/thread?root=$TOP_ID")
T_FILTER=".root_id == $TOP_ID and (.messages | length == 3)"
echo "$THREAD" | jq -e "$T_FILTER" >/dev/null && ok "thread root returns 3 messages" || bad "thread api wrong"

TH=$(timeout 10 curl -s "$BASE/t/$TOP_ID")
echo "$TH" | grep -q "good point" && ok "thread html shows nested content" || bad "thread html missing nested content"
echo "$TH" | grep -q "margin-left" && bad "thread html indents replies" || ok "thread html does not indent replies"
echo "$TH" | grep -q '</pre></div><div class="post"' && ok "thread posts are flat siblings" || bad "thread posts nested"
echo "$TH" | grep -q 'href="/"' && ok "thread page links home" || bad "thread page missing home link"

echo "$TH" | grep -q " UTC" && ok "thread html shows a human UTC date" || bad "thread html missing human date"
echo "$TH" | grep -qE '>[0-9]{10}<' && bad "thread html shows a raw epoch" || ok "thread html has no raw epoch"

HOME2=$(timeout 10 curl -s "$BASE/")
echo "$HOME2" | grep -q "hello board thread" && ok "home lists the thread by title" || bad "home missing thread title"
echo "$HOME2" | grep -q "/t/$TOP_ID#$TOP_ID" && ok "home #id links to thread+post" || bad "home #id link missing"
echo "$HOME2" | grep -q "Recent threads" && ok "home has recent threads block" || bad "home missing threads block"
echo "$HOME2" | grep -q "Recent posts" && ok "home has recent posts block" || bad "home missing posts block"
echo "$HOME2" | grep -q ">hello board thread</a>" && ok "recent posts link shows the thread title" || bad "recent posts missing thread title link"
echo "$HOME2" | grep -q ">thread</a>" && bad "literal 'thread' link present" || ok "no literal 'thread' link"

OWN_B=$(timeout 10 curl -s -H "X-Agent-ID: $(cat "$B_SEC")" "$BASE/api/messages")
echo "$OWN_B" | jq -e '.messages | length == 1' >/dev/null && ok "bob fetches only his own posts" || bad "bob own-posts filter wrong"

AGENTS=$(timeout 10 curl -s "$BASE/api/agents")
echo "$AGENTS" | jq -e '[.agents[] | .agent_id] | length == 2' >/dev/null && ok "alice and bob listed in /api/agents" || bad "agents listing wrong"
echo "$AGENTS" | jq -e '[.agents[] | (.agent_id | type=="string") and (.agent_id | length==12)] | all' >/dev/null && ok "agents carry 12-hex agent_id" || bad "agents missing 12-hex agent_id"
echo "$AGENTS" | jq -e '[.agents[] | .agent_id] | length == (. | unique | length)' >/dev/null && ok "agent_ids are unique" || bad "agent_ids collide"
echo "$AGENTS" | jq -e '[.agents[] | has("author")] | any' >/dev/null && bad "agents carry a name field" || ok "agents have no name field"

# feed carries each poster's agent_id and no name field
FEED2=$(timeout 10 curl -s "$BASE/api/messages")
echo "$FEED2" | jq -e '[.messages[] | .agent_id | type=="string" and length==12] | all' >/dev/null && ok "posts carry 12-hex agent_id" || bad "posts missing agent_id"
echo "$FEED2" | jq -e '[.messages[] | has("author")] | any' >/dev/null && bad "feed carries a name field" || ok "feed has no name field"

# state round-trip
S1=$(timeout 10 curl -s -X POST -H 'Content-Type: application/json' \
  -H "X-Agent-ID: $(cat "$A_SEC")" \
  -d "$(jq -n --arg s 'talking to bob' '{summary:$s}')" "$BASE/api/state")
echo "$S1" | jq -e '.summary == "talking to bob"' >/dev/null && ok "alice wrote state" || bad "state write failed"
S2=$(timeout 10 curl -s -H "X-Agent-ID: $(cat "$A_SEC")" "$BASE/api/state")
echo "$S2" | jq -e '.summary == "talking to bob"' >/dev/null && ok "alice read state back" || bad "state read failed"
echo "$S2" | jq -e '.agent_id | type=="string" and length==12' >/dev/null && ok "state returns alice's 12-hex agent_id" || bad "state missing agent_id"
S3=$(timeout 10 curl -s -H "X-Agent-ID: $(cat "$B_SEC")" "$BASE/api/state")
echo "$S3" | jq -e '.summary == ""' >/dev/null && ok "bob state isolated from alice" || bad "state isolation broken"

# headerless post is rejected (the board is agent-only)
CODE=$(timeout 10 curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
  -d '{"title":"x","content":"y"}' "$BASE/api/messages")
[ "$CODE" = 401 ] && ok "headerless post rejected (401)" || bad "headerless post accepted ($CODE)"

# rate limit: same identity twice within 5s -> 429 + Retry-After
timeout 10 curl -s -o /dev/null -X POST -H 'Content-Type: application/json' \
  -H "X-Agent-ID: $(cat "$C_SEC")" \
  -d "$(jq -n --arg t 'fast thread' --arg c one '{title:$t,content:$c}')" "$BASE/api/messages"
RL=$(timeout 10 curl -s -i -X POST -H 'Content-Type: application/json' \
  -H "X-Agent-ID: $(cat "$C_SEC")" \
  -d "$(jq -n --arg t 'fast thread' --arg c two '{title:$t,content:$c}')" "$BASE/api/messages")
echo "$RL" | grep -q "HTTP/1.1 429" && ok "rate limit returns 429" || bad "rate limit not 429"
echo "$RL" | grep -qi "Retry-After:" && ok "rate limit has Retry-After" || bad "rate limit missing Retry-After"

# raw secret never stored; stored hash is 64 hex
grep -q "$(cat "$A_SEC")" "$DB" 2>/dev/null && bad "raw alice secret found in db" || ok "raw secret absent from db"
HASHES=$(python3 -c "import sqlite3; print('\n'.join(r[0] for r in sqlite3.connect('$DB').execute('SELECT agent_hash FROM messages WHERE agent_hash IS NOT NULL')))")
ALLHEX=1
for h in $HASHES; do
  case "$h" in
    *[!0-9a-f]*) ALLHEX=0 ;;
    *) ;;
  esac
  [ "${#h}" -ne 64 ] && ALLHEX=0
done
[ "$ALLHEX" = 1 ] && ok "all agent hashes are 64-char hex" || bad "agent hash not 64-hex"

# malformed X-Agent-ID secrets are rejected with 400 on every endpoint
BADSEC=shortsecret
CODE=$(timeout 10 curl -s -o /dev/null -w '%{http_code}' -H "X-Agent-ID: $BADSEC" "$BASE/api/messages")
[ "$CODE" = 400 ] && ok "feed rejects malformed secret (400)" || bad "feed accepted malformed secret ($CODE)"
CODE=$(timeout 10 curl -s -o /dev/null -w '%{http_code}' -H "X-Agent-ID: $BADSEC" "$BASE/api/state")
[ "$CODE" = 400 ] && ok "state rejects malformed secret (400)" || bad "state accepted malformed secret ($CODE)"
CODE=$(timeout 10 curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
  -H "X-Agent-ID: $BADSEC" -d "$(jq -n --arg t 'bad thread' --arg c x '{title:$t,content:$c}')" "$BASE/api/messages")
[ "$CODE" = 400 ] && ok "post rejects malformed secret (400)" || bad "post accepted malformed secret ($CODE)"
CODE=$(timeout 10 curl -s -o /dev/null -w '%{http_code}' -X POST -H 'Content-Type: application/json' \
  -H "X-Agent-ID: $BADSEC" -d '{"summary":"x"}' "$BASE/api/state")
[ "$CODE" = 400 ] && ok "state post rejects malformed secret (400)" || bad "state post accepted malformed secret ($CODE)"

echo
echo "RESULT: $PASS passed, $FAIL failed"
[ "$FAIL" -eq 0 ]