use std::env;
use std::path::PathBuf;

fn main() {
    pyo3_build_config::use_pyo3_cfgs();

    // Get the i2pd vendor directory
    let i2pd_dir = PathBuf::from("vendor/i2pd");
    
    // Check if i2pd submodule exists and has content
    let i2pd_exists = i2pd_dir.exists() && i2pd_dir.join("CMakeLists.txt").exists();
    
    if i2pd_exists {
        // Configure CMake build for i2pd
        // CMake::Config::new() expects the SOURCE directory, not the build directory
        let mut cmake_config = cmake::Config::new(&i2pd_dir);
        
        // Set CMake options
        cmake_config
            .define("WITH_LIBRARY", "ON")
            .define("WITH_BINARY", "OFF")  // We only need the library
            .define("WITH_STATIC", "ON")   // Build static libraries
            .define("WITH_UPNP", "OFF")    // Disable UPnP for simplicity
            .build_arg("--parallel")
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
    } else {
        eprintln!("Warning: i2pd submodule not found or empty. Skipping i2pd build.");
        eprintln!("If you need i2pd, run: git submodule update --init --recursive");
    }
    
    // Link system libraries
    if cfg!(target_os = "windows") {
        println!("cargo:rustc-link-lib=wsock32");
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=iphlpapi");
    }
    
    // Link against Boost (only if i2pd exists)
    if i2pd_exists {
        // Try to use pkg-config for Boost on Unix systems
        if cfg!(unix) && !cfg!(target_os = "macos") {
            // Try pkg-config for Boost
            if let Ok(lib) = pkg_config::Config::new()
                .probe("libboost_filesystem")
                .or_else(|_| pkg_config::Config::new().probe("boost_filesystem"))
            {
                for path in lib.link_paths {
                    println!("cargo:rustc-link-search=native={}", path.display());
                }
            }
            if let Ok(lib) = pkg_config::Config::new()
                .probe("libboost_program_options")
                .or_else(|_| pkg_config::Config::new().probe("boost_program_options"))
            {
                for path in lib.link_paths {
                    println!("cargo:rustc-link-search=native={}", path.display());
                }
            }
        }
        
        // Link Boost libraries (pkg-config will handle the correct names if available)
        if cfg!(target_os = "windows") {
            println!("cargo:rustc-link-lib=boost_filesystem-mt");
            println!("cargo:rustc-link-lib=boost_program_options-mt");
        } else {
            // On Unix, try common names
            println!("cargo:rustc-link-lib=boost_filesystem");
            println!("cargo:rustc-link-lib=boost_program_options");
        }
    }
    
    // Link OpenSSL and zlib (only if i2pd exists, otherwise these are handled by dependencies)
    if i2pd_exists {
        println!("cargo:rustc-link-lib=ssl");
        println!("cargo:rustc-link-lib=crypto");
    }
    // zlib is usually always needed
    println!("cargo:rustc-link-lib=z");
    
    // Compile the C++ wrapper (only if i2pd exists)
    if i2pd_exists {
        let mut cpp_build = cc::Build::new();
        cpp_build
            .cpp(true)
            .file("vendor/i2pd_wrapper.cpp")
            .include(&i2pd_dir.join("libi2pd"))
            .include(&i2pd_dir.join("libi2pd_client"))
            .include(&i2pd_dir.join("libi2pd_wrapper"))
            .flag("-std=c++17");
        
        // Add compiler flags for better compatibility
        if cfg!(unix) {
            cpp_build.flag("-fPIC");
        }
        
        cpp_build.compile("i2pd_wrapper");
        println!("cargo:rustc-link-lib=static=i2pd_wrapper");
    } else {
        eprintln!("Warning: Skipping C++ wrapper compilation (i2pd not available)");
    }
    
    // Generate Rust bindings using bindgen
    let i2pd_wrapper_header = PathBuf::from("vendor/i2pd_wrapper.h");
    
    if !i2pd_wrapper_header.exists() {
        panic!("i2pd_wrapper.h not found at vendor/i2pd_wrapper.h");
    }
    
    let mut bindgen_builder = bindgen::Builder::default()
        .header(i2pd_wrapper_header.to_str().unwrap())
        .allowlist_function("i2pd_.*")
        .allowlist_type("i2pd_.*")
        // Include standard C headers for uint16_t, etc.
        .clang_arg("-include")
        .clang_arg("stdint.h");
    
    // Add include paths only if i2pd exists
    if i2pd_exists {
        let i2pd_include = i2pd_dir.join("libi2pd");
        let i2pd_client_include = i2pd_dir.join("libi2pd_client");
        let i2pd_wrapper_include = i2pd_dir.join("libi2pd_wrapper");
        
        if i2pd_include.exists() {
            bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", i2pd_include.display()));
        }
        if i2pd_client_include.exists() {
            bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", i2pd_client_include.display()));
        }
        if i2pd_wrapper_include.exists() {
            bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", i2pd_wrapper_include.display()));
        }
    }
    
    let bindings = bindgen_builder
        .generate()
        .expect("Unable to generate bindings");
    
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("i2pd_bindings.rs"))
        .expect("Couldn't write bindings!");
}
