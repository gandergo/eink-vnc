use std::env;
use std::path::Path;

fn main() {
    let target = env::var("TARGET").unwrap();
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo:rerun-if-changed=build.rs");

    let abi = match target.as_str() {
        "arm-unknown-linux-musleabihf" => Some("musl"),
        "arm-unknown-linux-gnueabihf" | "armv7-unknown-linux-gnueabihf" => Some("gnueabihf"),
        _ => None,
    };

    if let Some(abi) = abi {
        let dir = format!("{manifest}/libs/{abi}");
        let archive = format!("{dir}/libfreetype.a");
        println!("cargo:rerun-if-changed={archive}");
        if !Path::new(&archive).is_file() {
            let prefix = if abi == "musl" {
                "arm-linux-musleabihf"
            } else {
                "arm-linux-gnueabihf"
            };
            panic!(
                "{archive} is missing. From the repository root run \
                 TOOL_PREFIX={prefix} ./client/scripts/build-freetype.sh"
            );
        }
        println!("cargo:rustc-link-search=native={dir}");
        println!("cargo:rustc-link-lib=static=freetype");
        println!("cargo:rustc-link-lib=m");
    } else {
        println!("cargo:rustc-link-lib=freetype");
    }
}
