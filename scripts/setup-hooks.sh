#!/bin/sh

# Install git hooks from scripts/hooks into .git/hooks
HOOK_DIR="$(git rev-parse --show-toplevel)/.git/hooks"
SCRIPT_DIR="$(cd "$(dirname "$0")/hooks" && pwd)"

for hook in "$SCRIPT_DIR"/*; do
    name=$(basename "$hook")
    cp "$hook" "$HOOK_DIR/$name"
    chmod +x "$HOOK_DIR/$name"
    echo "Installed $name hook"
done
