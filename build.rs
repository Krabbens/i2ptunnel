use std::env;
use std::path::PathBuf;
use std::fs;

fn main() {
    pyo3_build_config::use_pyo3_cfgs();

    // Get the i2pd vendor directory
    let i2pd_dir = PathBuf::from("vendor/i2pd");
    let i2pd_build_dir = PathBuf::from("vendor/i2pd/build");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    
    // Check if i2pd is available (directory exists and is not empty)
    let i2pd_available = i2pd_dir.exists() && 
                         i2pd_dir.read_dir().map(|mut d| d.next().is_some()).unwrap_or(false);
    
    if !i2pd_available {
        println!("cargo:warning=i2pd submodule not found. Building without native i2pd support.");
        println!("cargo:warning=To enable i2pd support, run: git submodule update --init --recursive");
        
        // Generate stub bindings
        let stub_bindings = r#"// Stub bindings for i2pd (i2pd not available)
#[allow(non_snake_case)]
pub extern "C" fn i2pd_router_init(_config_dir: *const ::std::os::raw::c_char) -> ::std::os::raw::c_int {
    -1
}

#[allow(non_snake_case)]
pub extern "C" fn i2pd_router_start() -> ::std::os::raw::c_int {
    -1
}

#[allow(non_snake_case)]
pub extern "C" fn i2pd_router_stop() -> ::std::os::raw::c_int {
    -1
}

#[allow(non_snake_case)]
pub extern "C" fn i2pd_router_cleanup() {
}

#[allow(non_snake_case)]
pub extern "C" fn i2pd_http_proxy_start(_address: *const ::std::os::raw::c_char, _port: u16) -> ::std::os::raw::c_int {
    -1
}

#[allow(non_snake_case)]
pub extern "C" fn i2pd_https_proxy_start(_address: *const ::std::os::raw::c_char, _port: u16) -> ::std::os::raw::c_int {
    -1
}

#[allow(non_snake_case)]
pub extern "C" fn i2pd_http_proxy_stop() {
}

#[allow(non_snake_case)]
pub extern "C" fn i2pd_https_proxy_stop() {
}

#[allow(non_snake_case)]
pub extern "C" fn i2pd_router_is_running() -> ::std::os::raw::c_int {
    0
}
"#;
        
        fs::write(out_path.join("i2pd_bindings.rs"), stub_bindings)
            .expect("Couldn't write stub bindings!");
        return;
    }

    // Configure CMake build for i2pd
    let mut cmake_config = cmake::Config::new(&i2pd_build_dir);
    
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
        .include(&i2pd_dir.join("libi2pd"))
        .include(&i2pd_dir.join("libi2pd_client"))
        .include(&i2pd_dir.join("libi2pd_wrapper"))
        .flag("-std=c++17")
        .compile("i2pd_wrapper");
    
    println!("cargo:rustc-link-lib=static=i2pd_wrapper");
    
    // Generate Rust bindings using bindgen
    let i2pd_wrapper_header = PathBuf::from("vendor/i2pd_wrapper.h");
    let i2pd_include = i2pd_dir.join("libi2pd");
    let i2pd_client_include = i2pd_dir.join("libi2pd_client");
    let i2pd_wrapper_include = i2pd_dir.join("libi2pd_wrapper");
    let i2pd_api_include = i2pd_dir.join("libi2pd/api.h");
    
    let bindings = bindgen::Builder::default()
        .header(i2pd_wrapper_header.to_str().unwrap())
        .clang_arg(format!("-I{}", i2pd_include.display()))
        .clang_arg(format!("-I{}", i2pd_client_include.display()))
        .clang_arg(format!("-I{}", i2pd_wrapper_include.display()))
        .allowlist_function("i2pd_.*")
        .allowlist_type("i2pd_.*")
        .generate()
        .expect("Unable to generate bindings");
    
    bindings
        .write_to_file(out_path.join("i2pd_bindings.rs"))
        .expect("Couldn't write bindings!");
}
