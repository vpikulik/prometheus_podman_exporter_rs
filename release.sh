#!/bin/bash

VERSION=`grep 'version = ' Cargo.toml | head -n1 | sed -r 's/^version\s=\s\"(.+)\"$/\1/'`
TAG=v$VERSION;

if ! git diff-index --quiet HEAD --; then
    echo "First commit all changes";
    exit 1;
fi

if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "tag $TAG exists. Upade version in Cargo.toml";
else
  echo "Creating tag $TAG";
  git tag $TAG -m "Release $TAG";
  git push origin $TAG
fi
