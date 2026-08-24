#!/usr/bin/env bash
# coderun-pretool.sh - Compresses tool outputs via Coderun daemon
#
# This hook intercepts PreToolUse events and calls the Coderun daemon
# to compress tool outputs (file reads, search results, shell output).
#
# Exit codes:
#   0 - Proceed (with compressed output if available)
#   2 - Block (reason on stderr)
#   other - Non-blocking error, logged and ignored

set -euo pipefail

CODERUN_DAEMON_URL="${CODERUN_DAEMON_URL:-http://127.0.0.1:9527}"
REQUEST_TIMEOUT=10

# Read the JSON payload from stdin
payload=$(cat)

# Extract tool information
tool_name=$(echo "$payload" | jq -r '.tool_name // .tool // empty' 2>/dev/null || echo "")
tool_input=$(echo "$payload" | jq -c '.tool_input // {}' 2>/dev/null || echo "{}")

# If no tool name, just proceed
if [ -z "$tool_name" ]; then
  exit 0
fi

# Determine output type based on tool name
case "${tool_name,,}" in
  read|readfile)
    output_type="FileRead"
    ;;
  grep|search|rg)
    output_type="SearchResult"
    ;;
  bash|shell|exec|command)
    output_type="ShellOutput"
    ;;
  *)
    output_type="Other"
    ;;
esac

# Extract the content to compress (if available in the payload)
content=$(echo "$payload" | jq -r '.tool_input.content // .content // empty' 2>/dev/null || echo "")

# If no content to compress, just proceed
if [ -z "$content" ]; then
  exit 0
fi

# Check if content is too small to compress (less than 1000 chars)
if [ ${#content} -lt 1000 ]; then
  exit 0
fi

# Generate a correlation ID
correlation_id="req_$(uuidgen 2>/dev/null || echo "$(date +%s)-$$")"

# Build the Coderun request
request=$(jq -n \
  --arg cid "$correlation_id" \
  --arg tn "$tool_name" \
  --arg ot "$output_type" \
  --arg ct "$content" \
  '{
    correlation_id: $cid,
    hook_type: "PreToolCall",
    payload: {
      type: "ToolOutput",
      tool_name: $tn,
      output_type: $ot,
      content: $ct
    }
  }')

# Call the Coderun daemon
response=$(curl -s -m "$REQUEST_TIMEOUT" \
  -X POST \
  -H "Content-Type: application/json" \
  -d "$request" \
  "$CODERUN_DAEMON_URL/hook" 2>/dev/null || echo "")

# If no response or error, just proceed with original output
if [ -z "$response" ]; then
  exit 0
fi

# Check if we got a valid response
payload_type=$(echo "$response" | jq -r '.payload.type // empty' 2>/dev/null || echo "")

if [ "$payload_type" = "CompressedOutput" ]; then
  # Extract compression stats
  original_tokens=$(echo "$response" | jq -r '.payload.original_tokens // 0' 2>/dev/null || echo "0")
  compressed_tokens=$(echo "$response" | jq -r '.payload.compressed_tokens // 0' 2>/dev/null || echo "0")
  
  if [ "$original_tokens" -gt 0 ] && [ "$compressed_tokens" -gt 0 ]; then
    savings=$(( (original_tokens - compressed_tokens) * 100 / original_tokens ))
    echo "[coderun] Compressed $tool_name: ${savings}% reduction" >&2
  fi
  
  # The compressed content is in the response, but Claude Code will use
  # the original tool output. The compression stats are logged for visibility.
fi

# Default: proceed with original output
exit 0
