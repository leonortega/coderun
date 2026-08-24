#!/bin/bash
# Coderun Pre-Generation Hook for Claude Code
# Called when UserPromptSubmit is triggered
#
# Usage: Reads stdin with hook context, sends to Coderun daemon via HTTP

CODERUN_URL="${CODERUN_DAEMON_URL:-http://127.0.0.1:9527}"

# Read input from stdin (JSON)
INPUT=$(cat)

# Extract message from input (adapt based on Claude Code hook format)
MESSAGE=$(echo "$INPUT" | jq -r '.message // empty' 2>/dev/null || echo "")

if [ -z "$MESSAGE" ]; then
  # Pass through if no message
  echo "$INPUT"
  exit 0
fi

# Call Coderun daemon
RESPONSE=$(curl -s -X POST "${CODERUN_URL}/hook" \
  -H "Content-Type: application/json" \
  -d "{
    \"hook_type\": \"PreGeneration\",
    \"payload\": {
      \"type\": \"MessageRewrite\",
      \"session_id\": \"claude-code\",
      \"message\": $(echo "$MESSAGE" | jq -Rs .)
    }
  }" \
  --connect-timeout 5 \
  --max-time 30 \
  2>/dev/null)

if [ $? -ne 0 ] || [ -z "$RESPONSE" ]; then
  # Fail-open: pass through original
  echo "$INPUT"
  exit 0
fi

# Extract rewritten message
REWRITTEN=$(echo "$RESPONSE" | jq -r '.payload.rewritten // empty' 2>/dev/null)

if [ -n "$REWRITTEN" ]; then
  # Output enriched context for Claude Code
  echo "$REWRITTEN"
else
  # Pass through original
  echo "$INPUT"
fi
