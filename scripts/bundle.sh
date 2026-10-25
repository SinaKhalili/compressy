#!/bin/zsh
# Builds Compressy.app (and a disk image to carry it to another Mac) in dist/.
#
#   scripts/bundle.sh             # dist/Compressy.app and dist/Compressy-<version>.dmg
#   scripts/bundle.sh --install   # ...and puts the app in /Applications
#
# The binary is self-contained (fonts and icons are compiled in, only system
# libraries are linked), so the bundle is the binary, an Info.plist and the
# icon. It's signed ad hoc: fine on this Mac; another Mac will ask once, in
# System Settings → Privacy & Security, before it opens.
set -euo pipefail
cd "${0:A:h}/.."

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
app=dist/Compressy.app

cargo build --release

rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/compressy "$app/Contents/MacOS/compressy"

# The icon: the 1024 px master (scripts/icon.py draws it) at every size.
iconset=$(mktemp -d)/Compressy.iconset
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
  sips -z $size $size assets/icon-1024.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z $double $double assets/icon-1024.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/Compressy.icns"
rm -rf "${iconset:h}"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Compressy</string>
  <key>CFBundleDisplayName</key><string>Compressy</string>
  <key>CFBundleIdentifier</key><string>town.sina.compressy</string>
  <key>CFBundleExecutable</key><string>compressy</string>
  <key>CFBundleIconFile</key><string>Compressy</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${version}</string>
  <key>CFBundleVersion</key><string>${version}</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.video</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST
plutil -lint "$app/Contents/Info.plist" >/dev/null

codesign --force --sign - --timestamp=none "$app"
codesign --verify --strict "$app"

# A disk image with an Applications shortcut, to drag the app onto.
stage=$(mktemp -d)
cp -R "$app" "$stage/"
ln -s /Applications "$stage/Applications"
dmg="dist/Compressy-${version}.dmg"
rm -f "$dmg"
hdiutil create -quiet -volname Compressy -srcfolder "$stage" -format UDZO -ov "$dmg"
rm -rf "$stage"

echo "$app"
echo "$dmg"

if [[ "${1:-}" == "--install" ]]; then
  if pgrep -xq compressy; then
    echo "Compressy is running; quit it, then run this again." >&2
    exit 1
  fi
  rm -rf /Applications/Compressy.app
  cp -R "$app" /Applications/
  # Tell Launch Services about the new icon and bundle.
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/Compressy.app
  echo "/Applications/Compressy.app"
fi
