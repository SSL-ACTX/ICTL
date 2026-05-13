#!/bin/bash
set -e

# ICTL Professional Test Suite Runner
# Usage: ./scripts/ictl_test.sh [category]

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}=== ICTL Automated Test Suite ===${NC}"

CATEGORY=${1:-"all"}

run_test_category() {
    local cat=$1
    echo -e "\n${BLUE}Running Category: $cat${NC}"
    # Match the new test file names: ictl_semantic.rs, etc.
    if cargo test --test "ictl_${cat}"; then
        echo -e "${GREEN}Category $cat PASSED${NC}"
    else
        echo -e "${RED}Category $cat FAILED${NC}"
        exit 1
    fi
}

if [ "$CATEGORY" == "all" ]; then
    run_test_category "semantic"
    run_test_category "temporal"
    run_test_category "entropic"
    run_test_category "acausal"
    run_test_category "isochronous"
else
    run_test_category "$CATEGORY"
fi

echo -e "\n${GREEN}=== ALL REQUESTED TESTS PASSED ===${NC}"
