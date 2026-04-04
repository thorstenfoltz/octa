#!/bin/bash

# Define color codes
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

# Unicode symbols
CHECK_MARK="${GREEN}✔${NC}"
CROSS_MARK="${RED}✖${NC}"

# Get the current branch name
BRANCH_NAME=${1:-$(git rev-parse --abbrev-ref HEAD)}

# Define the branch name pattern
BRANCH_NAME_REGEX="^(master|(feature|fix|hotfix|chore|refactor|test|docs)/[a-z0-9._-]+)$"

# Check the branch name against the pattern
if [[ ! $BRANCH_NAME =~ $BRANCH_NAME_REGEX && $BRANCH_NAME != "main" ]]; then
	echo -e "\n\n${CROSS_MARK} ${RED}Error:${NC} Branch name '${BRANCH_NAME}' does not follow the naming convention.\n"
	echo -e "${RED}Branch names must match the pattern:${NC} $BRANCH_NAME_REGEX"
	echo -e "${RED}Do you use feature/fix/hotfix/chore/refactor/test/docs as a prefix?${NC}"
	echo -e "${RED}Do you use only lowercase letters, numbers, dots, and hyphens in the branch name?${NC}"
	echo -e "${RED}You can rename your branch by running:${NC} git branch -m <new-branch-name>\n\n"
	exit 1
fi

echo -e "\n\n${CHECK_MARK} ${GREEN}Branch name '${BRANCH_NAME}' is valid.${NC}\n\n"
