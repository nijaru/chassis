#!/usr/bin/env bash
# Package every exported qualification fixture from a native release build.
set -euo pipefail

case "$(uname -s)" in
  Darwin) prefix=lib; suffix=dylib; macos=true ;;
  Linux) prefix=lib; suffix=so; macos=false ;;
  MINGW*|MSYS*|CYGWIN*) prefix=; suffix=dll; macos=false ;;
  *) printf 'Unsupported packaging platform\n' >&2; exit 1 ;;
esac

package() {
  local library=$1 name=$2 identifier=$3 display_name=$4
  local bundle="target/release/$name.clap"
  local source="target/release/$prefix$library.$suffix"
  if "$macos"; then
    mkdir -p "$bundle/Contents/MacOS"
    cp "$source" "$bundle/Contents/MacOS/$name"
    chmod +x "$bundle/Contents/MacOS/$name"
    printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
      '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
      '<plist version="1.0"><dict>' \
      '<key>CFBundleDevelopmentRegion</key><string>en</string>' \
      "<key>CFBundleExecutable</key><string>$name</string>" \
      "<key>CFBundleIdentifier</key><string>$identifier</string>" \
      "<key>CFBundleName</key><string>$display_name</string>" \
      '<key>CFBundlePackageType</key><string>BNDL</string>' \
      '<key>CFBundleShortVersionString</key><string>0.0.0</string>' \
      '<key>CFBundleVersion</key><string>1</string>' \
      '</dict></plist>' > "$bundle/Contents/Info.plist"
  else
    cp "$source" "$bundle"
  fi
}

package chassis_clap_conformance chassis-clap-conformance org.nijaru.chassis.conformance 'Chassis CLAP Conformance'
package chassis_clap_delayed_probe chassis-delayed-probe com.nijaru.chassis.delayed-probe 'Chassis Delayed Probe'
package chassis_clap_note_probe chassis-note-probe com.nijaru.chassis.note-probe 'Chassis Note Probe'
