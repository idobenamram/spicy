#!/bin/sh
# Copies each reference in docs/references.md into externals/<key>, at the version that
# the table pins: shallow, and only the "Read" folders when the row names any.
# Usage: scripts/fetch_references.sh [key ...]   (no key: every reference)
set -eu
cd "$(dirname "$0")/.."
grep -E '^\| `[a-z0-9-]+` \|' docs/references.md |
    while IFS='|' read -r _ key _ version repo paths _; do
        key=$(echo "$key" | tr -d ' `')
        version=$(echo "$version" | tr -d ' `')
        repo=$(echo "$repo" | tr -d ' ')
        paths=$(echo "$paths" | tr -d '`')
        if [ $# -gt 0 ] && ! echo " $* " | grep -q " $key "; then continue; fi
        dir=externals/$key
        if [ "$(cat "$dir/.git/spicy-pinned" 2>/dev/null)" = "$version" ]; then
            echo "$key: $version, already there"
            continue
        fi
        echo "$key: fetching $version"
        [ -d "$dir/.git" ] || git init -q "$dir"
        git -C "$dir" remote remove origin 2>/dev/null || true
        git -C "$dir" remote add origin "$repo"
        if [ -n "$(echo "$paths" | tr -d ' ')" ]; then
            # Only the named folders: fetch the tree without file contents, then check
            # out the folders, which fetches just their files.
            git -C "$dir" fetch -q --depth 1 --filter=blob:none origin "$version"
            git -C "$dir" sparse-checkout set $paths
        else
            git -C "$dir" fetch -q --depth 1 origin "$version"
            git -C "$dir" sparse-checkout disable 2>/dev/null || true
        fi
        git -C "$dir" -c advice.detachedHead=false checkout -q --detach FETCH_HEAD
        echo "$version" >"$dir/.git/spicy-pinned"
    done
