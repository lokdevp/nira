#!/usr/bin/env bash
# Build a self-contained AppImage for Linux (x86_64) without linuxdeploy:
# the AppDir is assembled manually (binary + runtime libs via ldd + GTK data),
# and appimagetool produces the final .AppImage.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"   # workspace root
cd "$ROOT"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
TOOLS_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/nira-appimage"
APPDIR="dist/appimage/AppDir"
APPIMAGE="dist/nira-${VERSION}-x86_64.AppImage"

echo "==> Building release binary"
cargo build --release

mkdir -p "$TOOLS_DIR" dist/appimage

# ---- appimagetool (cached) ----
if [ ! -f "$TOOLS_DIR/appimagetool-x86_64.AppImage" ]; then
    echo "==> Downloading appimagetool"
    curl -fL --retry 3 -o "$TOOLS_DIR/appimagetool-x86_64.AppImage" \
        https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
    chmod +x "$TOOLS_DIR/appimagetool-x86_64.AppImage"
fi

echo "==> Assembling AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib" "$APPDIR/usr/share/applications" \
    "$APPDIR/usr/share/metainfo"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"

cp target/release/nira "$APPDIR/usr/bin/"
DESKTOP="$ROOT/packaging/linux/io.github.lokdevp.Nira.desktop"
cp "$DESKTOP" "$APPDIR/usr/share/applications/"
# appimagetool requires a .desktop file at the AppDir root.
cp "$DESKTOP" "$APPDIR/"
# AppStream metadata, named after the desktop file. appimagetool only looks
# for the legacy .appdata.xml name, which AppStream still reads.
cp "$ROOT/packaging/linux/io.github.lokdevp.Nira.metainfo.xml" \
    "$APPDIR/usr/share/metainfo/io.github.lokdevp.Nira.appdata.xml"
# appimagetool also requires the icon referenced by the desktop file
# (Icon=nira) at the AppDir root.
cp "$ROOT/packaging/linux/icons/256x256/apps/nira.png" "$APPDIR/nira.png"
cp "$ROOT/packaging/linux/icons/256x256/apps/nira.png" \
    "$APPDIR/usr/share/icons/hicolor/256x256/apps/nira.png"

# ---- bundle the runtime libraries (via ldd), excluding glibc internals ----
echo "==> Bundling shared libraries"
is_system_lib() {
    case "$(basename "$1")" in
        ld-linux*|libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*|libresolv.so*|libutil.so*|libnsl.so*|libnss_*|libnsl*|libBrokenLocale*|libanl.so*) return 0 ;;
        *) return 1 ;;
    esac
}
# Copy the libraries a binary or module links to into usr/lib.
# glibc is excluded, but libgcc_s/libstdc++ ARE bundled (portability).
bundle_deps() {
    ldd "$1" | awk '/=> \//{print $3}' | sort -u | while read -r lib; do
        if ! is_system_lib "$lib"; then
            cp -n "$lib" "$APPDIR/usr/lib/" 2>/dev/null || true
        fi
    done
}
bundle_deps target/release/nira

# ---- GTK runtime data ----
SHARE=/usr/share
# GSettings schemas. They are found through XDG_DATA_DIRS, so the host's
# schemas stay visible too.
if [ -f "$SHARE/glib-2.0/schemas/gschemas.compiled" ]; then
    mkdir -p "$APPDIR/usr/share/glib-2.0/schemas"
    cp "$SHARE/glib-2.0/schemas/gschemas.compiled" "$APPDIR/usr/share/glib-2.0/schemas/"
fi
# Adwaita icons.
if [ -d "$SHARE/icons/Adwaita" ]; then
    mkdir -p "$APPDIR/usr/share/icons"
    cp -r "$SHARE/icons/Adwaita" "$APPDIR/usr/share/icons/"
fi

# gdk-pixbuf loaders. GTK needs the SVG loader (librsvg) for Adwaita and the
# symbolic icons. The bundled gdk-pixbuf looks for loaders at its build-time
# path, which may not exist on the host, so the AppRun points it at a
# loaders.cache listing the bundled ones. The cache needs absolute paths, so
# it is written here with an @APPDIR@ placeholder that the AppRun fills in.
PIXBUF_BINVER="$(pkg-config --variable=gdk_pixbuf_binary_version gdk-pixbuf-2.0)"
PIXBUF_MODULEDIR="$(pkg-config --variable=gdk_pixbuf_moduledir gdk-pixbuf-2.0)"
PIXBUF_QUERY="$(pkg-config --variable=gdk_pixbuf_query_loaders gdk-pixbuf-2.0)"
LOADERS_DEST="$APPDIR/usr/lib/gdk-pixbuf-2.0/$PIXBUF_BINVER/loaders"
mkdir -p "$LOADERS_DEST"
if [ -d "$PIXBUF_MODULEDIR" ]; then
    for loader in "$PIXBUF_MODULEDIR"/*.so; do
        [ -e "$loader" ] || continue
        cp "$loader" "$LOADERS_DEST/"
        bundle_deps "$loader"
    done
fi
ABS_APPDIR="$(cd "$APPDIR" && pwd)"
GDK_PIXBUF_MODULEDIR="$(cd "$LOADERS_DEST" && pwd)" "$PIXBUF_QUERY" \
    | sed "s|$ABS_APPDIR|@APPDIR@|g" \
    > "$APPDIR/usr/lib/gdk-pixbuf-2.0/$PIXBUF_BINVER/loaders.cache.in"
echo "    $(ls "$LOADERS_DEST" | wc -l) pixbuf loaders bundled"

# The dconf GSettings backend. Without it GSettings falls back to memory and
# GTK can't see the desktop's settings (window buttons, fonts, icon theme).
GIO_MODULEDIR="$(pkg-config --variable=giomoduledir gio-2.0)"
if [ -f "$GIO_MODULEDIR/libdconfsettings.so" ]; then
    mkdir -p "$APPDIR/usr/lib/gio/modules"
    cp "$GIO_MODULEDIR/libdconfsettings.so" "$APPDIR/usr/lib/gio/modules/"
    bundle_deps "$GIO_MODULEDIR/libdconfsettings.so"
else
    echo "WARNING: libdconfsettings.so not found; GTK won't read desktop settings" >&2
fi

# ---- AppRun ----
cat > "$APPDIR/AppRun" << 'APPEOF'
#!/bin/sh
SELF="$(readlink -f "$0")"
HERE="${SELF%/*}"
export LD_LIBRARY_PATH="$HERE/usr/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
export XDG_DATA_DIRS="$HERE/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export GIO_MODULE_DIR="$HERE/usr/lib/gio/modules"
# The AppImage is mounted read-only, so the loader cache with this mount's
# paths goes in the user's cache folder, rewritten on every launch.
for template in "$HERE"/usr/lib/gdk-pixbuf-2.0/*/loaders.cache.in; do
    [ -f "$template" ] || continue
    cache_dir="${XDG_CACHE_HOME:-$HOME/.cache}/nira"
    if mkdir -p "$cache_dir" && \
        sed "s|@APPDIR@|$HERE|g" "$template" > "$cache_dir/appimage-loaders.cache"; then
        export GDK_PIXBUF_MODULE_FILE="$cache_dir/appimage-loaders.cache"
    fi
done
exec "$HERE/usr/bin/nira" "$@"
APPEOF
chmod +x "$APPDIR/AppRun"

echo "==> Building AppImage"
"$TOOLS_DIR/appimagetool-x86_64.AppImage" --appimage-extract-and-run "$APPDIR" "$APPIMAGE"

echo "==> Done:"
ls -lh "$APPIMAGE"
