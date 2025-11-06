use std::env;
use std::path::PathBuf;

fn main() {
    pyo3_build_config::use_pyo3_cfgs();

    // Get the i2pd vendor directory
    let i2pd_dir = PathBuf::from("vendor/i2pd");
    
    // Check if i2pd submodule exists and has content
    let i2pd_available = i2pd_dir.exists() && 
        (i2pd_dir.join("CMakeLists.txt").exists() || 
         i2pd_dir.join("libi2pd").exists());
    
    if i2pd_available {
        // Configure CMake build for i2pd - use source directory, not build directory
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
        println!("cargo:warning=i2pd submodule not found. Skipping i2pd build. Router functionality will be limited.");
        println!("cargo:warning=To enable full functionality, run: git submodule update --init --recursive");
    }
    
    // Link system libraries
    if cfg!(target_os = "windows") {
        println!("cargo:rustc-link-lib=wsock32");
        println!("cargo:rustc-link-lib=ws2_32");
        println!("cargo:rustc-link-lib=iphlpapi");
    }
    
    // Link against Boost and OpenSSL (these should be found by CMake or system)
    if i2pd_available {
        println!("cargo:rustc-link-lib=boost_filesystem");
        println!("cargo:rustc-link-lib=boost_program_options");
    }
    println!("cargo:rustc-link-lib=ssl");
    println!("cargo:rustc-link-lib=crypto");
    println!("cargo:rustc-link-lib=z");
    
    // Compile the C++ wrapper only if i2pd is available
    if i2pd_available {
        let mut cpp_build = cc::Build::new();
        cpp_build
            .cpp(true)
            .file("vendor/i2pd_wrapper.cpp")
            .include(&i2pd_dir.join("libi2pd"))
            .include(&i2pd_dir.join("libi2pd_client"));
        
        // Check if libi2pd_wrapper exists
        let wrapper_include = i2pd_dir.join("libi2pd_wrapper");
        if wrapper_include.exists() {
            cpp_build.include(&wrapper_include);
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
        
        let mut bindgen_builder = bindgen::Builder::default()
            .header(i2pd_wrapper_header.to_str().unwrap())
            .clang_arg(format!("-I{}", i2pd_include.display()))
            .clang_arg(format!("-I{}", i2pd_client_include.display()))
            .allowlist_function("i2pd_.*")
            .allowlist_type("i2pd_.*");
        
        if i2pd_wrapper_include.exists() {
            bindgen_builder = bindgen_builder.clang_arg(format!("-I{}", i2pd_wrapper_include.display()));
        }
        
        let bindings = bindgen_builder
            .generate()
            .expect("Unable to generate bindings");
        
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("i2pd_bindings.rs"))
            .expect("Couldn't write bindings!");
    } else {
        // Generate stub bindings when i2pd is not available
        // We'll create a minimal C stub library and link against it
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        
        // Create stub C source file
        let stub_c_source = r#"// Stub implementation - i2pd not available
#include <stdint.h>

int i2pd_router_init(const char* config_dir) {
    (void)config_dir;
    return -1;
}

int i2pd_router_start(void) {
    return -1;
}

int i2pd_router_stop(void) {
    return -1;
}

void i2pd_router_cleanup(void) {
}

int i2pd_http_proxy_start(const char* address, uint16_t port) {
    (void)address;
    (void)port;
    return -1;
}

int i2pd_https_proxy_start(const char* address, uint16_t port) {
    (void)address;
    (void)port;
    return -1;
}

void i2pd_http_proxy_stop(void) {
}

void i2pd_https_proxy_stop(void) {
}

int i2pd_router_is_running(void) {
    return 0;
}
"#;
        
        let stub_c_path = out_path.join("i2pd_stub.c");
        std::fs::write(&stub_c_path, stub_c_source)
            .expect("Couldn't write stub C source!");
        
        // Compile stub library
        cc::Build::new()
            .file(&stub_c_path)
            .compile("i2pd_stub");
        
        println!("cargo:rustc-link-lib=static=i2pd_stub");
        
        // Generate bindings from header (bindgen can work without i2pd source)
        let i2pd_wrapper_header = PathBuf::from("vendor/i2pd_wrapper.h");
        let bindings = bindgen::Builder::default()
            .header(i2pd_wrapper_header.to_str().unwrap())
            .allowlist_function("i2pd_.*")
            .allowlist_type("i2pd_.*")
            .generate()
            .expect("Unable to generate bindings");
        
        bindings
            .write_to_file(out_path.join("i2pd_bindings.rs"))
            .expect("Couldn't write bindings!");
    }
}
