#!/usr/bin/env bash
# Require Signed-off-by on every non-merge, non-bot commit in BASE_SHA..HEAD.
# Owned-repo policy: reject Anthropic co-author and Claude-Session trailers.
set -euo pipefail

base="${BASE_SHA:-}"
if [ -z "${base}" ]; then
  echo "FAIL: BASE_SHA is empty" >&2
  exit 1
fi

if ! git cat-file -e "${base}^{commit}" >/dev/null 2>&1; then
  echo "FAIL: BASE_SHA ${base} is not a commit" >&2
  exit 1
fi

while IFS= read -r sha; do
  [ -n "${sha}" ] || continue
  parents="$(git log -1 --format='%P' "${sha}")"
  case "${parents}" in
    *" "*)
      continue
      ;;
  esac
  author="$(git log -1 --format='%an' "${sha}")"
  case "${author}" in
    *"[bot]")
      echo "Skipping bot commit ${sha} by ${author}"
      continue
      ;;
  esac
  body="$(git log -1 --format='%B' "${sha}")"
  if ! printf '%s\n' "${body}" | grep -qi '^Signed-off-by:'; then
    echo "::error::Commit ${sha} by ${author} is missing Signed-off-by. Use git commit -s."
    exit 1
  fi
  if printf '%s\n' "${body}" | grep -qiE '^[[:space:]]*Co-authored-by:.*anthropic\.com'; then
    echo "::error::Commit ${sha} by ${author} credits an Anthropic co-author. Remove the Co-authored-by trailer."
    exit 1
  fi
  if printf '%s\n' "${body}" | grep -qiE '^[[:space:]]*Claude-Session:'; then
    echo "::error::Commit ${sha} by ${author} contains a Claude-Session trailer. Remove it."
    exit 1
  fi
done < <(git rev-list "${base}..HEAD")

echo "All commits have DCO sign-off"
