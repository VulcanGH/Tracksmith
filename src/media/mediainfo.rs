//! MediaInfo's DLL is loaded only in a disposable worker process, never in the server.
use anyhow::Result;
use std::path::Path;

#[cfg(windows)]
pub fn worker(dll: &Path, file: &Path) -> Result<()> {
    use anyhow::{Context, ensure};
    use std::{ffi::c_void, os::windows::ffi::OsStrExt};
    fn wide(value: &std::ffi::OsStr) -> Vec<u16> {
        value.encode_wide().chain(Some(0)).collect()
    }
    // SAFETY: documented MediaInfo Windows SDK ABI; symbols remain valid while lib is held.
    unsafe {
        let lib = libloading::Library::new(dll).context("Loading MediaInfo library")?;
        let new: libloading::Symbol<unsafe extern "system" fn() -> *mut c_void> =
            lib.get(b"MediaInfo_New")?;
        let delete: libloading::Symbol<unsafe extern "system" fn(*mut c_void)> =
            lib.get(b"MediaInfo_Delete")?;
        let option: libloading::Symbol<
            unsafe extern "system" fn(*mut c_void, *const u16, *const u16) -> *const u16,
        > = lib.get(b"MediaInfo_Option")?;
        let open: libloading::Symbol<unsafe extern "system" fn(*mut c_void, *const u16) -> usize> =
            lib.get(b"MediaInfo_Open")?;
        let inform: libloading::Symbol<
            unsafe extern "system" fn(*mut c_void, usize) -> *const u16,
        > = lib.get(b"MediaInfo_Inform")?;
        let handle = new();
        ensure!(!handle.is_null(), "MediaInfo allocation failed");
        // Cleanup runs on every ordinary return, including malformed JSON and open failures.
        struct Guard<'a> {
            handle: *mut c_void,
            delete: &'a (dyn Fn(*mut c_void) + 'a),
        }
        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                (self.delete)(self.handle);
            }
        }
        let cleanup = |handle| delete(handle);
        let _guard = Guard {
            handle,
            delete: &cleanup,
        };
        option(
            handle,
            wide("Output".as_ref()).as_ptr(),
            wide("JSON".as_ref()).as_ptr(),
        );
        option(
            handle,
            wide("Complete".as_ref()).as_ptr(),
            wide("1".as_ref()).as_ptr(),
        );
        ensure!(
            open(handle, wide(file.as_os_str()).as_ptr()) != 0,
            "MediaInfo could not open the file"
        );
        let ptr = inform(handle, 0);
        ensure!(!ptr.is_null(), "MediaInfo returned no inspection");
        let mut length = 0;
        while length < 8 * 1024 * 1024 && *ptr.add(length) != 0 {
            length += 1;
        }
        ensure!(length < 8 * 1024 * 1024, "MediaInfo output exceeded limit");
        let text = String::from_utf16(std::slice::from_raw_parts(ptr, length))?;
        let json: serde_json::Value = serde_json::from_str(&text)?;
        println!("{}", serde_json::to_string(&json)?);
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn worker(_: &Path, _: &Path) -> Result<()> {
    anyhow::bail!("MediaInfo DLL worker is Windows-only; configure a MediaInfo CLI")
}
