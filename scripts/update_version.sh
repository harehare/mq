#!/bin/bash

export MQ_VERSION="0.5.2"
export README="../README.md"
export INSTALL_DOC="../docs/books/src/start/install.md"

# Update Cargo.toml files
for crate in ../crates/*; do
    if [ -f "$crate/Cargo.toml" ]; then
        tmpfile=$(mktemp)
        mq -I text 'include "update_version" | update_crate_version()' "$crate/Cargo.toml" > "$tmpfile" && mv "$tmpfile" "$crate/Cargo.toml"
    fi
done

# Update package.json files
for dir in ../packages ../editors; do
    for package in "$dir"/*; do
        if [ -f "$package/package.json" ]; then
            tmpfile=$(mktemp)
            mq -I text 'include "update_version" | update_npm_version()' "$package/package.json" > "$tmpfile" && mv "$tmpfile" "$package/package.json"
        fi
    done
done

# Update pyproject.toml files
tmpfile=$(mktemp)
mq -I text 'include "update_version" | update_py_version()' "../crates/mq-python/pyproject.toml" > "$tmpfile" && mv "$tmpfile" "../crates/mq-python/pyproject.toml"

# Update README.md with the new version
mq -U --args VERSION $MQ_VERSION '.code | select(contains("docker")) | update(s"$$ docker run --rm ghcr.io/harehare/mq:${VERSION}")' $README > README.md.tmp \
  && mv README.md.tmp $README

mq -U --args VERSION $MQ_VERSION '.code | select(contains("docker")) | update(s"$$ docker run --rm ghcr.io/harehare/mq:${VERSION}")' $INSTALL_DOC > INSTALL_DOC.md.tmp \
  && mv INSTALL_DOC.md.tmp $INSTALL_DOC

mq -U --args VERSION $MQ_VERSION '.code | select(contains("cargo install --git https://github.com/harehare/mq.git mq-run")) | gsub("--tag.+", s"--tag v${VERSION}")' $README > README.md.tmp \
  && mv README.md.tmp $README

mq -U --args VERSION $MQ_VERSION '.code | select(contains("curl -L https://github.com/harehare/mq/releases/download/")) | gsub("v0.4.3", s"v${VERSION}")' $README > README.md.tmp \
  && mv README.md.tmp $README

mq -U --args VERSION $MQ_VERSION '.code | select(contains("cargo install --git https://github.com/harehare/mq.git mq-run")) | gsub("--tag.+", s"--tag v${VERSION}")' $INSTALL_DOC > INSTALL_DOC.md.tmp \
  && mv INSTALL_DOC.md.tmp $INSTALL_DOC

mq -U --args VERSION $MQ_VERSION '.code | select(contains("https://github.com/harehare/mq/releases/download/")) | gsub("v0.4.0", s"v${VERSION}")' $INSTALL_DOC > INSTALL_DOC.md.tmp \
  && mv INSTALL_DOC.md.tmp $INSTALL_DOC
