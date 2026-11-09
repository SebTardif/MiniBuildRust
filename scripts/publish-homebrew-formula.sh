#!/usr/bin/env bash
# Write Formula/minibuild.rb for a GitHub release tag.
# PUSH=1 clones SebTardif/homebrew-minibuild with TAP_KEY and pushes the formula.
set -euo pipefail

tag="${TAG:-}"
if [ -z "${tag}" ]; then
  echo "FAIL: TAG is empty" >&2
  exit 1
fi
case "${tag}" in
  v*) version="${tag#v}" ;;
  *)
    echo "FAIL: TAG ${tag} must start with v" >&2
    exit 1
    ;;
esac

repo="${GITHUB_REPOSITORY:-SebTardif/MiniBuildRust}"
tap_repo="${HOMEBREW_TAP_REPO:-SebTardif/homebrew-minibuild}"
out="${FORMULA_OUT:-Formula/minibuild.rb}"
work="$(mktemp -d)"
trap 'rm -rf "${work}"' EXIT

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

fetch_asset() {
  local target="$1"
  local ext="$2"
  local dest="$3"
  local name="minibuild-${tag}-${target}.${ext}"
  if [ -n "${FIXTURE_DIR:-}" ]; then
    cp "${FIXTURE_DIR}/${name}" "${dest}"
    return
  fi
  curl --retry 5 --retry-all-errors --retry-delay 2 -fsSL \
    -o "${dest}" \
    "https://github.com/${repo}/releases/download/${tag}/${name}"
}

arm="${work}/arm.tar.gz"
intel="${work}/intel.tar.gz"
linux="${work}/linux.tar.gz"
fetch_asset "aarch64-apple-darwin" "tar.gz" "${arm}"
fetch_asset "x86_64-apple-darwin" "tar.gz" "${intel}"
fetch_asset "x86_64-unknown-linux-gnu" "tar.gz" "${linux}"

arm_sha="$(sha256_file "${arm}")"
intel_sha="$(sha256_file "${intel}")"
linux_sha="$(sha256_file "${linux}")"

base="https://github.com/${repo}/releases/download/${tag}"
mkdir -p "$(dirname "${out}")"
cat > "${out}" <<EOF
class Minibuild < Formula
  desc "Parallel, incremental build system with zero dependencies"
  homepage "https://github.com/${repo}"
  license "MIT"

  on_macos do
    on_arm do
      url "${base}/minibuild-${tag}-aarch64-apple-darwin.tar.gz"
      sha256 "${arm_sha}"
    end

    on_intel do
      url "${base}/minibuild-${tag}-x86_64-apple-darwin.tar.gz"
      sha256 "${intel_sha}"
    end
  end

  on_linux do
    on_intel do
      url "${base}/minibuild-${tag}-x86_64-unknown-linux-gnu.tar.gz"
      sha256 "${linux_sha}"
    end
  end

  def install
    bin.install "minibuild"
  end

  test do
    assert_match "--help", shell_output("#{bin}/minibuild --help")
  end
end
EOF

echo "OK: wrote ${out} for ${tag}"

if [ "${PUSH:-}" != "1" ]; then
  exit 0
fi

if [ -z "${TAP_KEY:-}" ]; then
  echo "FAIL: TAP_KEY is empty" >&2
  exit 1
fi

key_file="${work}/tap.key"
known_hosts="${work}/known_hosts"
printf '%s\n' "${TAP_KEY}" > "${key_file}"
chmod 600 "${key_file}"
ssh-keyscan -t ed25519 github.com > "${known_hosts}" 2>/dev/null
export GIT_SSH_COMMAND="ssh -i ${key_file} -o IdentitiesOnly=yes -o UserKnownHostsFile=${known_hosts} -o StrictHostKeyChecking=yes"

tap_dir="${work}/tap"
git clone "git@github.com:${tap_repo}.git" "${tap_dir}"
mkdir -p "${tap_dir}/Formula"
cp "${out}" "${tap_dir}/Formula/minibuild.rb"
git -C "${tap_dir}" add Formula/minibuild.rb
if git -C "${tap_dir}" diff --cached --quiet; then
  echo "OK: formula already matches ${tag}"
  exit 0
fi
git -C "${tap_dir}" -c user.name="minibuild release" -c user.email="sebtardif@ncf.ca" \
  commit -m "minibuild ${version}"
git -C "${tap_dir}" push origin HEAD
echo "OK: pushed formula ${tag}"
