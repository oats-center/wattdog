//! Bindgen-generated declarations for the C-compatible Thornwave scanner API.
//!
//! The handwritten C++ façade in `ffi/thornwave_scan.cc` hides Thornwave's C++
//! classes, strings, callbacks, and exceptions behind plain C structs and
//! function pointers. This module contains only the generated raw bindings; all
//! safe access is provided by [`crate::scanner`].

#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    reason = "bindgen output mirrors C API"
)]
#![allow(
    clippy::unreadable_literal,
    reason = "bindgen emits numeric constants without separators"
)]
#![allow(
    missing_docs,
    reason = "raw bindgen items are documented at the safe wrapper boundary"
)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
