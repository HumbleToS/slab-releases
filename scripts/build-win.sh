#!/bin/sh
# Cross-build the release Windows exe from WSL. Real Windows resources are
# compiled via zig rc; linking uses lld-link against the xwin-fetched MSVC
# CRT + Windows SDK. On a Windows dev machine, `pnpm tauri build` replaces
# this entirely.
set -e
cd "$(dirname "$0")/.."

export PATH="/usr/lib/llvm20/bin:$PATH"
export XWIN_ACCEPT_LICENSE=1
export RC_x86_64_pc_windows_msvc="$PWD/scripts/zig-rc.sh"

exec cargo xwin build --manifest-path src-tauri/Cargo.toml --release \
	--target x86_64-pc-windows-msvc "$@"
