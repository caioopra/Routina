#!/usr/bin/env bash
# Installs the project's git hooks.
# Run once after cloning: ./scripts/setup-hooks.sh

REPO_ROOT="$(git rev-parse --show-toplevel)"
ln -sf "$REPO_ROOT/scripts/pre-commit" "$REPO_ROOT/.git/hooks/pre-commit"
echo "Pre-commit hook installed."
