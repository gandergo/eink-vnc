#!/bin/bash
# Build static FreeType 2.13.3 for one Kobo toolchain.
# TOOL_PREFIX=arm-linux-musleabihf or arm-linux-gnueabihf.
# HarfBuzz, MuPDF, zlib, png, brotli, and bzip2 are not linked.
set -eu

TOOL_PREFIX="${TOOL_PREFIX:?set TOOL_PREFIX to arm-linux-musleabihf or arm-linux-gnueabihf}"
case "$TOOL_PREFIX" in
    arm-linux-musleabihf) ABI=musl ;;
    arm-linux-gnueabihf) ABI=gnueabihf ;;
    *)
        echo "unsupported TOOL_PREFIX=$TOOL_PREFIX" >&2
        exit 1
        ;;
esac

ROOT=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
for dir in \
    /opt/arm-linux-musleabihf-cross/bin \
    /tmp/toolchains/arm-linux-musleabihf-cross/bin \
    /opt/gcc-linaro-4.9.4-2017.01-x86_64_arm-linux-gnueabihf/bin
do
    if [ -x "$dir/${TOOL_PREFIX}-gcc" ]; then
        PATH="$dir:$PATH"
        break
    fi
done
command -v "${TOOL_PREFIX}-gcc" >/dev/null
command -v gcc >/dev/null

SHA256=5c3a8e78f7b24c20b25b54ee575d6daa40007a5f4eea2845861c3409b3021747
SRC="${TMPDIR:-/tmp}/einkvnc-freetype-2.13.3-${TOOL_PREFIX}"
mkdir -p "$SRC"
if [ ! -d "$SRC/freetype-2.13.3" ]; then
    tarball="$SRC/freetype-2.13.3.tar.gz"
    ok=0
    for url in \
        https://downloads.sourceforge.net/project/freetype/freetype2/2.13.3/freetype-2.13.3.tar.gz \
        https://mirror.accum.se/mirror/gnu.org/savannah/freetype/freetype-2.13.3.tar.gz \
        https://download.savannah.gnu.org/releases/freetype/freetype-2.13.3.tar.gz
    do
        if curl -fL --retry 5 --retry-all-errors -o "$tarball" "$url"; then
            ok=1
            break
        fi
    done
    [ "$ok" -eq 1 ]
    echo "$SHA256  $tarball" | sha256sum -c -
    tar -C "$SRC" -xzf "$tarball"
fi

(
    cd "$SRC/freetype-2.13.3"
    make distclean >/dev/null 2>&1 || true
    # CC_BUILD compiles the host-side apinames tool. PKG_CONFIG is disabled so
    # configure cannot pick the host FreeType or HarfBuzz.
    # -fPIC: the armv7 debug target links a PIE executable.
    PKG_CONFIG=/bin/false \
    CC_BUILD=gcc \
    CFLAGS="-O2 -fPIC" \
    ./configure --host="$TOOL_PREFIX" --prefix="$SRC/install" \
        --enable-static --disable-shared \
        --without-png --without-zlib --without-brotli --without-harfbuzz \
        --without-bzip2
    make -j"$(nproc)"
    make install
)

mkdir -p "$ROOT/libs/$ABI"
cp "$SRC/install/lib/libfreetype.a" "$ROOT/libs/$ABI/libfreetype.a"
echo "wrote $ROOT/libs/$ABI/libfreetype.a"
