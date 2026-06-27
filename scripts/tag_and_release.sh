#!/usr/bin/env bash

# Local release helper: bump versions, regenerate changelog, commit, tag, and push.

set -euo pipefail

if [ -n "$(git status --porcelain)" ]; then
    echo "❗ Please commit all changes before bumping the version."
    exit 1
fi

NAME=$(sed -n 's/^name *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)
CURRENT=$(sed -n 's/^version *= *"\([^"]*\)".*/\1/p' Cargo.toml | head -n 1)
BUMP="${1:-}"

if [ -z "$BUMP" ]; then
    echo "🦋 What kind of change is this for $NAME? (current version is $CURRENT) [patch, minor, major] >"
    read -r BUMP
fi

case "$BUMP" in
    patch) NEW=$(echo "$CURRENT" | awk -F. '{$NF+=1; OFS="."; print $1,$2,$3}') ;;
    minor) NEW=$(echo "$CURRENT" | awk -F. '{$(NF-1)+=1; $NF=0; OFS="."; print $1,$2,$3}') ;;
    major) NEW=$(echo "$CURRENT" | awk -F. '{$1+=1; $2=0; $3=0; OFS="."; print $1,$2,$3}') ;;
    *) echo "Please specify patch, minor, or major"; exit 1 ;;
esac

command -v git >/dev/null || { echo "git is required"; exit 1; }
command -v cargo >/dev/null || { echo "cargo is required"; exit 1; }
command -v git-cliff >/dev/null || { echo "git-cliff is required: https://git-cliff.org/docs/installation"; exit 1; }

if git rev-parse "v${NEW}" >/dev/null 2>&1; then
    echo "❗ Tag v${NEW} already exists."
    exit 1
fi

echo "🦋 Would tag and push $NAME $CURRENT -> $NEW"
read -p "Proceed? [Y/n] " -r CONFIRM
CONFIRM=${CONFIRM:-y}
if [[ ! "$CONFIRM" =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 0
fi

# Update release manifests.
echo "🦋 Updating Cargo.toml to version ${NEW}"
sed -i.bak "s/^version *= *\"[^\"]*\"/version = \"${NEW}\"/" Cargo.toml
rm Cargo.toml.bak

if [ -f "npm/package.json" ]; then
    echo "🦋 Updating npm/package.json to version ${NEW}"
    sed -i.bak "s/\"version\":[[:space:]]*\"[^\"]*\"/\"version\": \"${NEW}\"/" npm/package.json
    rm npm/package.json.bak
fi

echo "🦋 Updating Cargo.lock..."
cargo generate-lockfile

echo "🦋 Regenerating CHANGELOG.md..."
git cliff --offline --tag "v${NEW}" -o CHANGELOG.md

if [ -f "npm/package.json" ]; then
    just sync_readme >/dev/null 2>&1 || cp README.md npm/README.md
fi

echo "🦋 Committing version bump ${NEW}..."
git add Cargo.toml Cargo.lock CHANGELOG.md
[ -f "npm/package.json" ] && git add npm/package.json npm/README.md
git commit -m "release: ${NAME} v${NEW}"

echo "🦋 Creating git tag v${NEW}"
git tag "v${NEW}"

echo "🦋 Pushing..."
git push
git push --tags
