#!/usr/bin/env bash
# Cuts a release from your machine: release-plz bumps the version and writes the changelog
# from the commits since the last tag, then this commits, tags and publishes them to
# crates.io and pushes to origin. Only feat, fix, perf and breaking commits make a release;
# chore, test, docs and the rest ride along with the next one.
#
#   scripts/release.sh
#
# Needs release-plz (`cargo install release-plz`) and `cargo login`. Run it on an up-to-date
# main once CI has passed on Tangled.

set -euo pipefail

# the same as release_commits in release-plz.toml
release_commits='^(feat|fix|perf)(\(.*\))?!?:|^[a-z]+(\(.*\))?!:'

version() {
    sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1
}

cd "$(dirname "$0")/.."

if [[ "$(git branch --show-current)" != main ]]; then
    echo "releases are cut from main" >&2
    exit 1
fi
if [[ -n "$(git status --porcelain)" ]]; then
    echo "commit or stash your changes first" >&2
    exit 1
fi
git fetch --quiet --tags origin
if [[ "$(git rev-parse HEAD)" != "$(git rev-parse origin/main)" ]]; then
    echo "main isn't the same as origin/main; pull or push first" >&2
    exit 1
fi

last="$(git describe --tags --abbrev=0 --match 'v*')"
if ! git log --format=%s "$last..HEAD" | grep -qE "$release_commits"; then
    echo "nothing to release since $last"
    exit 0
fi

current="$(version)"
release-plz update
next="$(version)"
if [[ "$next" == "$current" ]]; then
    echo "release-plz found nothing to release since $last"
    exit 0
fi

git --no-pager diff CHANGELOG.md
read -rp "Release v$next? [y/N] " answer
if [[ "$answer" != [yY] ]]; then
    git checkout -- .
    echo "left as it was"
    exit 0
fi

git commit --quiet -am "chore: release v$next"
git tag -a "v$next" -m "v$next"
# publish before pushing, so a publish that fails leaves nothing to take back but the
# local commit and tag
cargo publish
git push --atomic origin HEAD:refs/heads/main "refs/tags/v$next"
