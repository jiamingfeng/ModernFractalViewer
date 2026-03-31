//! Android-specific mesh export: copies a local file into the Downloads folder
//! via the Android MediaStore API using JNI.
//!
//! This module is only compiled for `target_os = "android"`.
//!
//! # Flow
//! 1. `export_to_downloads` is called from the background export thread with the
//!    path of a temp file in the app's private internal storage.
//! 2. A new MediaStore row is inserted into `MediaStore.Downloads` to obtain a
//!    writable `ParcelFileDescriptor` (no `WRITE_EXTERNAL_STORAGE` permission
//!    needed on API 29+).
//! 3. The temp file bytes are copied into the file descriptor.
//! 4. The descriptor's native fd is detached and closed via `std::fs::File`.
//! 5. The temp file at `src_path` is removed by the caller.

#![cfg(target_os = "android")]

use std::os::unix::io::FromRawFd;
use std::path::Path;

use jni::objects::{JObject, JValue};
use jni::JNIEnv;

/// Copy `src_path` into the Android Downloads folder as `display_name`.
///
/// Returns the public path string (e.g. `"Downloads/mandelbulb_20260331_143022.glb"`)
/// on success, or an error message suitable for showing in the UI.
///
/// # Errors
/// Returns `Err` if the JVM cannot be reached, the MediaStore insert fails,
/// the file descriptor cannot be opened, or the byte copy fails.
/// On error the temp file at `src_path` is left intact so the caller can
/// surface the internal-storage path as a fallback.
pub fn export_to_downloads(src_path: &Path, display_name: &str, mime: &str) -> Result<String, String> {
    let ctx = ndk_context::android_context();

    // Wrap the raw JavaVM pointer without taking ownership — Android's runtime
    // manages the JVM lifetime, so we use ManuallyDrop to prevent drop from
    // calling DestroyJavaVM.
    let vm = std::mem::ManuallyDrop::new(
        unsafe { jni::JavaVM::from_raw(ctx.vm().cast()) }
            .map_err(|e| format!("JavaVM::from_raw: {e}"))?,
    );

    let mut env = vm
        .attach_current_thread()
        .map_err(|e| format!("JNI attach_current_thread: {e}"))?;

    let fd = insert_mediastore_entry(&mut env, display_name, mime, ctx.context())
        .map_err(|e| {
            // Clear any pending Java exception so future JNI calls succeed
            let _ = env.exception_clear();
            e
        })?;

    copy_file_to_fd(src_path, fd)?;

    Ok(format!("Downloads/{display_name}"))
}

/// Insert a new row into `MediaStore.Downloads` and return the detached native
/// file descriptor for writing.
///
/// Java equivalent:
/// ```java
/// ContentValues cv = new ContentValues();
/// cv.put(MediaStore.Downloads.DISPLAY_NAME, displayName);
/// cv.put(MediaStore.Downloads.MIME_TYPE, mime);
/// Uri uri = context.getContentResolver()
///                  .insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, cv);
/// ParcelFileDescriptor pfd = context.getContentResolver()
///                                   .openFileDescriptor(uri, "w");
/// int fd = pfd.detachFd();
/// ```
fn insert_mediastore_entry(
    env: &mut JNIEnv,
    display_name: &str,
    mime: &str,
    activity_ptr: *mut std::ffi::c_void,
) -> Result<i32, String> {
    // --- Build ContentValues ---
    let cv_class = env
        .find_class("android/content/ContentValues")
        .map_err(|e| format!("find ContentValues: {e}"))?;
    let cv = env
        .new_object(cv_class, "()V", &[])
        .map_err(|e| format!("new ContentValues: {e}"))?;

    // cv.put(MediaStore.Downloads.DISPLAY_NAME, displayName)
    let display_name_key = mediastore_downloads_field(env, "DISPLAY_NAME")?;
    let display_name_val: JObject = env
        .new_string(display_name)
        .map_err(|e| format!("new_string displayName: {e}"))?
        .into();
    env.call_method(
        &cv,
        "put",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[JValue::Object(&display_name_key), JValue::Object(&display_name_val)],
    )
    .map_err(|e| format!("cv.put DISPLAY_NAME: {e}"))?;

    // cv.put(MediaStore.Downloads.MIME_TYPE, mime)
    let mime_key = mediastore_downloads_field(env, "MIME_TYPE")?;
    let mime_val: JObject = env
        .new_string(mime)
        .map_err(|e| format!("new_string mime: {e}"))?
        .into();
    env.call_method(
        &cv,
        "put",
        "(Ljava/lang/String;Ljava/lang/String;)V",
        &[JValue::Object(&mime_key), JValue::Object(&mime_val)],
    )
    .map_err(|e| format!("cv.put MIME_TYPE: {e}"))?;

    // --- Get ContentResolver ---
    let activity = unsafe { JObject::from_raw(activity_ptr as jni::sys::jobject) };
    let resolver = env
        .call_method(
            &activity,
            "getContentResolver",
            "()Landroid/content/ContentResolver;",
            &[],
        )
        .map_err(|e| format!("getContentResolver: {e}"))?
        .l()
        .map_err(|e| format!("getContentResolver result: {e}"))?;

    // --- Get MediaStore.Downloads.EXTERNAL_CONTENT_URI ---
    let base_uri = mediastore_downloads_field(env, "EXTERNAL_CONTENT_URI")?;

    // --- Insert row, get content URI ---
    let insert_sig = "(Landroid/net/Uri;Landroid/content/ContentValues;)Landroid/net/Uri;";
    let new_uri = env
        .call_method(
            &resolver,
            "insert",
            insert_sig,
            &[JValue::Object(&base_uri), JValue::Object(&cv)],
        )
        .map_err(|e| format!("ContentResolver.insert: {e}"))?
        .l()
        .map_err(|e| format!("insert result: {e}"))?;

    if new_uri.is_null() {
        return Err("ContentResolver.insert returned null URI".to_string());
    }

    // --- Open file descriptor for writing ---
    let mode_str: JObject = env
        .new_string("w")
        .map_err(|e| format!("new_string \"w\": {e}"))?
        .into();
    let open_sig = "(Landroid/net/Uri;Ljava/lang/String;)Landroid/os/ParcelFileDescriptor;";
    let pfd = env
        .call_method(
            &resolver,
            "openFileDescriptor",
            open_sig,
            &[JValue::Object(&new_uri), JValue::Object(&mode_str)],
        )
        .map_err(|e| format!("openFileDescriptor: {e}"))?
        .l()
        .map_err(|e| format!("openFileDescriptor result: {e}"))?;

    if pfd.is_null() {
        return Err("openFileDescriptor returned null".to_string());
    }

    // --- Detach the native file descriptor (transfers ownership to us) ---
    let fd = env
        .call_method(&pfd, "detachFd", "()I", &[])
        .map_err(|e| format!("pfd.detachFd: {e}"))?
        .i()
        .map_err(|e| format!("detachFd result: {e}"))?;

    Ok(fd)
}

/// Get a static field from `android.provider.MediaStore$Downloads`.
/// Handles both `String` fields (like `DISPLAY_NAME`, `MIME_TYPE`) and
/// `Uri` fields (like `EXTERNAL_CONTENT_URI`).
fn mediastore_downloads_field<'local>(
    env: &mut JNIEnv<'local>,
    field_name: &str,
) -> Result<JObject<'local>, String> {
    // Try String signature first
    let class = env
        .find_class("android/provider/MediaStore$Downloads")
        .map_err(|e| format!("find MediaStore$Downloads: {e}"))?;

    match env.get_static_field(&class, field_name, "Ljava/lang/String;") {
        Ok(v) => return v.l().map_err(|e| format!("{field_name} as Object: {e}")),
        Err(_) => {
            let _ = env.exception_clear();
        }
    }

    // Retry with Uri signature
    let class2 = env
        .find_class("android/provider/MediaStore$Downloads")
        .map_err(|e| format!("find MediaStore$Downloads (uri): {e}"))?;

    env.get_static_field(&class2, field_name, "Landroid/net/Uri;")
        .map_err(|e| format!("get_static_field {field_name}: {e}"))?
        .l()
        .map_err(|e| format!("{field_name} as Object: {e}"))
}

/// Copy all bytes from `src` into a file descriptor obtained from `detachFd`.
/// The `fd` is consumed (closed) when the `File` is dropped.
fn copy_file_to_fd(src: &Path, fd: i32) -> Result<(), String> {
    let mut src_file =
        std::fs::File::open(src).map_err(|e| format!("open temp file: {e}"))?;
    // SAFETY: `fd` was obtained via `pfd.detachFd()` which transfers ownership.
    let mut dst_file = unsafe { std::fs::File::from_raw_fd(fd) };
    std::io::copy(&mut src_file, &mut dst_file)
        .map_err(|e| format!("copy to MediaStore fd: {e}"))?;
    Ok(())
}
