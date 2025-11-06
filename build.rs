use std::env;
use std::path::PathBuf;

fn find_openssl_include_dir() -> Option<PathBuf> {
    // Check OPENSSL_DIR environment variable
    if let Ok(openssl_dir) = env::var("OPENSSL_DIR") {
        let include_dir = PathBuf::from(&openssl_dir).join("include");
        if include_dir.exists() {
            return Some(include_dir);
        }
    }
    
    // Check OPENSSL_INCLUDE_DIR environment variable
    if let Ok(openssl_include) = env::var("OPENSSL_INCLUDE_DIR") {
        let include_dir = PathBuf::from(&openssl_include);
        if include_dir.exists() {
            return Some(include_dir);
        }
    }
    
    // Common Windows/MinGW paths
    if cfg!(target_os = "windows") {
        let common_paths = vec![
            PathBuf::from("C:/msys64/mingw64/include"),
            PathBuf::from("C:/msys64/usr/include"),
            PathBuf::from("C:/mingw64/include"),
            PathBuf::from("C:/mingw/include"),
        ];
        
        for path in common_paths {
            let openssl_path = path.join("openssl");
            if openssl_path.exists() {
                return Some(path);
            }
        }
    }
    
    // Try pkg-config (works on Unix-like systems)
    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(output) = std::process::Command::new("pkg-config")
            .args(&["--cflags", "openssl"])
            .output()
        {
            if output.status.success() {
                let flags = String::from_utf8_lossy(&output.stdout);
                for flag in flags.split_whitespace() {
                    if flag.starts_with("-I") {
                        let include_dir = PathBuf::from(&flag[2..]);
                        if include_dir.exists() {
                            return Some(include_dir);
                        }
                    }
                }
            }
        }
    }
    
    None
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
    
    // Find OpenSSL include directory
    let openssl_include = find_openssl_include_dir();
    if openssl_include.is_none() {
        println!("cargo:warning=OpenSSL include directory not found. If compilation fails, set OPENSSL_DIR or OPENSSL_INCLUDE_DIR environment variable.");
    }
    
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
    
    // Add OpenSSL include directory if found
    if let Some(ref openssl_inc) = openssl_include {
        cpp_build.include(openssl_inc);
        println!("cargo:warning=Using OpenSSL include directory: {}", openssl_inc.display());
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
    
    // Add OpenSSL include directory to bindgen if found
    if let Some(ref openssl_inc) = openssl_include {
        bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", openssl_inc.display()));
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
