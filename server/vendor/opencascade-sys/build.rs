use occt_sys::{occt_include_path, occt_lib_path};

fn main() {
    let target = std::env::var("TARGET").expect("No TARGET environment variable defined");
    let is_windows = target.to_lowercase().contains("windows");
    let is_windows_gnu = target.to_lowercase().contains("windows-gnu");

    println!("cargo:rustc-link-search=native={}", occt_lib_path().to_str().unwrap());
    // Link against shared OCCT libs. This avoids huge static links (and related
    // linker crashes / massive binaries) while still keeping OCCT as a build-time
    // dependency managed by `occt-sys`.
    println!("cargo:rustc-link-lib=dylib=TKMath");
    println!("cargo:rustc-link-lib=dylib=TKernel");
    println!("cargo:rustc-link-lib=dylib=TKFeat");
    println!("cargo:rustc-link-lib=dylib=TKGeomBase");
    println!("cargo:rustc-link-lib=dylib=TKG2d");
    println!("cargo:rustc-link-lib=dylib=TKG3d");
    println!("cargo:rustc-link-lib=dylib=TKTopAlgo");
    println!("cargo:rustc-link-lib=dylib=TKGeomAlgo");
    println!("cargo:rustc-link-lib=dylib=TKBRep");
    println!("cargo:rustc-link-lib=dylib=TKPrim");
    println!("cargo:rustc-link-lib=dylib=TKSTEP");
    println!("cargo:rustc-link-lib=dylib=TKSTEPAttr");
    println!("cargo:rustc-link-lib=dylib=TKSTEPBase");
    println!("cargo:rustc-link-lib=dylib=TKSTEP209");
    println!("cargo:rustc-link-lib=dylib=TKSTL");
    println!("cargo:rustc-link-lib=dylib=TKMesh");
    println!("cargo:rustc-link-lib=dylib=TKShHealing");
    println!("cargo:rustc-link-lib=dylib=TKFillet");
    println!("cargo:rustc-link-lib=dylib=TKBool");
    println!("cargo:rustc-link-lib=dylib=TKBO");
    println!("cargo:rustc-link-lib=dylib=TKOffset");
    println!("cargo:rustc-link-lib=dylib=TKXSBase");

    // Ensure binaries can find the cached "system OCCT" without LD_LIBRARY_PATH.
    // (Linux-only; other platforms will need different handling.)
    if !is_windows {
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}",
            occt_lib_path().to_string_lossy()
        );
    }

    if is_windows {
        println!("cargo:rustc-link-lib=dylib=user32");
    }

    let mut build = cxx_build::bridge("src/lib.rs");

    if is_windows_gnu {
        build.define("OCC_CONVERT_SIGNALS", "TRUE");
    }

    build
        .cpp(true)
        .flag_if_supported("-std=c++11")
        .define("_USE_MATH_DEFINES", "TRUE")
        .include(occt_include_path())
        .include("include")
        .compile("wrapper");

    println!("cargo:rustc-link-lib=static=wrapper");

    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=include/wrapper.hxx");
}
