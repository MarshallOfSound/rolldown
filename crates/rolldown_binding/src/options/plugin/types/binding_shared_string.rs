use std::sync::Arc;

use arcstr::ArcStr;
use napi::{
  Result,
  bindgen_prelude::{FromNapiValue, ToNapiValue, TypeName, ValidateNapiValue, ValueType},
  sys,
};

#[derive(Debug, Clone)]
pub struct BindingSharedString {
  inner: SharedStringInner,
}

#[derive(Debug, Clone)]
enum SharedStringInner {
  ArcStr(ArcStr),
  #[expect(clippy::rc_buffer, reason = "Arc<String> lets renderChunk recover owned code")]
  String(Arc<String>),
}

impl BindingSharedString {
  fn as_str(&self) -> &str {
    match &self.inner {
      SharedStringInner::ArcStr(value) => value.as_str(),
      SharedStringInner::String(value) => value.as_str(),
    }
  }

}

impl From<ArcStr> for BindingSharedString {
  fn from(value: ArcStr) -> Self {
    Self { inner: SharedStringInner::ArcStr(value) }
  }
}

impl From<Arc<String>> for BindingSharedString {
  fn from(value: Arc<String>) -> Self {
    Self { inner: SharedStringInner::String(value) }
  }
}

impl TypeName for BindingSharedString {
  fn type_name() -> &'static str {
    "String"
  }

  fn value_type() -> ValueType {
    ValueType::String
  }
}

impl ValidateNapiValue for BindingSharedString {}

impl FromNapiValue for BindingSharedString {
  unsafe fn from_napi_value(env: sys::napi_env, napi_val: sys::napi_value) -> Result<Self> {
    Ok(Self::from(Arc::new(unsafe { String::from_napi_value(env, napi_val)? })))
  }
}

/// Strings shorter than this are copied into the JS heap as before; longer ones (module
/// source, chunk code) are handed to V8 as external one-byte strings that borrow the
/// Rust allocation, so a hook call no longer pays a UTF-8 decode + copy of the whole module
/// per plugin per module on the JS main thread.
const EXTERNAL_STRING_MIN_LEN: usize = 1024;

impl ToNapiValue for &BindingSharedString {
  unsafe fn to_napi_value(env: sys::napi_env, value: Self) -> Result<sys::napi_value> {
    let s = value.as_str();
    if s.len() >= EXTERNAL_STRING_MIN_LEN {
      let created = if s.is_ascii() {
        // Zero copy: V8 reads the Rust bytes directly.
        unsafe { external_latin1::create(env, value) }
      } else {
        // One SIMD transcode into a buffer V8 then adopts, instead of V8's own two-pass
        // UTF-8 decoder plus a copy. Any source with a single non-ASCII character (an em
        // dash in a comment is enough) lands here.
        unsafe { external_utf16::create(env, s) }
      };
      if let Some(result) = created {
        return result;
      }
    }
    unsafe { ToNapiValue::to_napi_value(env, s) }
  }
}

/// `node_api_create_external_string_utf16` (Node-API 10), same runtime lookup as below.
mod external_utf16 {
  use std::ffi::c_void;
  use std::sync::OnceLock;

  use napi::{Result, sys};

  type BasicFinalize =
    Option<unsafe extern "C" fn(env: *mut c_void, data: *mut c_void, hint: *mut c_void)>;
  type CreateExternalUtf16 = unsafe extern "C" fn(
    env: sys::napi_env,
    str_: *const u16,
    length: isize,
    finalize: BasicFinalize,
    finalize_hint: *mut c_void,
    result: *mut sys::napi_value,
    copied: *mut bool,
  ) -> sys::napi_status;

  fn lookup() -> Option<CreateExternalUtf16> {
    static CELL: OnceLock<Option<CreateExternalUtf16>> = OnceLock::new();
    *CELL.get_or_init(|| {
      if std::env::var_os("ROLLDOWN_NO_EXTERNAL_STRINGS").is_some() {
        return None;
      }
      #[cfg(unix)]
      {
        // SAFETY: dlsym on the running process; signature per js_native_api.h.
        let sym = unsafe {
          libc::dlsym(libc::RTLD_DEFAULT, c"node_api_create_external_string_utf16".as_ptr())
        };
        if sym.is_null() {
          None
        } else {
          // SAFETY: see above.
          Some(unsafe { std::mem::transmute::<*mut c_void, CreateExternalUtf16>(sym) })
        }
      }
      #[cfg(not(unix))]
      {
        None
      }
    })
  }

  unsafe extern "C" fn finalize(env: *mut c_void, _data: *mut c_void, hint: *mut c_void) {
    // SAFETY: `hint` is the `Box<Vec<u16>>` leaked in `create`; called exactly once (on GC of
    // the external string, or right away when the engine copied instead).
    let owner = unsafe { Box::from_raw(hint.cast::<Vec<u16>>()) };
    super::report_external_memory(env.cast(), -byte_len(owner.len() * 2));
  }

  pub(super) unsafe fn create(env: sys::napi_env, s: &str) -> Option<Result<sys::napi_value>> {
    let create = lookup()?;
    let mut buf: Vec<u16> = vec![0; s.len()];
    let written = encoding_rs::mem::convert_str_to_utf16(s, &mut buf);
    buf.truncate(written);
    buf.shrink_to_fit();
    let owner = Box::new(buf);
    let ptr = owner.as_ptr();
    let units = owner.len();
    let len = isize::try_from(units).ok()?;
    let hint = Box::into_raw(owner).cast::<c_void>();
    let mut result = std::ptr::null_mut();
    let mut copied = false;
    // SAFETY: `ptr`/`len` describe `owner`'s buffer, alive until `finalize` runs.
    let status = unsafe { create(env, ptr, len, Some(finalize), hint, &raw mut result, &raw mut copied) };
    if status != sys::Status::napi_ok {
      // SAFETY: on failure Node has not taken ownership of `hint`.
      drop(unsafe { Box::from_raw(hint.cast::<Vec<u16>>()) });
      return None;
    }
    // Tell V8 how much off-heap memory the string pins so GC scheduling accounts for it
    // (external strings live in old space and only a full GC reclaims them). When the
    // engine copied instead, `finalize` has already run and subtracted the same amount.
    super::report_external_memory(env, byte_len(units * 2));
    Some(Ok(result))
  }

  fn byte_len(bytes: usize) -> i64 {
    i64::try_from(bytes).unwrap_or(i64::MAX)
  }
}

/// `node_api_create_external_string_latin1` (Node-API 10). Looked up at runtime so the
/// binding still loads on hosts that predate it; ASCII is a subset of Latin-1, so an
/// ASCII-only Rust `str` can be exposed byte-for-byte.
mod external_latin1 {
  use std::ffi::c_void;
  use std::sync::OnceLock;

  use napi::{Result, sys};

  use super::BindingSharedString;

  type BasicFinalize =
    Option<unsafe extern "C" fn(env: *mut c_void, data: *mut c_void, hint: *mut c_void)>;
  type CreateExternalLatin1 = unsafe extern "C" fn(
    env: sys::napi_env,
    str_: *const std::os::raw::c_char,
    length: isize,
    finalize: BasicFinalize,
    finalize_hint: *mut c_void,
    result: *mut sys::napi_value,
    copied: *mut bool,
  ) -> sys::napi_status;

  fn lookup() -> Option<CreateExternalLatin1> {
    static CELL: OnceLock<Option<CreateExternalLatin1>> = OnceLock::new();
    *CELL.get_or_init(|| {
      if std::env::var_os("ROLLDOWN_NO_EXTERNAL_STRINGS").is_some() {
        return None;
      }
      #[cfg(unix)]
      {
        // SAFETY: dlsym on the running process; the symbol, if present, has this signature
        // (js_native_api.h, NAPI_VERSION >= 10).
        let sym = unsafe {
          libc::dlsym(libc::RTLD_DEFAULT, c"node_api_create_external_string_latin1".as_ptr())
        };
        if sym.is_null() {
          None
        } else {
          // SAFETY: see above.
          Some(unsafe { std::mem::transmute::<*mut c_void, CreateExternalLatin1>(sym) })
        }
      }
      #[cfg(not(unix))]
      {
        None
      }
    })
  }

  unsafe extern "C" fn finalize(env: *mut c_void, _data: *mut c_void, hint: *mut c_void) {
    // SAFETY: `hint` is the `Box<BindingSharedString>` leaked in `create`; called exactly once
    // (on GC of the external string, or immediately when the engine copied instead).
    let owner = unsafe { Box::from_raw(hint.cast::<BindingSharedString>()) };
    super::report_external_memory(env.cast(), -byte_len(owner.as_str().len()));
  }

  fn byte_len(bytes: usize) -> i64 {
    i64::try_from(bytes).unwrap_or(i64::MAX)
  }

  /// Returns `None` when the API is unavailable (caller falls back to a copying create).
  pub(super) unsafe fn create(
    env: sys::napi_env,
    value: &BindingSharedString,
  ) -> Option<Result<sys::napi_value>> {
    let create = lookup()?;
    // Keep the backing allocation alive for as long as V8 references it.
    let owner = Box::new(value.clone());
    let ptr = owner.as_str().as_ptr();
    let bytes = owner.as_str().len();
    let len = isize::try_from(bytes).ok()?;
    let hint = Box::into_raw(owner).cast::<c_void>();
    let mut result = std::ptr::null_mut();
    let mut copied = false;
    // SAFETY: `ptr`/`len` describe `owner`'s bytes, which live until `finalize` runs.
    let status =
      unsafe { create(env, ptr.cast(), len, Some(finalize), hint, &raw mut result, &raw mut copied) };
    if status != sys::Status::napi_ok {
      // Not created: reclaim the owner ourselves and let the caller fall back.
      // SAFETY: on failure Node has not taken ownership of `hint`.
      drop(unsafe { Box::from_raw(hint.cast::<BindingSharedString>()) });
      return None;
    }
    // See external_utf16::create. The shared string's bytes may also be referenced from
    // Rust (an `ArcStr` module source), so this can over-report; V8 only uses the figure
    // as GC pressure, which is the intent.
    super::report_external_memory(env, byte_len(bytes));
    Some(Ok(result))
  }
}

/// Report a change in the off-heap memory retained by JS objects (`napi_adjust_external_memory`).
/// Best effort: a failure only makes GC scheduling less informed.
fn report_external_memory(env: sys::napi_env, change_in_bytes: i64) {
  if change_in_bytes == 0 || env.is_null() {
    return;
  }
  let mut adjusted = 0i64;
  // SAFETY: plain FFI call with a valid env and out-pointer.
  let _ = unsafe { sys::napi_adjust_external_memory(env, change_in_bytes, &raw mut adjusted) };
}

impl ToNapiValue for &mut BindingSharedString {
  unsafe fn to_napi_value(env: sys::napi_env, value: Self) -> Result<sys::napi_value> {
    unsafe { ToNapiValue::to_napi_value(env, &*value) }
  }
}

impl ToNapiValue for BindingSharedString {
  unsafe fn to_napi_value(env: sys::napi_env, value: Self) -> Result<sys::napi_value> {
    unsafe { ToNapiValue::to_napi_value(env, &value) }
  }
}
