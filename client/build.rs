use std::env;
use std::path::Path;

fn link_bundled_mupdf(manifest: &str, system_libs: &[&str]) {
    // Static MuPDF from scripts/build-kobo-libs.sh. FreeType and HarfBuzz
    // live in libmupdf-third.a; scan the group so either order resolves.
    println!("cargo:rustc-link-search=native=libs");
    // Plato's arm objects use short _binary_* font names. Force MuPDF's
    // long symbols out of the archive before aliasing the short names.
    let aliases = format!("{manifest}/libs/font-aliases.txt");
    println!("cargo:rerun-if-changed={aliases}");
    let text = std::fs::read_to_string(&aliases)
        .unwrap_or_else(|err| panic!("read {aliases}: {err}. Run scripts/build-kobo-libs.sh"));
    let mut pairs = Vec::new();
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(short) = parts.next() else { continue };
        let Some(long) = parts.next() else { continue };
        pairs.push((short.to_string(), long.to_string()));
    }
    for (_, long) in &pairs {
        println!("cargo:rustc-link-arg=-Wl,--undefined={long}");
    }
    // Define fz_hb_* before libmupdf.a. Otherwise MuPDF's copies run
    // fz_malloc on a null context and the first label segfaults.
    println!("cargo:rustc-link-arg=-Wl,--whole-archive");
    println!("cargo:rustc-link-arg=-lhballoc");
    println!("cargo:rustc-link-arg=-Wl,--no-whole-archive");
    println!("cargo:rustc-link-arg=-Wl,--start-group");
    println!("cargo:rustc-link-arg=-lmupdf_wrapper");
    println!("cargo:rustc-link-arg=-lmupdf");
    println!("cargo:rustc-link-arg=-lfreetype");
    println!("cargo:rustc-link-arg=-lmupdf-third");
    // rustc passes -nodefaultlibs and links -lc before these archives.
    // Rescan libc, libm, libpthread, and libgcc after MuPDF.
    for lib in system_libs {
        println!("cargo:rustc-link-arg=-l{lib}");
    }
    println!("cargo:rustc-link-arg=-Wl,--end-group");
    for (short, long) in &pairs {
        println!("cargo:rustc-link-arg=-Wl,--defsym={short}={long}");
    }
    // MuPDF renames the HarfBuzz C API to fzhb_*. Point the viewer's
    // hb_* calls at that copy so we do not link a second HarfBuzz.
    for name in [
        "hb_ft_font_create",
        "hb_ft_font_changed",
        "hb_font_destroy",
        "hb_buffer_create",
        "hb_buffer_destroy",
        "hb_buffer_clear_contents",
        "hb_buffer_add_utf8",
        "hb_buffer_set_direction",
        "hb_buffer_guess_segment_properties",
        "hb_shape",
        "hb_feature_from_string",
        "hb_buffer_get_length",
        "hb_buffer_get_glyph_infos",
        "hb_buffer_get_glyph_positions",
        "hb_buffer_get_direction",
        "hb_buffer_get_language",
        "hb_buffer_get_script",
    ] {
        let fzhb = name.replacen("hb_", "fzhb_", 1);
        println!("cargo:rustc-link-arg=-Wl,--defsym={name}={fzhb}");
    }
    println!("cargo:rustc-link-lib=static=stdc++");
}

fn expect_toolchain(manifest: &str, prefix: &str) {
    let path = format!("{manifest}/libs/toolchain-stamp");
    let stamp = std::fs::read_to_string(&path).unwrap_or_default();
    let stamp = stamp.trim();
    if stamp != prefix {
        panic!(
            "{path} says {stamp:?}; expected {prefix}. \
             Run TOOL_PREFIX={prefix} scripts/build-kobo-libs.sh"
        );
    }
}

fn main() {
    let target = env::var("TARGET").unwrap();
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();

    // Cross-compiling for Kobo.
    // The Linux instructions build armv7-unknown-linux-gnueabihf.
    // arm-unknown-linux-gnueabihf is the same hard-float ABI under the old triple.
    if target == "arm-unknown-linux-gnueabihf" || target == "armv7-unknown-linux-gnueabihf" {
        println!("cargo:rustc-env=PKG_CONFIG_ALLOW_CROSS=1");
        // glibc (unlike musl) keeps dlopen in libdl. Linaro's libstdc++ is
        // what -static-libstdc++ must find; Ubuntu's needs GLIBC_2.38.
        let linaro_lib =
            "/opt/gcc-linaro-4.9.4-2017.01-x86_64_arm-linux-gnueabihf/arm-linux-gnueabihf/lib";
        if Path::new(linaro_lib).is_dir() {
            println!("cargo:rustc-link-arg=-L{linaro_lib}");
        }
        expect_toolchain(&manifest, "arm-linux-gnueabihf");
        link_bundled_mupdf(&manifest, &["m", "c", "pthread", "dl", "gcc"]);
    } else if target == "arm-unknown-linux-musleabihf" {
        expect_toolchain(&manifest, "arm-linux-musleabihf");
        link_bundled_mupdf(&manifest, &["m", "c", "pthread", "gcc"]);
    // Handle the Linux and macOS platforms.
    } else {
        let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

        match target_os.as_ref() {
            "linux" => {
                println!("cargo:rustc-link-lib=dylib=stdc++");
            }
            "macos" => {
                println!("cargo:rustc-link-lib=dylib=c++");
            }
            _ => panic!("Unsupported platform: {}.", target_os),
        }
    }
}
