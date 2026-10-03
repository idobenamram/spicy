#!/bin/sh
# Checks every link in the docs, and every `docs/….md#anchor` that a code comment names.
# lychee does not read Rust files, so the code references go to it as a Markdown list.
# Needs lychee 0.24.2 (docs/writing_docs.md#links). CI runs this script.
set -eu
cd "$(dirname "$0")/.."
refs=target/doc-refs-in-code.md
mkdir -p target
grep -rnoE --include='*.rs' 'docs/[A-Za-z0-9_./-]+\.md(#[A-Za-z0-9_-]+)?' crates \
    | sed -E 's|^([^:]+:[0-9]+):(.*)$|- \1: [\2](../\2)|' >"$refs"
lychee . "$refs"
