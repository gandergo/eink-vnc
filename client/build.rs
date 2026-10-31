use std::env;

fn main() {
    let target = env::var("TARGET").unwrap();

    // Cross-compiling for Kobo.
    if target == "arm-unknown-linux-gnueabihf" {
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
