#!/bin/bash

set -e

VERSION=$1
if [ -z "$VERSION" ]; then
    echo "usage: $0 NEWVERSION" 1>&2
    exit 2
fi

for toml in Cargo.toml {*-tiger,tiger-bin-shared,tiger-tables,utils,xtiger-mcp}/Cargo.toml \
            xtiger-app/src-tauri/Cargo.toml xtiger-app/setup/Cargo.toml; do
    sed -i -e "s/^version = .*/version = \"$VERSION\"/" \
           -e 's/^\(tiger-.*, version = \)"[^"]*"/\1'"\"$VERSION\"/" \
           "$toml"
done

# The app shows and compares this version too.
for conf in xtiger-app/src-tauri/tauri.conf.json xtiger-app/setup/tauri.conf.json; do
    sed -i -e '0,/"version": "[^"]*"/s//"version": "'"$VERSION"'"/' "$conf"
done
