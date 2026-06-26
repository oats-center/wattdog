use std::{env, fs, path::PathBuf, process::Command};

const VENDORED_SDK_DIR: &str = "vendor/libpowermon_bin";

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=ffi/thornwave_scan.h");
    println!("cargo:rerun-if-changed=ffi/thornwave_scan.cc");
    println!("cargo:rerun-if-env-changed=THORNWAVE_SDK_DIR");
    println!("cargo:rerun-if-env-changed=THORNWAVE_LIB_FILE");

    println!("cargo:rerun-if-changed={VENDORED_SDK_DIR}/inc/powermon.h");
    println!("cargo:rerun-if-changed={VENDORED_SDK_DIR}/inc/powermon_scanner.h");

    let sdk_dir = env::var("THORNWAVE_SDK_DIR")
        .map_or_else(|_| PathBuf::from(VENDORED_SDK_DIR), PathBuf::from);
    let sdk_dir = sdk_dir.canonicalize().unwrap_or_else(|error| {
        panic!(
            "failed to resolve Thornwave SDK directory {}: {error}",
            sdk_dir.display()
        )
    });
    let include_dir = sdk_dir.join("inc");

    let required_headers = [
        include_dir.join("powermon.h"),
        include_dir.join("powermon_scanner.h"),
    ];
    for header in required_headers {
        assert!(
            header.exists(),
            "required Thornwave SDK header is missing: {}",
            header.display()
        );
    }

    let lib_file = env::var("THORNWAVE_LIB_FILE").unwrap_or_else(|_| default_library_file());
    let lib_path = sdk_dir.join(&lib_file);
    assert!(
        lib_path.exists(),
        "required Thornwave SDK static library is missing: {}. Set THORNWAVE_LIB_FILE if this SDK uses a different filename",
        lib_path.display()
    );

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .include(&include_dir)
        .file("ffi/thornwave_scan.cc")
        .warnings(true)
        .compile("thornwave_scan_facade");

    let mut bindings_builder = bindgen::Builder::default()
        .header("ffi/thornwave_scan.h")
        .allowlist_type("TwScanner")
        .allowlist_type("TwAdvertisement")
        .allowlist_function("tw_.*")
        .derive_debug(true);

    if let Some(include_path) = compiler_builtin_include_dir() {
        bindings_builder = bindings_builder
            .clang_arg("-isystem")
            .clang_arg(include_path);
    }

    let host = env::var("HOST").unwrap_or_default();
    let target = env::var("TARGET").unwrap_or_default();
    if host != target {
        bindings_builder = bindings_builder.clang_arg(format!("--target={target}"));
        if let Some(sysroot) = target_sysroot(&target) {
            bindings_builder =
                bindings_builder.clang_arg(format!("--sysroot={}", sysroot.display()));
        }
    } else if PathBuf::from("/usr/include").exists() {
        bindings_builder = bindings_builder
            .clang_arg("-isystem")
            .clang_arg("/usr/include");
    }

    let bindings = bindings_builder
        .generate()
        .expect("failed to generate Thornwave scanner bindings");

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR must be set by Cargo"));
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("failed to write Thornwave scanner bindings");

    let cargo_named_lib_file = format!("lib{lib_file}");
    let cargo_named_lib_path = out_dir.join(&cargo_named_lib_file);
    fs::copy(&lib_path, &cargo_named_lib_path).unwrap_or_else(|error| {
        panic!(
            "failed to copy Thornwave static library {} to {}: {error}",
            lib_path.display(),
            cargo_named_lib_path.display()
        )
    });

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!(
        "cargo:rustc-link-lib=static={}",
        static_library_name(&lib_file)
    );
    println!("cargo:rustc-link-lib=dylib=stdc++");
    link_system_library(
        "bluetooth",
        host_only_fallbacks(&["/usr/lib64/libbluetooth.so.3.19.16"]),
    );
    link_system_library(
        "dbus-1",
        host_only_fallbacks(&["/usr/lib64/libdbus-1.so.3.38.3"]),
    );
}

fn default_library_file() -> String {
    match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("aarch64") => "powermon_lib_rpi64_pic.a".to_string(),
        _ => "powermon_lib_pic.a".to_string(),
    }
}

fn compiler_builtin_include_dir() -> Option<String> {
    let compiler = env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let output = Command::new(compiler)
        .arg("-print-file-name=include")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if path.is_empty() || !PathBuf::from(&path).exists() {
        None
    } else {
        Some(path)
    }
}

fn target_sysroot(target: &str) -> Option<PathBuf> {
    let path = match target {
        "aarch64-unknown-linux-gnu" => PathBuf::from("/usr/aarch64-linux-gnu/sys-root"),
        _ => return None,
    };

    path.exists().then_some(path)
}

fn static_library_name(lib_file: &str) -> String {
    let without_prefix = lib_file.strip_prefix("lib").unwrap_or(lib_file);
    without_prefix
        .strip_suffix(".a")
        .unwrap_or(without_prefix)
        .to_string()
}

fn link_system_library(name: &str, fallback_paths: &[&str]) {
    for path in fallback_paths {
        if PathBuf::from(path).exists() {
            println!("cargo:rustc-link-arg={path}");
            return;
        }
    }

    println!("cargo:rustc-link-lib=dylib={name}");
}

fn host_only_fallbacks(paths: &'static [&'static str]) -> &'static [&'static str] {
    let host = env::var("HOST").unwrap_or_default();
    let target = env::var("TARGET").unwrap_or_default();
    if host == target { paths } else { &[] }
}
