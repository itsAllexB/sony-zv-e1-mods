#!/bin/sh
# Builds a universal (Apple silicon + Intel) "Open Gate Fix.app" into dist/.
set -eu
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
APP="dist/Open Gate Fix.app"

export MACOSX_DEPLOYMENT_TARGET=11.0
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp assets/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
lipo -create -output "$APP/Contents/MacOS/opengate-fix" \
    target/aarch64-apple-darwin/release/opengate-fix \
    target/x86_64-apple-darwin/release/opengate-fix

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Open Gate Fix</string>
    <key>CFBundleDisplayName</key><string>Open Gate Fix</string>
    <key>CFBundleIdentifier</key><string>io.github.itsallexb.opengatefix</string>
    <key>CFBundleExecutable</key><string>opengate-fix</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.video</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc signature: required on Apple silicon, but not a Developer ID.
codesign --force --sign - "$APP"
(cd dist && rm -f OpenGateFix-macos.zip && ditto -c -k --keepParent "Open Gate Fix.app" OpenGateFix-macos.zip)
echo "Built $APP"
