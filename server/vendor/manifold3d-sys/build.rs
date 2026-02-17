use std::env;
use std::path::PathBuf;

fn feature_export() -> bool {
    cfg!(feature = "export")
}

fn feature_parallel() -> bool {
    cfg!(feature = "parallel")
}

fn feature_static() -> bool {
    cfg!(feature = "static")
}

fn main() {
    // "System Manifold" for this repo: a single cached install prefix that manifold3d-sys
    // can reuse across targets/profiles/worktrees, instead of rebuilding Manifold into
    // every `target/*/build/manifold3d-sys-*` directory.
    //
    // Default: ~/.local/manifold (user install, no sudo required).
    let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let default_prefix = format!("{home}/.local/manifold");
    let prefix = env::var("MITTENS_MANIFOLD_PREFIX").unwrap_or(default_prefix);
    println!("cargo:rerun-if-env-changed=MITTENS_MANIFOLD_PREFIX");

    let prefix_path = PathBuf::from(&prefix);
    let lib_path = prefix_path.join("lib");
    let include_path = prefix_path.join("include");

    // Marker to avoid repeating expensive source builds.
    let marker = prefix_path.join(".mittens_manifold_ok");

    let looks_installed = || {
        include_path.join("manifold").exists()
            && (lib_path.join("libmanifold.so").exists()
                || lib_path.join("libmanifold.so.1").exists()
                || lib_path.join("libmanifold.so.2").exists()
                || lib_path.join("libmanifold.dylib").exists()
                || lib_path.join("manifold.lib").exists())
    };

    if marker.is_file() && looks_installed() {
        // Prefer the cached prefix.
        println!("cargo:rustc-link-search=native={}", lib_path.display());
        println!(
            "cargo:rustc-link-lib={}=manifold",
            if feature_static() { "static" } else { "dylib" }
        );
        println!(
            "cargo:rustc-link-lib={}=manifoldc",
            if feature_static() { "static" } else { "dylib" }
        );
        generate_bindings(&PathBuf::from(env::var("OUT_DIR").unwrap()));
        return;
    }

    std::fs::create_dir_all(&prefix_path).expect("create MITTENS_MANIFOLD_PREFIX");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap();

    let mut cmake_config = cmake::Config::new("vendor/manifold");

    cmake_config
        .define("BUILD_SHARED_LIBS", if feature_static() { "OFF" } else { "ON" } )
        .define("MANIFOLD_TEST", "OFF")
        .define("MANIFOLD_CBIND", "ON")
        .define("MANIFOLD_CROSS_SECTION", "ON")
        .define("MANIFOLD_PAR", if feature_parallel() { "ON" } else { "OFF" })
        .define("MANIFOLD_EXPORT", if feature_export() { "ON" } else { "OFF" })
        .define("CMAKE_INSTALL_PREFIX", &prefix)
        .out_dir(out_dir.clone());

    if target_os == "windows" {
        cmake_config.cxxflag("/EHsc");
    }

    let _dst = cmake_config.build();

    if feature_export() {
        println!("cargo:rustc-link-lib=assimp");
    }
    if feature_parallel() {
        println!("cargo:rustc-link-lib=tbb");
    }

    // `cmake::Config::build()` returns a path under OUT_DIR; always point consumers at the cached prefix.
    println!("cargo:rustc-link-search=native={}", lib_path.display());
    println!("cargo:rustc-link-lib={}=manifold", if feature_static() { "static" } else { "dylib" });
    println!("cargo:rustc-link-lib={}=manifoldc", if feature_static() { "static" } else { "dylib" });

    // Mark install as complete.
    let _ = std::fs::write(&marker, b"ok\n");

    match (
        target_arch.as_str(),
        target_os.as_str(),
        target_env.as_str(),
    ) {
        (_, "linux", _) | (_, "windows", "gnu") | (_, "android", _) => {
            println!("cargo:rustc-link-lib=dylib=stdc++")
        }
        (_, "macos", _) | (_, "ios", _) => println!("cargo:rustc-link-lib=dylib=c++"),
        (_, "windows", "msvc") => {}
        ("wasm32", "emscripten", _) => {
            println!("cargo:rustc-link-arg=--no-entry");
        }
        _ => unimplemented!(
            "target_os: {}, target_env: {}",
            target_os.as_str(),
            target_env.as_str()
        ),
    }

    generate_bindings(&out_dir)
}

fn generate_bindings(out_dir: &PathBuf) {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap();

    let mut bindings_builder = bindgen::Builder::default()
        .header("vendor/manifold/bindings/c/include/manifold/manifoldc.h")
        .clang_arg("-Ivendor/manifold/bindings/c/include");

    if feature_export() {
        bindings_builder = bindings_builder.clang_arg("-DMANIFOLD_EXPORT");
    }

    let mut bindings_builder =
        bindings_builder.parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    if target_arch == "wasm32" && target_os == "emscripten" {
        // Workaround for bug:
        // https://github.com/rust-lang/rust-bindgen/issues/751
        bindings_builder = bindings_builder.clang_arg("-fvisibility=default");
    }

    bindings_builder
        .generate()
        .expect("Unable to generate bindings")
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
