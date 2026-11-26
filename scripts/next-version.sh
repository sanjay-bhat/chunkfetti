#!/usr/bin/env bash
# Works out the next release from the commits since the last v* tag (see CONTRIBUTING.md).
# Writes version=, tag= and skip= to $GITHUB_OUTPUT (or stdout) and the release notes to release-notes.md.
set -euo pipefail

last=$(git describe --tags --abbrev=0 --match 'v[0-9]*' 2>/dev/null || true)
if [ -n "$last" ]; then range="$last..HEAD"; base=${last#v}; else range="HEAD"; base="0.0.0"; fi

subjects=$(git log "$range" --no-merges --format='%s')
bodies=$(git log "$range" --no-merges --format='%b')
out=${GITHUB_OUTPUT:-/dev/stdout}

if [ -z "$subjects" ]; then
  echo "skip=true" >> "$out"
  exit 0
fi

IFS=. read -r major minor patch <<< "$base"
if grep -qE '^[a-z]+(\([^)]*\))?!:' <<< "$subjects" || grep -q '^BREAKING CHANGE:' <<< "$bodies"; then
  if [ "$major" -eq 0 ]; then minor=$((minor + 1)); patch=0; else major=$((major + 1)); minor=0; patch=0; fi
elif grep -qE '^feat(\([^)]*\))?:' <<< "$subjects"; then
  minor=$((minor + 1)); patch=0
else
  patch=$((patch + 1))
fi
version="$major.$minor.$patch"

{
  section() {  # title, pattern
    local lines
    lines=$(grep -E "$2" <<< "$subjects" | sed -E 's/^[a-z]+(\([^)]*\))?!?: */- /' || true)
    if [ -n "$lines" ]; then printf '### %s\n\n%s\n\n' "$1" "$lines"; fi
  }
  section "Breaking changes" '^[a-z]+(\([^)]*\))?!:'
  section "New" '^feat(\([^)]*\))?:'
  section "Fixed" '^fix(\([^)]*\))?:'
  section "Faster" '^perf(\([^)]*\))?:'
  other=$(grep -vE '^(feat|fix|perf)(\([^)]*\))?!?:' <<< "$subjects" | sed -E 's/^[a-z]+(\([^)]*\))?: *//; s/^/- /' || true)
  if [ -n "$other" ]; then printf '### Other\n\n%s\n\n' "$other"; fi
} > release-notes.md

echo "skip=false" >> "$out"
echo "version=$version" >> "$out"
echo "tag=v$version" >> "$out"
