fn main() {
    // GPUI element trees compile to very large stack frames in debug builds, and the
    // Windows default main-thread stack (1 MiB) overflows during the first render.
    // Zed applies the same linker workaround to its own binary.
    #[cfg(all(windows, target_env = "msvc"))]
    println!("cargo:rustc-link-arg-bins=/STACK:{}", 8 * 1024 * 1024);

    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=resources/circuitfabric.rc");
        println!("cargo:rerun-if-changed=../../assets/branding/circuitfabric-framed.ico");
        embed_resource::compile("resources/circuitfabric.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to compile the CircuitFabric Windows icon resource");
    }
}
