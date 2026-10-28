//! Named-mutex guard that keeps a single instance per logon session.

use windows::core::w;
use windows::Win32::Foundation::{
    GetLastError, SetLastError, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE,
};
use windows::Win32::System::Threading::CreateMutexW;

/// Returns `Some(handle)` for the first instance, `None` if another one is running.
/// The handle must stay alive for the lifetime of the process.
pub fn acquire_single_instance() -> windows::core::Result<Option<HANDLE>> {
    unsafe {
        SetLastError(ERROR_SUCCESS);
        let handle = CreateMutexW(None, true, w!("Local\\bt-volume-single-instance"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            Ok(None)
        } else {
            Ok(Some(handle))
        }
    }
}
