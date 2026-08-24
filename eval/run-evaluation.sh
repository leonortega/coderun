#!/usr/bin/env bash
# run-evaluation.sh - Run Coderun evaluation suite
#
# Usage:
#   ./eval/run-evaluation.sh              # Run all evaluations
#   ./eval/run-evaluation.sh model        # Run model routing only
#   ./eval/run-evaluation.sh context      # Run context quality only
#   ./eval/run-evaluation.sh --view       # Open results in web UI

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}═══════════════════════════════════════════${NC}"
echo -e "${BLUE}  Coderun AI Runtime Evaluation Suite${NC}"
echo -e "${BLUE}═══════════════════════════════════════════${NC}"
echo

# Check if promptfoo is installed
if ! command -v npx &> /dev/null; then
    echo -e "${RED}Error: npx not found. Please install Node.js.${NC}"
    exit 1
fi

# Check if promptfoo is available
if ! npx promptfoo --version &> /dev/null 2>&1; then
    echo -e "${YELLOW}Installing promptfoo...${NC}"
    npm install -g promptfoo
fi

# Parse arguments
EVAL_TYPE="${1:-all}"
VIEW_MODE=false

if [ "$1" = "--view" ] || [ "$2" = "--view" ]; then
    VIEW_MODE=true
fi

# Function to run model routing evaluation
run_model_routing() {
    echo -e "\n${BLUE}Running Model Routing Evaluation...${NC}\n"
    
    cd "$PROJECT_DIR"
    npx promptfoo eval \
        -c eval/promptfoo.yaml \
        --tests eval/datasets/model-routing.yaml \
        --output eval/results/model-routing.json \
        2>&1 | tee eval/results/model-routing.log
    
    return $?
}

# Function to run context quality evaluation
run_context_quality() {
    echo -e "\n${BLUE}Running Context Quality Evaluation...${NC}\n"
    
    cd "$PROJECT_DIR"
    npx promptfoo eval \
        -c eval/promptfoo.yaml \
        --tests eval/datasets/context-quality.yaml \
        --output eval/results/context-quality.json \
        2>&1 | tee eval/results/context-quality.log
    
    return $?
}

# Run evaluations
EXIT_CODE=0

case "$EVAL_TYPE" in
    model)
        run_model_routing || EXIT_CODE=$?
        ;;
    context)
        run_context_quality || EXIT_CODE=$?
        ;;
    all)
        run_model_routing || EXIT_CODE=$?
        run_context_quality || EXIT_CODE=$?
        ;;
    *)
        echo -e "${RED}Unknown evaluation type: $EVAL_TYPE${NC}"
        echo "Usage: $0 [model|context|all] [--view]"
        exit 1
        ;;
esac

# View results if requested
if [ "$VIEW_MODE" = true ]; then
    echo -e "\n${BLUE}Opening results in web UI...${NC}\n"
    cd "$PROJECT_DIR"
    npx promptfoo view -c eval/promptfoo.yaml
fi

# Print summary
echo -e "\n${BLUE}═══════════════════════════════════════════${NC}"
echo -e "${BLUE}  Evaluation Complete${NC}"
echo -e "${BLUE}═══════════════════════════════════════════${NC}"
echo

if [ $EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}✓ All evaluations passed${NC}"
else
    echo -e "${RED}✗ Some evaluations failed${NC}"
fi

echo
echo "Results saved to: eval/results/"
echo "View results: ./eval/run-evaluation.sh --view"

exit $EXIT_CODE
