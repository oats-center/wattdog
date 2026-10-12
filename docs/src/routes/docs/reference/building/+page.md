---
title: "Build from source"
description: "Build wattdog against the Thornwave SDK, cross-compile for Raspberry Pi, and build the container."
---

## Requirements

- Rust (edition 2024) and Cargo.
- A C++ compiler, `libclang` (for bindgen), and the BlueZ and D-Bus development headers.
  - Fedora: `sudo dnf install gcc-c++ clang-libs bluez-libs-devel dbus-devel`
  - Debian/Ubuntu: `sudo apt install g++ clang libbluetooth-dev libdbus-1-dev pkg-config`
- The [Thornwave PowerMon SDK](https://git.thornwave.com/git/thornwave/libpowermon_bin), which
  isn't in this repository. Clone it into the ignored `vendor/` path:

```bash
mkdir -p vendor
git clone https://git.thornwave.com/git/thornwave/libpowermon_bin.git vendor/libpowermon_bin
cargo build --release
```

The binary is `target/release/wattdog`.

## Which SDK library is linked

`build.rs` links one static library from the SDK and the system's `stdc++`, `bluetooth`, and
`dbus-1`:

| Target | Default `THORNWAVE_LIB_FILE` |
|---|---|
| x86_64 | `powermon_lib_pic.a` |
| aarch64 | `powermon_lib_rpi64_pic.a` |

To use a different checkout or file:

```bash
export THORNWAVE_SDK_DIR=/opt/libpowermon_bin
export THORNWAVE_LIB_FILE=powermon_lib_arm64_pic.a
cargo build --release
```

:::caution[SDK update, June 2026]
The current SDK ships `powermon_lib_arm64_pic.a` instead of `powermon_lib_rpi64_pic.a`, so
aarch64 builds against a fresh clone need `THORNWAVE_LIB_FILE=powermon_lib_arm64_pic.a`. Its
x86_64 library is also built with a newer GCC: linking fails on Ubuntu 24.04's libstdc++ (missing
`std::format` symbols) but works with GCC 16 (Fedora 44).
:::

## Cross-compile for Raspberry Pi (64-bit)

The repository includes `Cross.toml` and a Fedora-based `Dockerfile.aarch64-unknown-linux-gnu` for
[`cross`](https://github.com/cross-rs/cross):

```bash
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target aarch64-unknown-linux-gnu
```

The binary is `target/aarch64-unknown-linux-gnu/release/wattdog`.

## Build the container

Build the binary first, then the runtime image from it:

```bash
cross build --release --target aarch64-unknown-linux-gnu
podman build --arch arm64 -f packaging/Containerfile -t localhost/wattdog:latest .
```

For x86_64, build with Cargo and pass `--build-arg TARGET_TRIPLE=x86_64-unknown-linux-gnu`. Copy
`target/release/wattdog` to `target/x86_64-unknown-linux-gnu/release/` first, or build with
`--target x86_64-unknown-linux-gnu`.

## Library-only build

The crate also exposes the Thornwave FFI as a small library for other projects:

```bash
cargo check --lib --no-default-features --features ffi
```
