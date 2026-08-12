#!/bin/sh
# Cross-link the test binaries against the real Windows target (cargo-xwin
# fetches the MSVC CRT + Windows SDK on first run) and execute them on the
# Windows host through WSL interop. On a Windows dev machine, plain
# `cargo test` does the same job.
set -e
cd "$(dirname "$0")/.."

export PATH="/usr/lib/llvm20/bin:$PATH"
export XWIN_ACCEPT_LICENSE=1
export RC_x86_64_pc_windows_msvc="$PWD/scripts/zig-rc.sh"
export CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUNNER="$PWD/scripts/win-run.sh"

exec cargo xwin test --manifest-path src-tauri/Cargo.toml \
	--target x86_64-pc-windows-msvc "$@"
