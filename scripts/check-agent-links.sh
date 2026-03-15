#!/bin/sh

set -eu

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

tmp_file="$(mktemp)"
trap 'rm -f "$tmp_file"' EXIT INT TERM

find . -type f \( -name 'AGENT.md' -o -name 'AGENTS.md' \) | sort > "$tmp_file"

missing=0

while IFS= read -r agent_file; do
    dir="$(dirname "$agent_file")"
    expected="$dir/CLAUDE.md"

    if [ ! -L "$expected" ]; then
        echo "Missing symlink: $expected"
        missing=1
    fi
done < "$tmp_file"

exit "$missing"
