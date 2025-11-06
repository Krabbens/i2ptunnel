use std::env;
use std::path::PathBuf;

fn main() {
    pyo3_build_config::use_pyo3_cfgs();

    // Get the i2pd vendor directory
    let i2pd_dir = PathBuf::from("vendor/i2pd");
    
    if !i2pd_dir.exists() {
        eprintln!("ERROR: i2pd submodule not found at {}", i2pd_dir.display());
        eprintln!("Please run: git submodule update --init --recursive");
        eprintln!("Or if you don't have the i2pd submodule, you need to initialize it first.");
        panic!("i2pd submodule not found. Please run: git submodule update --init --recursive");
    }

    // Configure CMake build for i2pd (use source directory, not build directory)
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
        .include(&i2pd_dir.join("libi2pd"));
    
    // Add includes that exist
    let libi2pd_client = i2pd_dir.join("libi2pd_client");
    if libi2pd_client.exists() {
        cpp_build.include(&libi2pd_client);
    }
    
    let libi2pd_wrapper = i2pd_dir.join("libi2pd_wrapper");
    if libi2pd_wrapper.exists() {
        cpp_build.include(&libi2pd_wrapper);
    }
    
    cpp_build
        .flag("-std=c++17")
        .compile("i2pd_wrapper");
    
    println!("cargo:rustc-link-lib=static=i2pd_wrapper");
    
    // Generate Rust bindings using bindgen
    let i2pd_wrapper_header = PathBuf::from("vendor/i2pd_wrapper.h");
    let i2pd_include = i2pd_dir.join("libi2pd");
    
    let mut bindgen_builder = bindgen::Builder::default()
        .header(i2pd_wrapper_header.to_str().unwrap())
        .clang_arg(format!("-I{}", i2pd_include.display()));
    
    // Add includes that exist
    let libi2pd_client = i2pd_dir.join("libi2pd_client");
    if libi2pd_client.exists() {
        bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", libi2pd_client.display()));
    }
    
    let libi2pd_wrapper = i2pd_dir.join("libi2pd_wrapper");
    if libi2pd_wrapper.exists() {
        bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", libi2pd_wrapper.display()));
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
