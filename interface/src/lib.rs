use std::ffi::{CStr, c_char};

/// Преобразует указатель на C-строку в Rust строку.
///
/// Возвращает `None`, если указатель равен `NULL`.
///
/// # Safety
///
/// При преобразование происходит проверка на NULL.
pub unsafe fn ptr_to_string(cstr_ptr: *const c_char) -> Option<String> {
    if cstr_ptr.is_null() {
        return None;
    }

    let c_str = unsafe { CStr::from_ptr(cstr_ptr) };

    c_str.to_str().ok().map(|s| s.to_string())
}
