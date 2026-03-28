fn main() {
    #[cfg(feature = "dlss")]
    dlss_bindings();
    #[cfg(feature = "fsr")]
    fsr_bindings();
}

#[cfg(feature = "dlss")]
fn dlss_bindings() {
    use std::{env, path::PathBuf};

    // Get SDK paths
    let dlss_sdk = env::var("DLSS_SDK")
        .expect("DLSS_SDK environment variable not set. Consult the dlss_wgpu readme.");
    let vulkan_sdk = env::var("VULKAN_SDK").expect("VULKAN_SDK environment variable not set");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Link to needed libraries
    #[cfg(not(target_os = "windows"))]
    {
        println!("cargo:rustc-link-search=native={dlss_sdk}/lib/Linux_x86_64");
        println!("cargo:rustc-link-lib=static=nvsdk_ngx");
        println!("cargo:rustc-link-lib=dylib=stdc++");
        println!("cargo:rustc-link-lib=dylib=dl");
    }
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-search=native={dlss_sdk}/lib/Windows_x86_64/x64");
        #[cfg(not(target_feature = "crt-static"))]
        println!("cargo:rustc-link-lib=static=nvsdk_ngx_d");
        #[cfg(target_feature = "crt-static")]
        println!("cargo:rustc-link-lib=static=nvsdk_ngx_s");
    }

    // Generate rust bindings
    #[cfg(not(target_os = "windows"))]
    let vulkan_sdk_include = "include";
    #[cfg(target_os = "windows")]
    let vulkan_sdk_include = "Include";
    bindgen::Builder::default()
        .header(format!("{}/src/dlss/wrapper.h", env!("CARGO_MANIFEST_DIR")))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .wrap_static_fns(true)
        .wrap_static_fns_path(out_dir.join("wrap_static_fns"))
        .clang_arg(format!("-I{dlss_sdk}/include"))
        .clang_arg(format!("-I{vulkan_sdk}/{vulkan_sdk_include}"))
        .allowlist_item(".*NGX.*")
        .blocklist_item("Vk.*")
        .blocklist_item("PFN_vk.*")
        .blocklist_item(".*Cuda.*")
        .blocklist_item(".*CUDA.*")
        .generate()
        .unwrap()
        .write_to_file(out_dir.join("bindings.rs"))
        .unwrap();

    // Generate and link a library for static inline functions
    cc::Build::new()
        .file(out_dir.join("wrap_static_fns.c"))
        .includes([
            format!("{dlss_sdk}/include"),
            format!("{vulkan_sdk}/{vulkan_sdk_include}"),
        ])
        .compile("wrap_static_fns");

    // Copy DLSS runtime DLLs next to the output binary so the SDK finds them at launch.
    copy_dlss_runtime_dlls(&dlss_sdk, &out_dir);
}

#[cfg(feature = "fsr")]
fn fsr_bindings() {
    use std::{env, path::PathBuf};

    let fsr_sdk = env::var("FSR_SDK")
        .expect("FSR_SDK environment variable not set. Run scripts/capy-fsr-setup.ps1 first.");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // The FidelityFX SDK ships a single import lib; the upscaler / frame-gen
    // backends are loaded at runtime through the loader DLL.
    let signedbin = format!("{fsr_sdk}/Kits/FidelityFX/signedbin");
    println!("cargo:rustc-link-search=native={signedbin}");
    println!("cargo:rustc-link-lib=static=amd_fidelityfx_loader_dx12");

    // Include paths for the unified API and feature-specific headers.
    let api_include = format!("{fsr_sdk}/Kits/FidelityFX/api/include");
    let upscaler_include = format!("{fsr_sdk}/Kits/FidelityFX/upscalers/include");
    let framegen_include = format!("{fsr_sdk}/Kits/FidelityFX/framegeneration/include");

    // Generate Rust bindings from the unified C API header.
    bindgen::Builder::default()
        .header(format!("{}/src/fsr/wrapper.h", env!("CARGO_MANIFEST_DIR")))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .clang_args([
            format!("-I{api_include}"),
            format!("-I{upscaler_include}"),
            format!("-I{framegen_include}"),
        ])
        .clang_arg("-fms-extensions")
        // Only expose FFX/Ffx symbols.
        // Note: (?i) inline flag does not work reliably in bindgen 0.72's
        // allowlist regex, so use character classes for case-insensitivity.
        .allowlist_type("[fF][fF][xX].*")
        .allowlist_function("[fF][fF][xX].*")
        .allowlist_var("[fF][fF][xX].*")
        .allowlist_var("FFX_.*")
        .generate()
        .unwrap()
        .write_to_file(out_dir.join("fsr_bindings.rs"))
        .unwrap();

    // Copy runtime DLLs next to the output binary.
    copy_fsr_runtime_dlls(&signedbin, &out_dir);
}

#[cfg(feature = "fsr")]
fn copy_fsr_runtime_dlls(signedbin: &str, out_dir: &std::path::Path) {
    let Some(profile_dir) = out_dir
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
    else {
        println!(
            "cargo:warning=Could not determine target profile directory; skipping FSR DLL copy."
        );
        return;
    };

    let lib_dir = profile_dir.join("lib");
    let _ = std::fs::create_dir_all(&lib_dir);

    let src_dir = std::path::PathBuf::from(signedbin);
    let dlls = [
        "amd_fidelityfx_loader_dx12.dll",
        "amd_fidelityfx_upscaler_dx12.dll",
        "amd_fidelityfx_framegeneration_dx12.dll",
    ];
    for dll in &dlls {
        let src = src_dir.join(dll);
        let dst = lib_dir.join(dll);
        if src.exists() {
            if let Err(e) = std::fs::copy(&src, &dst) {
                println!(
                    "cargo:warning=Failed to copy {dll} to {}: {e}",
                    dst.display()
                );
            }
        } else {
            println!("cargo:warning=FSR runtime DLL not found: {}", src.display());
        }
    }
}

#[cfg(feature = "dlss")]
fn copy_dlss_runtime_dlls(dlss_sdk: &str, out_dir: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        // OUT_DIR is typically target/<profile>/build/<crate>-<hash>/out.
        // Walk up to the profile directory where cargo places the final binary.
        let Some(profile_dir) = out_dir
            .parent() // build/<crate>-<hash>/
            .and_then(|p| p.parent()) // build/
            .and_then(|p| p.parent())
        // target/<profile>/
        else {
            println!(
                "cargo:warning=Could not determine target profile directory; skipping DLSS DLL copy."
            );
            return;
        };

        let lib_dir = profile_dir.join("lib");
        let _ = std::fs::create_dir_all(&lib_dir);

        let src_dir = std::path::PathBuf::from(format!("{dlss_sdk}/lib/Windows_x86_64/rel"));
        let dlls = ["nvngx_dlss.dll", "nvngx_dlssd.dll", "nvngx_dlssg.dll"];
        for dll in &dlls {
            let src = src_dir.join(dll);
            let dst = lib_dir.join(dll);
            if src.exists() {
                if let Err(e) = std::fs::copy(&src, &dst) {
                    println!(
                        "cargo:warning=Failed to copy {dll} to {}: {e}",
                        dst.display()
                    );
                }
            } else {
                println!(
                    "cargo:warning=DLSS runtime DLL not found: {}",
                    src.display()
                );
            }
        }
    }
}
