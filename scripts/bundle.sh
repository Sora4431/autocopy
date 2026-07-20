#!/usr/bin/env bash
# Packages the release binary into a minimal AutoCopy.app bundle.
#
# A .app bundle isn't required to run AutoCopy (`cargo run --release` works
# fine on its own) — it's what most users expect to drag into /Applications,
# and it's required if you want to add AutoCopy as a Login Item via System
# Settings.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release

APP="AutoCopy.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp target/release/autocopy "$APP/Contents/MacOS/autocopy"

cat > "$APP/Contents/Info.plist" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>autocopy</string>
    <key>CFBundleIdentifier</key>
    <string>com.autocopy.app</string>
    <key>CFBundleName</key>
    <string>AutoCopy</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF

echo "Built $APP — move it to /Applications, then launch it once so macOS can prompt for Accessibility permission."
