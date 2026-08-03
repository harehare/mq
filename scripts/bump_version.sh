#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")"

export README="../README.md"
export INSTALL_DOC="../docs/books/src/start/install.md"

BUMP="${1:-patch}"

CURRENT_VERSION=$(grep -m1 '^mq-lang = ' ../Cargo.toml | sed -E 's/.*version = "([0-9]+\.[0-9]+\.[0-9]+)".*/\1/')

if [ -z "$CURRENT_VERSION" ]; then
    echo "Could not determine the current version from ../Cargo.toml" >&2
    exit 1
fi

if [[ "$BUMP" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    NEW_VERSION="$BUMP"
else
    IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT_VERSION"
    case "$BUMP" in
        major) NEW_VERSION="$((MAJOR + 1)).0.0" ;;
        minor) NEW_VERSION="${MAJOR}.$((MINOR + 1)).0" ;;
        patch) NEW_VERSION="${MAJOR}.${MINOR}.$((PATCH + 1))" ;;
        *)
            echo "Usage: $0 [major|minor|patch|X.Y.Z]" >&2
            exit 1
            ;;
    esac
fi

export MQ_VERSION="$NEW_VERSION"

echo "Bumping version: ${CURRENT_VERSION} -> ${MQ_VERSION}"

tmpfile=$(mktemp)
mq -I text --args version $MQ_VERSION 'import "bump_version" | bump_version::crates_version()' ../Cargo.toml > "$tmpfile"
tmpfile2=$(mktemp)
awk -v transformed="$tmpfile" '
    BEGIN { while ((getline line < transformed) > 0) t[++n] = line }
    { if ($0 == "") print ""; else print t[++i] }
' ../Cargo.toml > "$tmpfile2" && mv "$tmpfile2" ../Cargo.toml
rm -f "$tmpfile"

# Update Cargo.toml files
for crate in ../crates/*; do
    if [ -f "$crate/Cargo.toml" ]; then
        tmpfile=$(mktemp)
        mq -I text --args version $MQ_VERSION 'import "bump_version" | bump_version::crate_version()' "$crate/Cargo.toml" > "$tmpfile" && mv "$tmpfile" "$crate/Cargo.toml"
    fi
done

# Update package.json files
for dir in ../packages ../editors; do
    for package in "$dir"/*; do
        if [ -f "$package/package.json" ]; then
            tmpfile=$(mktemp)
            mq -I text --args version $MQ_VERSION 'import "bump_version" | bump_version::npm_version()' "$package/package.json" > "$tmpfile" && mv "$tmpfile" "$package/package.json"
        fi
    done
done

# Update README.md with the new version
mq -U --args VERSION $MQ_VERSION 'import "bump_version" | bump_version::code_block_version(VERSION)' $README > README.md.tmp \
  && mv README.md.tmp $README

# Update INSTALL_DOC.md with the new version
mq -U --args VERSION $MQ_VERSION 'import "bump_version" | bump_version::code_block_version(VERSION)' $INSTALL_DOC > INSTALL_DOC.md.tmp \
  && mv INSTALL_DOC.md.tmp $INSTALL_DOC

echo "Done. Review with 'git diff', then:"
echo "  git add -A && git commit -m \"chore: bump version to ${MQ_VERSION}\""
echo "  git tag v${MQ_VERSION}"
