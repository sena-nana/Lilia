use crate::PlatformResult;

/// Keeps the machine awake while a remote session is active.
///
/// Only Windows exposes a process-scoped switch today; elsewhere this is a
/// no-op so callers do not need platform branches.
#[cfg(all(windows, not(test)))]
pub fn set_system_awake(active: bool) -> PlatformResult<()> {
    use windows::Win32::System::Power::{
        SetThreadExecutionState, ES_CONTINUOUS, ES_SYSTEM_REQUIRED,
    };

    let flags = if active {
        ES_CONTINUOUS | ES_SYSTEM_REQUIRED
    } else {
        ES_CONTINUOUS
    };
    let previous = unsafe { SetThreadExecutionState(flags) };
    if previous.0 == 0 {
        return Err(crate::PlatformError::new(
            "system_awake_failed",
            "SetThreadExecutionState failed",
            true,
        ));
    }
    Ok(())
}

#[cfg(any(not(windows), test))]
pub fn set_system_awake(_active: bool) -> PlatformResult<()> {
    Ok(())
}
