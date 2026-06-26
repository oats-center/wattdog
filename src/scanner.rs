//! Safe Rust wrapper around the Thornwave scanner FFI façade.
//!
//! This module owns the raw `TwScanner` handle, registers the advertisement
//! callback, and converts callback payloads into Rust-owned [`Sample`] values.
//! The callback path performs no blocking work: it only copies data and attempts
//! a bounded channel send.

use std::{ffi::CStr, ptr::NonNull, sync::Arc};

use anyhow::{Context, Result, anyhow};
use tokio::sync::mpsc;
use tracing::{debug, info};

use crate::{ffi, metrics::Metrics, sample::Sample};

/// Owned Thornwave passive scanner handle.
///
/// Dropping the handle stops scanning and destroys the underlying C++ scanner
/// through the C façade. The callback state is kept alive for the full lifetime
/// of the raw scanner handle.
#[derive(Debug)]
pub struct Scanner {
    raw: NonNull<ffi::TwScanner>,
    _callback_state: Box<CallbackState>,
}

#[derive(Debug)]
struct CallbackState {
    tx: mpsc::Sender<Sample>,
    metrics: Arc<Metrics>,
}

// SAFETY: `Scanner` owns an opaque SDK handle that is only accessed through the
// C façade functions. The callback state is heap allocated and remains alive
// until `tw_scanner_destroy` returns. All cross-thread communication from the
// callback uses Tokio's thread-safe `mpsc::Sender` and metric handles.
unsafe impl Send for Scanner {}

impl Scanner {
    /// Returns the Thornwave SDK version as a raw BCD value.
    ///
    /// The value is read through the C façade from `Powermon::getVersion()`.
    /// A zero return indicates that the SDK threw an exception while reporting
    /// its version.
    #[must_use]
    pub fn library_version_bcd() -> u16 {
        unsafe { ffi::tw_library_version_bcd() }
    }

    /// Creates a scanner and registers the Rust advertisement callback.
    ///
    /// Observations are sent to `tx`. Queue overflow increments the scanner
    /// dropped-observations metric through `metrics` rather than blocking the
    /// Thornwave callback thread.
    ///
    /// # Errors
    ///
    /// Returns an error if the SDK scanner cannot be created or callback registration fails.
    pub fn new(tx: mpsc::Sender<Sample>, metrics: Arc<Metrics>) -> Result<Self> {
        info!("creating Thornwave scanner through SDK");
        let raw = unsafe { ffi::tw_scanner_create() };
        let raw = NonNull::new(raw).ok_or_else(last_error)?;
        info!("created Thornwave scanner handle");

        let mut callback_state = Box::new(CallbackState { tx, metrics });
        let user_data = (&raw mut *callback_state).cast::<std::ffi::c_void>();

        info!("registering Thornwave scanner callback");
        let set_callback_result = unsafe {
            ffi::tw_scanner_set_callback(raw.as_ptr(), Some(on_advertisement), user_data)
        };

        if set_callback_result != 0 {
            unsafe { ffi::tw_scanner_destroy(raw.as_ptr()) };
            return Err(last_error()).context("failed to set Thornwave scanner callback");
        }
        info!("registered Thornwave scanner callback");

        Ok(Self {
            raw,
            _callback_state: callback_state,
        })
    }

    /// Starts passive BLE advertisement scanning.
    ///
    /// The call delegates to the Thornwave SDK and returns the SDK's last error
    /// message if startup fails.
    ///
    /// # Errors
    ///
    /// Returns an error if the SDK fails to start BLE scanning.
    pub fn start_ble(&self) -> Result<()> {
        info!("calling Thornwave SDK startBleScan");
        let result = unsafe { ffi::tw_scanner_start_ble(self.raw.as_ptr()) };
        if result == 0 {
            info!("Thornwave SDK startBleScan returned successfully");
            Ok(())
        } else {
            Err(last_error())
        }
    }

    /// Stops BLE scanning.
    ///
    /// Stop errors are intentionally ignored by the C façade because this path
    /// is used during shutdown and `Drop`.
    pub fn stop(&self) {
        info!("stopping Thornwave BLE scanner transport");
        unsafe {
            ffi::tw_scanner_stop_ble(self.raw.as_ptr());
        }
        info!("stopped Thornwave BLE scanner transport");
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        unsafe { ffi::tw_scanner_destroy(self.raw.as_ptr()) };
    }
}

extern "C" fn on_advertisement(
    user_data: *mut std::ffi::c_void,
    advertisement: *const ffi::TwAdvertisement,
) {
    if user_data.is_null() || advertisement.is_null() {
        return;
    }

    let callback_state = unsafe { &*(user_data.cast::<CallbackState>()) };
    let sample = Sample::from_tw_advertisement(unsafe { &*advertisement });
    debug!(serial = sample.serial, "received Thornwave advertisement");

    if callback_state.tx.try_send(sample).is_err() {
        callback_state.metrics.observe_scanner_dropped();
    }
}

fn last_error() -> anyhow::Error {
    let ptr = unsafe { ffi::tw_last_error() };
    if ptr.is_null() {
        return anyhow!("unknown Thornwave SDK error");
    }

    let message = unsafe { CStr::from_ptr(ptr) }.to_string_lossy();
    if message.is_empty() {
        anyhow!("unknown Thornwave SDK error")
    } else {
        anyhow!(message.to_string())
    }
}
