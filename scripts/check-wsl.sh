#!/bin/sh
# Same gate as `pnpm check`, runnable from WSL/Linux: fmt + clippy against the
# real Windows target, with zig rc standing in for the missing Windows
# resource compiler (see zig-rc.sh).
set -e
cd "$(dirname "$0")/.."

export RC_x86_64_pc_windows_msvc="$PWD/scripts/zig-rc.sh"
if ! command -v clang >/dev/null 2>&1 && [ -x /usr/lib/llvm20/bin/clang ]; then
	export CC_x86_64_pc_windows_msvc=/usr/lib/llvm20/bin/clang
fi

cargo fmt --check --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets \
	--target x86_64-pc-windows-msvc -- -D warnings
