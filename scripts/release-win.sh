#!/bin/sh
# Build the signed Windows NSIS installer from WSL and publish it (plus the
# latest.json update feed) as a GitHub release on HumbleToS/slab-releases.
# Installed apps pick the release up automatically within six hours.
#
# Bump "version" in src-tauri/tauri.conf.json before running.
# Requires: ~/.tauri/slab-updater.key (BACK IT UP — lost key = no more updates),
# makensis in ~/.local/opt/nsis, and an authenticated `gh`.
set -e
cd "$(dirname "$0")/.."

REPO="HumbleToS/slab-releases"
VERSION=$(python3 -c "import json; print(json.load(open('src-tauri/tauri.conf.json'))['version'])")

# nsis-wrap/makensis sets NSISDIR itself — the bundler doesn't forward env.
export PATH="$HOME/.local/opt/nsis-wrap:/usr/lib/llvm20/bin:$PATH"
# tauri-cli probes the Linux host for a tray library even when bundling for
# Windows; a stub pkg-config entry satisfies it (see fake-appindicator).
export PKG_CONFIG_PATH="$HOME/.local/opt/fake-appindicator/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export XWIN_ACCEPT_LICENSE=1
export RC_x86_64_pc_windows_msvc="$PWD/scripts/zig-rc.sh"
TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.tauri/slab-updater.key")"
export TAURI_SIGNING_PRIVATE_KEY
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""

"$HOME/.local/bin/pnpm" tauri build --runner cargo-xwin \
	--target x86_64-pc-windows-msvc --bundles nsis

BUNDLE_DIR="src-tauri/target/x86_64-pc-windows-msvc/release/bundle/nsis"
SETUP="$BUNDLE_DIR/Slab_${VERSION}_x64-setup.exe"
python3 - "$VERSION" "$SETUP" "$REPO" <<'EOF'
import json, sys, datetime
version, setup, repo = sys.argv[1:4]
sig = open(setup + ".sig").read().strip()
feed = {
    "version": version,
    "notes": f"Slab {version}",
    "pub_date": datetime.datetime.now(datetime.timezone.utc).isoformat().replace("+00:00", "Z"),
    "platforms": {
        "windows-x86_64": {
            "signature": sig,
            "url": f"https://github.com/{repo}/releases/download/v{version}/Slab_{version}_x64-setup.exe",
        }
    },
}
json.dump(feed, open("latest.json", "w"), indent=2)
EOF

gh release create "v$VERSION" "$SETUP" "$SETUP.sig" latest.json \
	--repo "$REPO" --title "Slab $VERSION" --notes "Slab $VERSION"
rm latest.json
echo "released v$VERSION -> https://github.com/$REPO/releases"
