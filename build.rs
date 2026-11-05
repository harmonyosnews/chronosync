use std::env;

fn main() {
    for key in ["CHRONOSYNC_BUILD_VARIANT", "CHRONOSYNC_BUILD_TIME"] {
        println!("cargo:rerun-if-env-changed={key}");
    }
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    let variant = env::var("CHRONOSYNC_BUILD_VARIANT").unwrap_or_else(|_| "host".to_string());
    let timestamp = env::var("CHRONOSYNC_BUILD_TIME").unwrap_or_else(|_| "dev".to_string());
    println!("cargo:rustc-env=CHRONOSYNC_BUILD_TARGET={target}");
    println!("cargo:rustc-env=CHRONOSYNC_BUILD_VARIANT={variant}");
    println!("cargo:rustc-env=CHRONOSYNC_BUILD_TIME={timestamp}");
}
