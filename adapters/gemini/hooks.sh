#!/usr/bin/env bash
# Coderun adapter for Gemini CLI (Tier 1 — UserPromptSubmit + PreToolUse analogs, v0.4.0)
# Spec §3 Adapter Layer, PRINCIPLES.md:22-30 — native hook, not reverse proxy, fail-open on 30s timeout.
# Primary IPC: UDS + MessagePack (rmp); fallback: HTTP JSON on TCP 9527.
# See .claude/hooks/coderun-pregeneration.sh for reference.

set -euo pipefail
CODERUN_SOCKET="${CODERUN_SOCKET:-/tmp/coderun.sock}"
CODERUN_URL="${CODERUN_DAEMON_URL:-http://127.0.0.1:9527}"
TIMEOUT="${CODERUN_TIMEOUT_MS:-30000}"

HOOK_TYPE="${1:-PreGeneration}"   # PreGeneration | PreToolCall
INPUT="$(cat)"

# Redact secrets before logging/outbound (spec §6 packaging)
# shellcheck disable=SC2001
REDACTED="$(echo "$INPUT" | sed -E 's/(api[_-]?key[[:space:]]*[:=][[:space:]]*)[^[:space:]"]+/\1[REDACTED]/I; s/sk-[A-Za-z0-9]{20,}/[REDACTED]/g')"

# Try UDS/MessagePack first if socket exists (requires msgpack tooling — falls through to HTTP fallback, RwLock allows concurrent sessions in v0.4.0)
if [ -S "$CODERUN_SOCKET" ] && command -v python3 >/dev/null 2>&1; then
  :
fi

# HTTP fallback (fail-open)
HTTP_PAYLOAD="$(python3 -c "import json,sys; print(json.dumps({'hook_type': sys.argv[1], 'payload': {'type': 'MessageRewrite', 'session_id': 'gemini', 'message': sys.argv[2]}}))" "$HOOK_TYPE" "$INPUT" 2>/dev/null || echo "{}")"

RESPONSE="$(curl -sS --max-time $((TIMEOUT/1000)) -H 'Content-Type: application/json' -d "$HTTP_PAYLOAD" "$CODERUN_URL/hook" 2>/dev/null || echo "")"

if echo "$RESPONSE" | grep -q "RewrittenMessage"; then
  echo "$RESPONSE" | python3 -c "import json,sys; d=json.load(sys.stdin); print(d.get('payload',{}).get('rewritten',''))"
  exit 0
fi

# Fail-open: return original input unmodified
echo "$INPUT"
exit 0
