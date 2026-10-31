#!/bin/sh
# Build the static MuPDF archives the Kobo musl target links against.
# Requires arm-linux-musleabihf-gcc on PATH.
set -eu

# Resolve from this script so the caller's working directory does not matter.
CLIENT_DIR=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
cd "$CLIENT_DIR"

# #[link(name="harfbuzz")] still asks for this archive on some checkouts.
# It only has to exist: build.rs redirects hb_* onto MuPDF's fzhb_* symbols.
mkdir -p libs

# MuPDF's fz_hb_malloc faults unless fz_hb_lock stored a context. The UI
# shapes text without that lock, so provide libc allocators instead.
HB_SRC=""
for candidate in \
    "$CLIENT_DIR/native/hb_alloc.c" \
    "$CLIENT_DIR/ native/hb_alloc.c"
do
    if [ -f "$candidate" ]; then
        HB_SRC=$candidate
        break
    fi
done
if [ -z "$HB_SRC" ]; then
    echo "hb_alloc.c was not found under $CLIENT_DIR" >&2
    exit 1
fi
command -v arm-linux-musleabihf-gcc >/dev/null
command -v arm-linux-musleabihf-ar >/dev/null
# A fixed member name so a rerun replaces the object instead of appending one.
arm-linux-musleabihf-gcc -c -O2 -o libs/hb_alloc.o "$HB_SRC"
rm -f libs/libhballoc.a
arm-linux-musleabihf-ar rcs libs/libhballoc.a libs/hb_alloc.o
rm -f libs/hb_alloc.o

# Drop MuPDF's own copies. They allocate through a context the UI never sets,
# and they collide with libhballoc.a if both stay in the link.
if [ -f libs/libmupdf.a ] && arm-linux-musleabihf-ar t libs/libmupdf.a | grep -qx harfbuzz.o; then
    arm-linux-musleabihf-ar d libs/libmupdf.a harfbuzz.o
fi

if [ ! -f libs/libharfbuzz.a ]; then
    command -v arm-linux-musleabihf-gcc >/dev/null
    command -v arm-linux-musleabihf-ar >/dev/null
    stub=$(mktemp)
    echo 'void einkvnc_harfbuzz_stub(void) {}' | arm-linux-musleabihf-gcc -c -x c -o "$stub" -
    arm-linux-musleabihf-ar rcs libs/libharfbuzz.a "$stub"
    rm -f "$stub"
fi

write_font_aliases() {
    command -v arm-linux-musleabihf-gcc >/dev/null
    command -v arm-linux-musleabihf-nm >/dev/null
    symbols=$(mktemp)
    wanted=$(mktemp)
    arm-linux-musleabihf-nm -g --defined-only libs/libmupdf.a > "$symbols"
    grep -o '_binary_[A-Za-z0-9_]*' src/font/mod.rs | grep -v resources_fonts | sort -u > "$wanted"
    awk -v wanted="$wanted" '
        BEGIN {
            while ((getline name < wanted) > 0) need[name] = 1
        }
        $NF ~ /^_binary_resources_fonts_(droid|noto)_.*_start$/ {
            long = $NF
            body = long
            sub(/^_binary_resources_fonts_(droid|noto)_/, "", body)
            sub(/_start$/, "", body)
            short = "_binary_" body
            if (need[short]) print short, long
        }
    ' "$symbols" > libs/font-aliases.txt
    rm -f "$symbols" "$wanted"
    test "$(wc -l < libs/font-aliases.txt)" -gt 100
}

if [ -f libs/libmupdf.a ] && [ -f libs/libmupdf-third.a ] && [ -f libs/libmupdf_wrapper.a ] \
    && [ -f libs/libfreetype.a ]; then
    write_font_aliases
    echo "client/libs already has the native archives"
    exit 0
fi

command -v arm-linux-musleabihf-gcc >/dev/null
command -v arm-linux-musleabihf-ar >/dev/null

# Try each URL. mupdf.com redirects to casper.mupdf.com, and that TLS handshake
# times out from GitHub runners (curl exit 28) if left to curl's default wait.
fetch_file() {
    dest=$1
    shift
    rm -f "$dest"
    for url in "$@"; do
        echo "fetching $url"
        if curl -fL --connect-timeout 20 --max-time 180 --retry 2 --retry-all-errors \
            -o "$dest" "$url"; then
            return 0
        fi
        echo "could not fetch $url" >&2
        rm -f "$dest"
    done
    echo "no download succeeded for $(basename "$dest")" >&2
    return 1
}

check_sha256() {
    file=$1
    want=$2
    got=$(sha256sum "$file" | awk '{print $1}')
    if [ "$got" != "$want" ]; then
        echo "checksum mismatch for $file" >&2
        echo "got  $got" >&2
        echo "want $want" >&2
        return 1
    fi
}

ROOT="${TMPDIR:-/tmp}/einkvnc-mupdf-1.27.0"
mkdir -p "$ROOT"
if [ ! -d "$ROOT/mupdf-1.27.0-source/include" ]; then
    unpacked=0
    if fetch_file "$ROOT/mupdf.tar.gz" \
        "https://mupdf.com/downloads/archive/mupdf-1.27.0-source.tar.gz"; then
        if check_sha256 "$ROOT/mupdf.tar.gz" \
            ae2442416de499182d37a526c6fa2bacc7a3bed5a888d113ca04844484dfe7c6; then
            tar -C "$ROOT" -xzf "$ROOT/mupdf.tar.gz"
            unpacked=1
        fi
    fi
    if [ "$unpacked" -eq 0 ]; then
        # Same tar as the .gz above, recompressed. sources.voidlinux.org is reachable
        # when casper.mupdf.com is not.
        if ! command -v lzip >/dev/null; then
            if command -v sudo >/dev/null && command -v apt-get >/dev/null; then
                sudo apt-get update
                sudo apt-get install -y lzip
            fi
        fi
        if ! command -v lzip >/dev/null; then
            echo "lzip is required to unpack the MuPDF mirror" >&2
            exit 1
        fi
        fetch_file "$ROOT/mupdf.tar.lz" \
            "https://sources.voidlinux.org/mupdf-1.27.0/mupdf-1.27.0-source.tar.lz"
        check_sha256 "$ROOT/mupdf.tar.lz" \
            61b109c82093a1c39b1b7f58c04b6d8ad6fe8e83e266ce1d8423e1e5c9310911
        lzip -dc "$ROOT/mupdf.tar.lz" | tar -C "$ROOT" -xf -
    fi
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
        fetch_file "$ROOT/freetype.tar.gz" \
            "https://downloads.sourceforge.net/project/freetype/freetype2/2.13.3/freetype-2.13.3.tar.gz" \
            "https://mirror.accum.se/mirror/gnu.org/savannah/freetype/freetype-2.13.3.tar.gz" \
            "https://download.savannah.gnu.org/releases/freetype/freetype-2.13.3.tar.gz"
        check_sha256 "$ROOT/freetype.tar.gz" \
            5c3a8e78f7b24c20b25b54ee575d6daa40007a5f4eea2845861c3409b3021747
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
if arm-linux-musleabihf-ar t libs/libmupdf.a | grep -qx harfbuzz.o; then
    arm-linux-musleabihf-ar d libs/libmupdf.a harfbuzz.o
fi
cp "$ROOT/mupdf-1.27.0-source/build/release/libmupdf-third.a" libs/libmupdf-third.a
cp "$ROOT/freetype/lib/libfreetype.a" libs/libfreetype.a
arm-linux-musleabihf-ar d libs/libmupdf-third.a \
    ftbase.o ftbbox.o ftbitmap.o ftdebug.o ftfstype.o ftgasp.o ftglyph.o ftinit.o \
    ftstroke.o ftsynth.o ftsystem.o fttype1.o cff.o type1cid.o psaux.o pshinter.o \
    psnames.o raster.o sfnt.o smooth.o truetype.o type1.o

# Prefer client/native. Also accept "client/ native": a checkout where the
# directory name itself has a leading space, which gcc cannot open as native/.
WRAPPER=""
for candidate in \
    "$CLIENT_DIR/native/mupdf_wrapper.c" \
    "$CLIENT_DIR/ native/mupdf_wrapper.c"
do
    if [ -f "$candidate" ]; then
        WRAPPER=$candidate
        break
    fi
done
if [ -z "$WRAPPER" ]; then
    echo "mupdf_wrapper.c was not found under $CLIENT_DIR" >&2
    find "$CLIENT_DIR" -name 'mupdf_wrapper.c' -print >&2 || true
    exit 1
fi

arm-linux-musleabihf-gcc -c -O2 \
    -I "$ROOT/mupdf-1.27.0-source/include" \
    -o "$ROOT/mupdf_wrapper.o" \
    "$WRAPPER"
arm-linux-musleabihf-ar rcs libs/libmupdf_wrapper.a "$ROOT/mupdf_wrapper.o"
write_font_aliases
echo "wrote client/libs/libmupdf.a client/libs/libmupdf-third.a client/libs/libfreetype.a client/libs/libmupdf_wrapper.a"
