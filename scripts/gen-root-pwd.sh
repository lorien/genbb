#!/usr/bin/env bash
# Generate the GenBB root bootstrap password line: `{salt}:{hash}`.
#
# Prints exactly one line to stdout so it can be redirected straight to
# the bootstrap file:
#
#   scripts/gen-root-pwd.sh > var/root.pwd
#
# The salt is 16 random bytes as 32 hex chars and the hash is sha256 of
# salt:password — the exact formula the server verifies on the root's
# first login (see spec/docs/adr/0013-root-user-sessions.md). The
# password is asked for twice on the terminal (input hidden; prompts go
# to stderr, so stdout stays pure) and never touches argv or history.
#
# Needs: openssl, sha256sum.
set -uo pipefail

command -v openssl >/dev/null 2>&1 || { echo "openssl binary not found on PATH" >&2; exit 1; }
command -v sha256sum >/dev/null 2>&1 || { echo "sha256sum binary not found on PATH" >&2; exit 1; }

read -r -s -p "Root password: " PW
echo >&2
read -r -s -p "Confirm password: " PW2
echo >&2

if [ -z "$PW" ]; then
  echo "empty password" >&2
  exit 1
fi
if [ "$PW" != "$PW2" ]; then
  echo "passwords do not match" >&2
  exit 1
fi

SALT=$(openssl rand -hex 16)
HASH=$(printf '%s:%s' "$SALT" "$PW" | sha256sum | cut -d' ' -f1)
LINE="$SALT:$HASH"

if ! grep -Eq '^[0-9a-f]{32}:[0-9a-f]{64}$' <<<"$LINE"; then
  echo "generated line failed its own shape check" >&2
  exit 1
fi

printf '%s\n' "$LINE"
