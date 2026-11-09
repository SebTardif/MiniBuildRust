#!/usr/bin/env bash
# Publish the minibuild crate. Skip when this version is already on crates.io.
# crates.io rejects requests with an empty User-Agent.
set -euo pipefail

name="minibuild"
user_agent="SebTardif/MiniBuildRust (github.com/SebTardif/MiniBuildRust)"

version="$(
  cargo metadata --no-deps --format-version 1 | python3 -c '
import json, sys
pkgs = json.load(sys.stdin)["packages"]
print(next(p["version"] for p in pkgs if p["name"] == "minibuild"))
'
)"

echo "PLAN: publish ${name} ${version}"

code=""
for attempt in 1 2 3; do
  code="$(
    curl -sS -A "${user_agent}" -o /dev/null -w "%{http_code}" \
      "https://crates.io/api/v1/crates/${name}/${version}"
  )"
  if [ "${code}" != "429" ]; then
    break
  fi
  echo "WAIT: crates.io rate limited ${name} ${version} (attempt ${attempt})"
  sleep 15
done

if [ "${code}" = "200" ]; then
  echo "OK: ${name} ${version} is already on crates.io"
  exit 0
fi

if [ "${code}" != "404" ]; then
  echo "FAIL: crates.io lookup for ${name} ${version} returned ${code}" >&2
  exit 1
fi

if [ -z "${CARGO_REGISTRY_TOKEN:-}" ]; then
  echo "FAIL: CARGO_REGISTRY_TOKEN is empty" >&2
  exit 1
fi

echo "DO: cargo publish --locked"
cargo publish --locked
echo "OK: published ${name} ${version}"
