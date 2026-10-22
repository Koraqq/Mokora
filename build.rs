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

    tauri_build::build();
}
