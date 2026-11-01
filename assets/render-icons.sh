#!/usr/bin/env bash
# Render every icon file from assets/nira.svg. Run from the crate root after
# editing the SVG; the outputs are committed so builds don't need these tools.
# Needs: rsvg-convert (librsvg), magick (ImageMagick 7).
set -euo pipefail

SVG=assets/nira.svg
ICONS=packaging/linux/icons

for n in 16 22 24 32 48 256; do
    mkdir -p "$ICONS/${n}x${n}/apps"
    rsvg-convert -w "$n" -h "$n" "$SVG" -o "$ICONS/${n}x${n}/apps/nira.png"
done

# About dialog logo.
rsvg-convert -w 256 -h 256 "$SVG" -o assets/nira-about.png

# Windows icon (installer and Start-menu shortcut).
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
for n in 16 24 32 48 256; do
    rsvg-convert -w "$n" -h "$n" "$SVG" -o "$tmp/$n.png"
done
magick "$tmp"/16.png "$tmp"/24.png "$tmp"/32.png "$tmp"/48.png "$tmp"/256.png assets/nira.ico
