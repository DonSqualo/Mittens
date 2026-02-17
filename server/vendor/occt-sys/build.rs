fn main() {
    const LIB_DIR: &str = "lib";
    const INCLUDE_DIR: &str = "include";

    // "System OCCT" for this repo: a single cached install prefix that occt-sys
    // can reuse across targets/profiles/worktrees, instead of rebuilding OCCT into
    // every `target/*/build/occt-sys-*` directory.
    //
    // Default: ~/.local/occt (user install, no sudo required).
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let default_prefix = format!("{home}/.local/occt");
    let prefix = std::env::var("MITTENS_OCCT_PREFIX").unwrap_or(default_prefix);
    println!("cargo:rerun-if-env-changed=MITTENS_OCCT_PREFIX");

    let prefix_path = std::path::PathBuf::from(&prefix);
    let lib_path = prefix_path.join(LIB_DIR);
    let include_path = prefix_path.join(INCLUDE_DIR);

    // Marker to avoid repeating expensive source builds.
    let marker = prefix_path.join(".mittens_occt_ok");

    let has_any = |dir: &std::path::Path, prefix: &str| -> bool {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return false;
        };
        for ent in rd.flatten() {
            let name = ent.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name.starts_with(prefix) {
                return true;
            }
        }
        false
    };

    let looks_installed = || {
        include_path.join("Standard_Version.hxx").is_file()
            // Prefer symlinks like libTKernel.so, but accept versioned libs too (libTKernel.so.7.x).
            && has_any(&lib_path, "libTKernel.so")
            && has_any(&lib_path, "libTKMath.so")
            && has_any(&lib_path, "libTKBRep.so")
            && has_any(&lib_path, "libTKSTEP.so")
            && has_any(&lib_path, "libTKSTEPBase.so")
            && has_any(&lib_path, "libTKXSBase.so")
    };

    if marker.is_file() && looks_installed() {
        println!("cargo:rustc-env=OCCT_LIB_PATH={}", lib_path.display());
        println!("cargo:rustc-env=OCCT_INCLUDE_PATH={}", include_path.display());
        return;
    }

    std::fs::create_dir_all(&prefix_path).expect("create MITTENS_OCCT_PREFIX");

    // Build OCCT once from the vendored source, but install into the cached prefix.
    let current_dir = std::env::current_dir().expect("Should have a 'current' directory");
    let patch_dir = current_dir.join("patch");

    let _dst = cmake::Config::new("OCCT")
        .define("BUILD_PATCH", patch_dir)
        .define("BUILD_LIBRARY_TYPE", "Shared")
        .define("CMAKE_BUILD_TYPE", "Release")
        .define("BUILD_MODULE_ApplicationFramework", "FALSE")
        .define("BUILD_MODULE_Draw", "FALSE")
        .define("USE_D3D", "FALSE")
        .define("USE_DRACO", "FALSE")
        .define("USE_EIGEN", "FALSE")
        .define("USE_FFMPEG", "FALSE")
        .define("USE_FREEIMAGE", "FALSE")
        .define("USE_FREETYPE", "FALSE")
        .define("USE_GLES2", "FALSE")
        .define("USE_OPENGL", "FALSE")
        .define("USE_OPENVR", "FALSE")
        .define("USE_RAPIDJSON", "FALSE")
        .define("USE_TBB", "FALSE")
        .define("USE_TCL", "FALSE")
        .define("USE_TK", "FALSE")
        .define("USE_VTK", "FALSE")
        .define("USE_XLIB", "FALSE")
        // Install into the cached prefix.
        .define("CMAKE_INSTALL_PREFIX", &prefix)
        .define("INSTALL_DIR_LIB", LIB_DIR)
        .define("INSTALL_DIR_INCLUDE", INCLUDE_DIR)
        .build();

    // `cmake::Config::build()` returns a path under OUT_DIR, even if we set
    // `CMAKE_INSTALL_PREFIX`. Always point consumers at the cached prefix.
    let lib_path = prefix_path.join(LIB_DIR);
    let include_path = prefix_path.join(INCLUDE_DIR);

    // Mark install as complete.
    let _ = std::fs::write(&marker, b"ok\n");

    println!("cargo:rustc-env=OCCT_LIB_PATH={}", lib_path.display());
    println!("cargo:rustc-env=OCCT_INCLUDE_PATH={}", include_path.display());
}
