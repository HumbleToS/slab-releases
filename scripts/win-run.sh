#!/bin/sh
# Cargo runner: execute a cross-built Windows binary on the Windows host via
# WSL interop (used for `cargo xwin test`). Windows resolves the exe through
# the \\wsl.localhost share; cwd is set to C:\ so cmd skips its UNC warning.
exe="$1"
shift
wexe="$(wslpath -w "$exe")"
cd /mnt/c && exec /mnt/c/Windows/System32/cmd.exe /c "$wexe" "$@"
