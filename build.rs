use std::env;
use std::path::PathBuf;

fn main() {
    pyo3_build_config::use_pyo3_cfgs();

    // Get the i2pd vendor directory
    let i2pd_dir = PathBuf::from("vendor/i2pd");
    let i2pd_build_dir = PathBuf::from("vendor/i2pd/build");
    
    if !i2pd_dir.exists() {
        panic!("i2pd submodule not found. Please run: git submodule update --init --recursive");
    }

    // Configure CMake build for i2pd
    // The CMakeLists.txt is in the build/ subdirectory, which sets CMAKE_SOURCE_DIR to parent
    let mut cmake_config = cmake::Config::new(&i2pd_build_dir);
    
    // Set CMake options
    cmake_config
        .define("WITH_LIBRARY", "ON")
        .define("WITH_BINARY", "OFF")  // We only need the library
        .define("WITH_STATIC", "ON")   // Build static libraries
        .define("WITH_UPNP", "OFF")    // Disable UPnP for simplicity
        .cxxflag("-std=c++17")        // Ensure C++17 support
        // Use GCC explicitly to avoid Clang/libstdc++ linking issues
        .define("CMAKE_C_COMPILER", "gcc")
        .define("CMAKE_CXX_COMPILER", "g++")
        // Override flags to remove --target which causes linking issues
        .define("CMAKE_CXX_FLAGS", "-std=c++17 -ffunction-sections -fdata-sections -fPIC -m64")
        .define("CMAKE_C_FLAGS", "-ffunction-sections -fdata-sections -fPIC -m64")
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
        // Add the i2pd root directory so includes like "libi2pd_wrapper/capi.h" resolve correctly
        .include(&i2pd_dir)
        .include(&i2pd_dir.join("libi2pd"))
        .include(&i2pd_dir.join("libi2pd_client"))
        .include(&i2pd_dir.join("libi2pd_wrapper"))
        .include(&i2pd_dir.join("i18n"))  // For I18N_langs.h
        .flag("-std=c++17")
        // Use g++ explicitly to avoid cross-compilation target flags
        .compiler("g++")
        .compile("i2pd_wrapper");
    
    println!("cargo:rustc-link-lib=static=i2pd_wrapper");
    
    // Generate Rust bindings using bindgen
    let i2pd_wrapper_header = PathBuf::from("vendor/i2pd_wrapper.h");
    let i2pd_include = i2pd_dir.join("libi2pd");
    let i2pd_client_include = i2pd_dir.join("libi2pd_client");
    let i2pd_wrapper_include = i2pd_dir.join("libi2pd_wrapper");
    
    let bindings = bindgen::Builder::default()
        .header(i2pd_wrapper_header.to_str().unwrap())
        .clang_arg(format!("-I{}", i2pd_include.display()))
        .clang_arg(format!("-I{}", i2pd_client_include.display()))
        .clang_arg(format!("-I{}", i2pd_wrapper_include.display()))
        .clang_arg("-include")
        .clang_arg("stdint.h")  // Include stdint.h for uint16_t etc.
        .allowlist_function("i2pd_.*")
        .allowlist_type("i2pd_.*")
        .generate()
        .expect("Unable to generate bindings");
    
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("i2pd_bindings.rs"))
        .expect("Couldn't write bindings!");
}
