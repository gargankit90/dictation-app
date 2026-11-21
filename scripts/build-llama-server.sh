#!/usr/bin/env bash
# Builds the on-device cleanup helper (Q4): llama.cpp's `llama-server`, one
# static binary for macOS 13+, with the Metal library embedded and no web UI.
# Tauri bundles it from src-tauri/binaries/llama-server-<target triple>
# (`bundle.externalBin` in src-tauri/tauri.macos.conf.json) and signs it with
# the app.
#
#   scripts/build-llama-server.sh [target-triple]   # default: rustc's host
#   scripts/build-llama-server.sh --check [triple]  # verify the built binary
#
# Pinned: the llama.cpp release tag b12359 (commit b1b770d90, the build PR
# #326 evaluated Gemma 4 E2B with), its source tarball by SHA-256. A
# mismatch fails closed. The tarball is the only download: the server's web
# UI, which the build would otherwise fetch, is switched off.
#
# The result is cached per user (VOCET_BUILD_CACHE, default
# ~/Library/Caches/vocet-build), so every worktree and later run copies it
# in a second; the first build takes a few minutes. src-tauri/build.rs runs
# this script on macOS when the binary is missing.
set -euo pipefail

TAG=b12359
COMMIT=b1b770d90d94cad9e9ea5b993bdbc383a522fb28
SHA256=e2eadc0fd06f81abea3b8012584e67bea3b403d91bcd1b3072b656cbcb2fb4bb
URL="https://github.com/ggml-org/llama.cpp/archive/refs/tags/$TAG.tar.gz"
MIN_MACOS=13.0
# Changing any build setting below must change this, so caches rebuild.
RECIPE=1

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

check=0
if [ "${1:-}" = "--check" ]; then
  check=1
  shift
fi
triple=${1:-$(rustc -vV | sed -n 's/^host: //p')}
case "$triple" in
aarch64-apple-darwin) arch=arm64 ;;
x86_64-apple-darwin) arch=x86_64 ;;
*)
  echo "build-llama-server: only macOS targets are supported, not $triple" >&2
  exit 1
  ;;
esac

out="$root/src-tauri/binaries/llama-server-$triple"

# The checks CI runs, and this script after every build: a Mach-O for the
# right architecture, macOS $MIN_MACOS as its minimum, only system
# libraries, and the server's own options answering.
verify() {
  local bin=$1
  # Output is captured first: under pipefail, `grep -q` closing the pipe
  # early would fail the producer with SIGPIPE.
  local kind help version
  kind=$(file "$bin")
  help=$("$bin" --help 2>&1 || true)
  version=$("$bin" --version 2>&1 || true)
  [[ "$kind" == *"Mach-O 64-bit executable $arch"* ]] || {
    echo "build-llama-server: $bin is not a $arch executable" >&2
    return 1
  }
  local minos
  minos=$(otool -l "$bin" | awk '/LC_BUILD_VERSION/{found=1} found && /minos/{print $2; exit}')
  [ "$minos" = "$MIN_MACOS" ] || {
    echo "build-llama-server: minimum macOS is $minos, not $MIN_MACOS" >&2
    return 1
  }
  local foreign
  foreign=$(otool -L "$bin" | tail -n +2 | awk '{print $1}' |
    grep -v -E '^(/usr/lib/|/System/Library/)' || true)
  [ -z "$foreign" ] || {
    echo "build-llama-server: links non-system libraries: $foreign" >&2
    return 1
  }
  [[ "$help" == *"--no-webui"* ]] || {
    echo "build-llama-server: $bin doesn't run or lacks --no-webui" >&2
    return 1
  }
  [[ "$version" == *"build ${TAG#b}"* ]] || {
    echo "build-llama-server: $bin is not build ${TAG#b}" >&2
    return 1
  }
}

if [ "$check" = 1 ]; then
  verify "$out"
  echo "build-llama-server: $out is llama.cpp $TAG for $triple, macOS $MIN_MACOS+, system libraries only"
  exit 0
fi

cache_root=${VOCET_BUILD_CACHE:-$HOME/Library/Caches/vocet-build}
cached="$cache_root/llama-server/$TAG-r$RECIPE-$triple/llama-server"

if [ ! -x "$cached" ]; then
  work=$(mktemp -d "${TMPDIR:-/tmp}/llama-server.XXXXXX")
  trap 'rm -rf "$work"' EXIT
  echo "build-llama-server: downloading llama.cpp $TAG"
  curl --fail --silent --show-error --location --proto '=https' \
    --output "$work/src.tar.gz" "$URL"
  actual=$(shasum -a 256 "$work/src.tar.gz" | awk '{print $1}')
  if [ "$actual" != "$SHA256" ]; then
    echo "build-llama-server: SHA-256 mismatch for $URL" >&2
    echo "  expected $SHA256" >&2
    echo "  got      $actual" >&2
    exit 1
  fi
  tar -xzf "$work/src.tar.gz" -C "$work"
  src="$work/llama.cpp-$TAG"
  [ -d "$src" ] || {
    echo "build-llama-server: the tarball has no llama.cpp-$TAG folder" >&2
    exit 1
  }
  echo "build-llama-server: building (a few minutes, once per machine)"
  cmake -S "$src" -B "$work/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_OSX_DEPLOYMENT_TARGET="$MIN_MACOS" \
    -DCMAKE_OSX_ARCHITECTURES="$arch" \
    -DBUILD_SHARED_LIBS=OFF \
    -DGGML_METAL=ON \
    -DGGML_METAL_EMBED_LIBRARY=ON \
    -DGGML_NATIVE=OFF \
    -DGGML_BLAS=OFF \
    -DGGML_RPC=OFF \
    -DLLAMA_OPENSSL=OFF \
    -DLLAMA_BUILD_UI=OFF \
    -DLLAMA_USE_PREBUILT_UI=OFF \
    -DLLAMA_BUILD_TESTS=OFF \
    -DLLAMA_BUILD_EXAMPLES=OFF \
    -DLLAMA_BUILD_TOOLS=ON \
    -DLLAMA_BUILD_SERVER=ON \
    -DLLAMA_BUILD_NUMBER="${TAG#b}" \
    -DLLAMA_BUILD_COMMIT="${COMMIT:0:9}" \
    >"$work/configure.log" 2>&1 || {
    tail -40 "$work/configure.log" >&2
    exit 1
  }
  cmake --build "$work/build" --target llama-server -j "$(sysctl -n hw.ncpu)" \
    >"$work/build.log" 2>&1 || {
    tail -40 "$work/build.log" >&2
    exit 1
  }
  verify "$work/build/bin/llama-server"
  mkdir -p "$(dirname "$cached")"
  cp "$work/build/bin/llama-server" "$cached.tmp"
  mv "$cached.tmp" "$cached"
fi

mkdir -p "$(dirname "$out")"
cp "$cached" "$out.tmp"
mv "$out.tmp" "$out"
echo "build-llama-server: $out"
