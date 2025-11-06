use std::env;
use std::path::PathBuf;

/// Find OpenSSL include directory
fn find_openssl_include_dir() -> Option<PathBuf> {
    // Try environment variable first
    if let Ok(openssl_dir) = env::var("OPENSSL_DIR") {
        let include_dir = PathBuf::from(&openssl_dir).join("include");
        if include_dir.exists() {
            return Some(include_dir);
        }
    }
    
    // Try OPENSSL_INCLUDE_DIR
    if let Ok(include_dir) = env::var("OPENSSL_INCLUDE_DIR") {
        let path = PathBuf::from(&include_dir);
        if path.exists() {
            return Some(path);
        }
    }
    
    // Common Windows/MSYS2 paths
    if cfg!(target_os = "windows") {
        // Check BOOST_ROOT first (often OpenSSL is in same location)
        if let Ok(boost_root) = env::var("BOOST_ROOT") {
            let include_dir = PathBuf::from(&boost_root).join("include");
            let openssl_header = include_dir.join("openssl").join("dsa.h");
            if openssl_header.exists() {
                return Some(include_dir);
            }
        }
        
        // Try common MSYS2/MinGW paths
        let common_paths = vec![
            PathBuf::from("C:/msys64/mingw64/include"),
            PathBuf::from("C:/mingw64/include"),
            PathBuf::from("C:/msys64/usr/include"),
            PathBuf::from("C:/Program Files/OpenSSL-Win64/include"),
            PathBuf::from("C:/OpenSSL-Win64/include"),
        ];
        
        for path in common_paths {
            let openssl_header = path.join("openssl").join("dsa.h");
            if openssl_header.exists() {
                return Some(path);
            }
        }
        
        // Try to find MSYS2 installation via environment
        if let Ok(msystem) = env::var("MSYSTEM") {
            // MSYSTEM is like MINGW64, MINGW32, etc.
            if let Ok(msys_root) = env::var("MSYS_ROOT") {
                let include_dir = PathBuf::from(&msys_root).join("mingw64").join("include");
                let openssl_header = include_dir.join("openssl").join("dsa.h");
                if openssl_header.exists() {
                    return Some(include_dir);
                }
            }
        }
    }
    
    // Try pkg-config (Unix-like systems)
    #[cfg(unix)]
    {
        if let Ok(output) = std::process::Command::new("pkg-config")
            .args(&["--cflags", "openssl"])
            .output()
        {
            if let Ok(output_str) = String::from_utf8(output.stdout) {
                for flag in output_str.split_whitespace() {
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
    
    // Add OpenSSL include directory
    if let Some(openssl_include) = find_openssl_include_dir() {
        println!("cargo:warning=Found OpenSSL include directory: {}", openssl_include.display());
        cpp_build.include(&openssl_include);
    } else {
        // Try to get it from CMake's OpenSSL detection
        // CMake usually finds OpenSSL and stores it in CMAKE_PREFIX_PATH
        if let Ok(cmake_prefix) = env::var("CMAKE_PREFIX_PATH") {
            for prefix in cmake_prefix.split(";") {
                let include_dir = PathBuf::from(prefix).join("include");
                let openssl_header = include_dir.join("openssl").join("dsa.h");
                if openssl_header.exists() {
                    println!("cargo:warning=Found OpenSSL via CMAKE_PREFIX_PATH: {}", include_dir.display());
                    cpp_build.include(&include_dir);
                    break;
                }
            }
        }
        
        // Last resort: check if BOOST_ROOT is set (often OpenSSL is in same location)
        if let Ok(boost_root) = env::var("BOOST_ROOT") {
            let include_dir = PathBuf::from(&boost_root).join("include");
            let openssl_header = include_dir.join("openssl").join("dsa.h");
            if openssl_header.exists() {
                println!("cargo:warning=Found OpenSSL via BOOST_ROOT: {}", include_dir.display());
                cpp_build.include(&include_dir);
            }
        }
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
    
    // Add OpenSSL include directory to bindgen as well
    if let Some(openssl_include) = find_openssl_include_dir() {
        bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", openssl_include.display()));
    } else if let Ok(cmake_prefix) = env::var("CMAKE_PREFIX_PATH") {
        for prefix in cmake_prefix.split(";") {
            let include_dir = PathBuf::from(prefix).join("include");
            let openssl_header = include_dir.join("openssl").join("dsa.h");
            if openssl_header.exists() {
                bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", include_dir.display()));
                break;
            }
        }
    } else if let Ok(boost_root) = env::var("BOOST_ROOT") {
        let include_dir = PathBuf::from(&boost_root).join("include");
        let openssl_header = include_dir.join("openssl").join("dsa.h");
        if openssl_header.exists() {
            bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", include_dir.display()));
        }
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
