#!/usr/bin/env bash
# llvm-rc-compatible resource compiler backed by `zig rc` (resinator), used
# when cross-building the Windows exe from WSL. embed-resource probes with
# "-V /?" and expects an LLVM-RC banner; at compile time it passes llvm-rc
# style options, which map onto resinator's rc.exe-compatible CLI.
if [[ "$1" == "-V" || "$1" == "/?" ]]; then
	echo "OVERVIEW: LLVM Resource Converter (zig rc wrapper; supports no-preprocess)"
	exit 0
fi

args=()
for a in "$@"; do
	case "$a" in
	/no-preprocess) args+=("/:no-preprocess") ;;
	--) ;;
	*) args+=("$a") ;;
	esac
done
exec "$HOME/.local/bin/zig" rc "${args[@]}"
