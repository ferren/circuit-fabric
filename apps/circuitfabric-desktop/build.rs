fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=resources/circuitfabric.rc");
        println!("cargo:rerun-if-changed=../../assets/branding/circuitfabric-framed.ico");
        embed_resource::compile("resources/circuitfabric.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to compile the CircuitFabric Windows icon resource");
    }
}
