fn main() {
    if let Ok(ver) = std::env::var("VIGIL_VERSION") {
        println!("cargo:rustc-env=VIGIL_VERSION={ver}");
    }
}
