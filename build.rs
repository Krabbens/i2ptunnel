use std::env;
use std::path::PathBuf;

/// Find OpenSSL include directories
fn find_openssl_includes() -> Vec<PathBuf> {
    let mut includes = Vec::new();
    
    // Try environment variables first
    if let Ok(openssl_dir) = env::var("OPENSSL_DIR") {
        let include_dir = PathBuf::from(&openssl_dir).join("include");
        if include_dir.exists() {
            includes.push(include_dir);
        }
    }
    
    if let Ok(openssl_include) = env::var("OPENSSL_INCLUDE_DIR") {
        let include_dir = PathBuf::from(&openssl_include);
        if include_dir.exists() {
            includes.push(include_dir);
        }
    }
    
    // On Windows with MinGW, try common locations
    if cfg!(target_os = "windows") {
        // Try to infer from compiler path (e.g., C:/ProgramData/mingw64/mingw64)
        if let Ok(cc) = env::var("CC") {
            if let Some(mingw_base) = PathBuf::from(&cc).parent().and_then(|p| p.parent()) {
                let include_dir = mingw_base.join("include");
                if include_dir.exists() {
                    includes.push(include_dir);
                }
            }
        }
        
        // Common MinGW/MSYS2 locations
        let common_paths = vec![
            "C:/msys64/mingw64/include",
            "C:/msys64/usr/include",
            "C:/ProgramData/mingw64/mingw64/include",
            "C:/mingw64/include",
        ];
        
        for path_str in common_paths {
            let path = PathBuf::from(path_str);
            if path.exists() && !includes.contains(&path) {
                includes.push(path);
            }
        }
    }
    
    // On Unix-like systems, try pkg-config
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(output) = std::process::Command::new("pkg-config")
            .args(&["--cflags-only-I", "openssl"])
            .output()
        {
            if let Ok(cflags) = String::from_utf8(output.stdout) {
                for flag in cflags.split_whitespace() {
                    if flag.starts_with("-I") {
                        let include_dir = PathBuf::from(&flag[2..]);
                        if include_dir.exists() && !includes.contains(&include_dir) {
                            includes.push(include_dir);
                        }
                    }
                }
            }
        }
    }
    
    includes
}

fn main() {
    pyo3_build_config::use_pyo3_cfgs();

    // Get the i2pd vendor directory
    let i2pd_dir = PathBuf::from("vendor/i2pd");
    let i2pd_build_dir = i2pd_dir.join("build");
    
    if !i2pd_dir.exists() {
        panic!("i2pd submodule not found. Please run: git submodule update --init --recursive");
    }
    
    if !i2pd_build_dir.exists() {
        panic!("i2pd build directory not found. Expected: {}", i2pd_build_dir.display());
    }

    // Configure CMake build for i2pd
    // CMakeLists.txt is in vendor/i2pd/build/, and it expects source in parent directory
    let mut cmake_config = cmake::Config::new(&i2pd_build_dir);
    
    // Set CMake options
    cmake_config
        .define("WITH_LIBRARY", "ON")
        .define("WITH_BINARY", "OFF")  // We only need the library
        .define("WITH_STATIC", "ON")   // Build static libraries
        .define("WITH_UPNP", "OFF")    // Disable UPnP for simplicity
        // Help CMake find Boost
        .define("Boost_NO_BOOST_CMAKE", "ON")
        .build_arg(format!("-j{}", num_cpus::get()));

    // Build i2pd libraries
    let i2pd_dst = cmake_config.build();
    
    // Output library search paths
    println!("cargo:rustc-link-search=native={}/lib", i2pd_dst.display());
    println!("cargo:rustc-link-search=native={}/lib64", i2pd_dst.display());
    
    // Link against i2pd libraries
    println!("cargo:rustc-link-lib=static=i2pd");
    println!("cargo:rustc-link-lib=static=i2pdclient");
    println!("cargo:rustc-link-lib=static=i2pdlang");
    
    // Link system libraries
    if cfg!(target_os = "windows") {
        println!("cargo:rustc-link-lib=wsock32");
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=iphlpapi");
    }
    
    // Link against Boost and OpenSSL (these should be found by CMake)
    println!("cargo:rustc-link-lib=boost_filesystem");
    println!("cargo:rustc-link-lib=boost_program_options");
    println!("cargo:rustc-link-lib=ssl");
    println!("cargo:rustc-link-lib=crypto");
    println!("cargo:rustc-link-lib=z");
    
    // Find OpenSSL include directories
    let openssl_includes = find_openssl_includes();
    
    // Compile the C++ wrapper
    let mut cpp_build = cc::Build::new();
    cpp_build
        .cpp(true)
        .file("vendor/i2pd_wrapper.cpp")
        .include(&i2pd_dir)  // Add i2pd root so "libi2pd_wrapper/capi.h" resolves correctly
        .include(&i2pd_dir.join("libi2pd"))
        .include(&i2pd_dir.join("libi2pd_client"))
        .include(&i2pd_dir.join("libi2pd_wrapper"))
        .include(&i2pd_dir.join("i18n"));  // For I18N_langs.h
    
    // Add OpenSSL include directories
    for include_dir in &openssl_includes {
        cpp_build.include(include_dir);
    }
    
    cpp_build
        .flag("-std=c++17")
        .compile("i2pd_wrapper");
    
    println!("cargo:rustc-link-lib=static=i2pd_wrapper");
    
    // Generate Rust bindings using bindgen
    let i2pd_wrapper_header = PathBuf::from("vendor/i2pd_wrapper.h");
    let i2pd_include = i2pd_dir.join("libi2pd");
    let i2pd_client_include = i2pd_dir.join("libi2pd_client");
    let i2pd_wrapper_include = i2pd_dir.join("libi2pd_wrapper");
    let _i2pd_api_include = i2pd_dir.join("libi2pd/api.h");
    
    let mut bindgen_builder = bindgen::Builder::default()
        .header(i2pd_wrapper_header.to_str().unwrap())
        .clang_arg(format!("-I{}", i2pd_dir.display()))  // Add i2pd root for includes
        .clang_arg(format!("-I{}", i2pd_include.display()))
        .clang_arg(format!("-I{}", i2pd_client_include.display()))
        .clang_arg(format!("-I{}", i2pd_wrapper_include.display()));
    
    // Add OpenSSL include directories to bindgen
    for include_dir in &openssl_includes {
        bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", include_dir.display()));
    }
    
    let bindings = bindgen_builder
        .allowlist_function("i2pd_.*")
        .allowlist_type("i2pd_.*")
        .generate()
        .expect("Unable to generate bindings");
    
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("i2pd_bindings.rs"))
        .expect("Couldn't write bindings!");
}
