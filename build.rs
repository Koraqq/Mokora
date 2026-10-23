use std::{env, fs, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));

    // protoc is a native program that gets its arguments in the ANSI code page,
    // so it cannot write to a path with non-ASCII characters (a project folder
    // such as `文件\Mokora`). It writes the descriptor set to the temp folder
    // and the result is copied into OUT_DIR.
    let desc_tmp = env::temp_dir().join(format!("mokora-liqi-desc-{}.bin", std::process::id()));

    prost_build::Config::new()
        .type_attribute(".", "#[allow(dead_code)]")
        .file_descriptor_set_path(&desc_tmp)
        .compile_protos(
            &["src/bridge/majsoul/proto/liqi.proto"],
            &["src/bridge/majsoul/proto/"],
        )
        .expect("failed to compile liqi.proto");
    fs::copy(&desc_tmp, out_dir.join("liqi_desc.bin")).expect("failed to copy liqi_desc.bin");
    let _ = fs::remove_file(&desc_tmp);

    println!("cargo:rerun-if-changed=src/bridge/majsoul/proto/liqi.proto");
    println!("cargo:rerun-if-changed=build.rs");

    // Surface the build target triple to runtime code so it can pick the
    // right bundled python-build-standalone / uv binary out of the Tauri
    // resource dir. Cargo only exposes this via the `TARGET` env var at
    // build time; we forward it as `TARGET_TRIPLE` for the binary.
    let target = env::var("TARGET").expect("TARGET not set by cargo");
    println!("cargo:rustc-env=TARGET_TRIPLE={target}");

    // The Windows app manifest (Common Controls v6, which Tauri's windowing
    // code calls into) goes into every binary the linker makes, the test
    // harness included. Left to Tauri it only reaches the app's exe, and a
    // test binary that links that code fails to start with
    // STATUS_ENTRYPOINT_NOT_FOUND. Same content as Tauri's default.
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    if msvc {
        let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set"))
            .join("windows-app-manifest.xml");
        println!("cargo:rerun-if-changed=windows-app-manifest.xml");
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        // No UAC section of the linker's own: the manifest stays as Tauri's.
        println!("cargo:rustc-link-arg=/MANIFESTUAC:NO");
        println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
        tauri_build::try_build(
            tauri_build::Attributes::new()
                .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
        )
        .expect("tauri build");
    } else {
        tauri_build::build();
    }
}
