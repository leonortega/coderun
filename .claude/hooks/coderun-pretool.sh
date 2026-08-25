#!/bin/bash
# Coderun Pre-Tool Hook for Claude Code
# Called when PreToolUse is triggered
#
# Usage: Reads stdin with hook context, sends to Coderun daemon via HTTP

CODERUN_URL="${CODERUN_DAEMON_URL:-http://127.0.0.1:9527}"

# Read input from stdin (JSON)
INPUT=$(cat)

# Extract tool name and content
TOOL_NAME=$(echo "$INPUT" | jq -r '.tool_name // empty' 2>/dev/null || echo "")
CONTENT=$(echo "$INPUT" | jq -r '.content // empty' 2>/dev/null || echo "")

if [ -z "$CONTENT" ]; then
  # Pass through if no content
  echo "$INPUT"
  exit 0
fi

# Determine output type based on tool name
OUTPUT_TYPE="Other"
case "${TOOL_NAME,,}" in
  read|readfile) OUTPUT_TYPE="FileRead" ;;
  grep|search) OUTPUT_TYPE="SearchResult" ;;
  bash|shell|exec) OUTPUT_TYPE="ShellOutput" ;;
esac

# Call Coderun daemon
RESPONSE=$(curl -s -X POST "${CODERUN_URL}/hook" \
  -H "Content-Type: application/json" \
  -d "{
    \"hook_type\": \"PreToolCall\",
    \"payload\": {
      \"type\": \"ToolOutput\",
      \"tool_name\": \"${TOOL_NAME}\",
      \"output_type\": \"${OUTPUT_TYPE}\",
      \"content\": $(echo "$CONTENT" | jq -Rs .)
    }
  }" \
  --connect-timeout 5 \
  --max-time 10 \
  2>/dev/null)

if [ $? -ne 0 ] || [ -z "$RESPONSE" ]; then
  # Fail-open: pass through original
  echo "$INPUT"
  exit 0
fi

# Extract compressed content
COMPRESSED=$(echo "$RESPONSE" | jq -r '.payload.compressed // empty' 2>/dev/null)

if [ -n "$COMPRESSED" ]; then
  echo "$COMPRESSED"
else
  # Pass through original
  echo "$INPUT"
fi
