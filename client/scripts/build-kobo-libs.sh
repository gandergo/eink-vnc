#!/bin/sh
# Build the static MuPDF archives the Kobo musl target links against.
# Requires arm-linux-musleabihf-gcc on PATH.
set -eu

cd "$(dirname "$0")/.."

if [ -f libs/libmupdf.a ] && [ -f libs/libmupdf-third.a ] && [ -f libs/libmupdf_wrapper.a ] \
    && [ -f libs/libfreetype.a ]; then
    echo "client/libs already has the native archives"
    exit 0
fi

command -v arm-linux-musleabihf-gcc >/dev/null
command -v arm-linux-musleabihf-ar >/dev/null

ROOT="${TMPDIR:-/tmp}/einkvnc-mupdf-1.27.0"
mkdir -p "$ROOT"
if [ ! -d "$ROOT/mupdf-1.27.0-source/include" ]; then
    curl -L --retry 3 -o "$ROOT/mupdf.tar.gz" \
        https://mupdf.com/downloads/archive/mupdf-1.27.0-source.tar.gz
    tar -C "$ROOT" -xzf "$ROOT/mupdf.tar.gz"
fi

# Do not pick the host's libfreetype via pkg-config; those objects are the wrong architecture.
export PKG_CONFIG=/bin/false

make -C "$ROOT/mupdf-1.27.0-source" -j"$(nproc)" build=release \
    CC=arm-linux-musleabihf-gcc \
    CXX=arm-linux-musleabihf-g++ \
    LD=arm-linux-musleabihf-ld \
    AR=arm-linux-musleabihf-ar \
    RANLIB=arm-linux-musleabihf-ranlib \
    HAVE_X11=no \
    HAVE_GLUT=no \
    HAVE_LIBCRYPTO=no \
    libs

# FreeType with variation support. MuPDF's bundled copy omits FT_Get_MM_Var,
# which the UI font loader calls, so ship an upstream static library and drop
# the bundled objects to avoid duplicate FT_* symbols.
if [ ! -f "$ROOT/freetype/lib/libfreetype.a" ]; then
    if [ ! -d "$ROOT/freetype-2.13.3" ]; then
        curl -L --retry 3 -o "$ROOT/freetype.tar.gz" \
            https://download.savannah.gnu.org/releases/freetype/freetype-2.13.3.tar.gz
        tar -C "$ROOT" -xzf "$ROOT/freetype.tar.gz"
    fi
    (
        cd "$ROOT/freetype-2.13.3"
        ./configure --host=arm-linux-musleabihf --prefix="$ROOT/freetype" \
            --enable-static --disable-shared \
            --without-png --without-zlib --without-brotli --without-harfbuzz \
            --without-bzip2
        make -j"$(nproc)"
        make install
    )
fi

mkdir -p libs
cp "$ROOT/mupdf-1.27.0-source/build/release/libmupdf.a" libs/libmupdf.a
cp "$ROOT/mupdf-1.27.0-source/build/release/libmupdf-third.a" libs/libmupdf-third.a
cp "$ROOT/freetype/lib/libfreetype.a" libs/libfreetype.a
arm-linux-musleabihf-ar d libs/libmupdf-third.a \
    ftbase.o ftbbox.o ftbitmap.o ftdebug.o ftfstype.o ftgasp.o ftglyph.o ftinit.o \
    ftstroke.o ftsynth.o ftsystem.o fttype1.o cff.o type1cid.o psaux.o pshinter.o \
    psnames.o raster.o sfnt.o smooth.o truetype.o type1.o

arm-linux-musleabihf-gcc -c -O2 \
    -I "$ROOT/mupdf-1.27.0-source/include" \
    -o "$ROOT/mupdf_wrapper.o" \
    native/mupdf_wrapper.c
arm-linux-musleabihf-ar rcs libs/libmupdf_wrapper.a "$ROOT/mupdf_wrapper.o"
echo "wrote client/libs/libmupdf.a client/libs/libmupdf-third.a client/libs/libfreetype.a client/libs/libmupdf_wrapper.a"
