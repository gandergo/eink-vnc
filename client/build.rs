use std::env;

fn main() {
    let target = env::var("TARGET").unwrap();

    // Cross-compiling for Kobo.
    // The Linux instructions build armv7-unknown-linux-gnueabihf.
    // arm-unknown-linux-gnueabihf is the same soft-float-off ABI under the old triple.
    if target == "arm-unknown-linux-gnueabihf" || target == "armv7-unknown-linux-gnueabihf" {
        println!("cargo:rustc-env=PKG_CONFIG_ALLOW_CROSS=1");
        println!("cargo:rustc-link-search=libs");
        println!("cargo:rustc-link-lib=dylib=stdc++");
        println!("cargo:rustc-link-lib=z");
        println!("cargo:rustc-link-lib=bz2");
        println!("cargo:rustc-link-lib=jpeg");
        println!("cargo:rustc-link-lib=png16");
        println!("cargo:rustc-link-lib=gumbo");
        println!("cargo:rustc-link-lib=openjp2");
        println!("cargo:rustc-link-lib=jbig2dec");
    } else if target == "arm-unknown-linux-musleabihf" {
        // Static MuPDF from scripts/build-kobo-libs.sh. FreeType and HarfBuzz
        // live in libmupdf-third.a; scan the group so either order resolves.
        println!("cargo:rustc-link-search=native=libs");
        // Plato's arm objects use short _binary_* font names. Force MuPDF's
        // long symbols out of the archive before aliasing the short names.
        let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
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
        println!("cargo:rustc-link-arg=-lm");
        println!("cargo:rustc-link-arg=-lc");
        println!("cargo:rustc-link-arg=-lpthread");
        println!("cargo:rustc-link-arg=-lgcc");
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
