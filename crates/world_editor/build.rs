fn main() {
    // Delay-load vendor DLLs so the render crate can add the `lib/`
    // subdirectory to the DLL search path before the import is resolved.
    #[cfg(feature = "fsr")]
    {
        println!("cargo:rustc-link-arg=/DELAYLOAD:amd_fidelityfx_loader_dx12.dll");
        println!("cargo:rustc-link-arg=delayimp.lib");
    }
}
