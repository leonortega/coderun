#!/usr/bin/env bash
# coderun-pregeneration.sh - Enriches user prompts with context via Coderun daemon
#
# This hook intercepts UserPromptSubmit events and calls the Coderun daemon
# to enrich the prompt with repository context, knowledge, and skills.
#
# Exit codes:
#   0 - Proceed (with enriched message on stdout if available)
#   2 - Block (reason on stderr)
#   other - Non-blocking error, logged and ignored

set -euo pipefail

CODERUN_DAEMON_URL="${CODERUN_DAEMON_URL:-http://127.0.0.1:9527}"
REQUEST_TIMEOUT=30

# Read the JSON payload from stdin
payload=$(cat)

# Extract the prompt/message from the payload
# Claude Code passes the user's message in the payload
message=$(echo "$payload" | jq -r '.message // .prompt // empty' 2>/dev/null || echo "")

# If no message found, just proceed
if [ -z "$message" ]; then
  exit 0
fi

# Generate a correlation ID
correlation_id="req_$(uuidgen 2>/dev/null || echo "$(date +%s)-$$")"

# Build the Coderun request
request=$(jq -n \
  --arg cid "$correlation_id" \
  --arg msg "$message" \
  --arg sid "${SESSION_ID:-unknown}" \
  '{
    correlation_id: $cid,
    hook_type: "PreGeneration",
    payload: {
      type: "MessageRewrite",
      session_id: $sid,
      message: $msg
    }
  }')

# Call the Coderun daemon
response=$(curl -s -m "$REQUEST_TIMEOUT" \
  -X POST \
  -H "Content-Type: application/json" \
  -d "$request" \
  "$CODERUN_DAEMON_URL/hook" 2>/dev/null || echo "")

# If no response or error, just proceed with original message
if [ -z "$response" ]; then
  exit 0 fi

# Check if we got a valid response
payload_type=$(echo "$response" | jq -r '.payload.type // empty' 2>/dev/null || echo "")

if [ "$payload_type" = "RewrittenMessage" ]; then
  # Extract the rewritten message
  rewritten=$(echo "$response" | jq -r '.payload.rewritten // empty' 2>/dev/null || echo "")
  
  if [ -n "$rewritten" ] && [ "$rewritten" != "$message" ]; then
    # Output the enriched message (Claude Code will use this instead)
    echo "$rewritten"
    exit 0
  fi
fi

# Default: proceed with original message
exit 0
