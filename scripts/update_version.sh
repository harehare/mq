#!/bin/bash

export MQ_VERSION="0.2.12"

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
