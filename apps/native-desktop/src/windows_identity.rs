pub const APP_USER_MODEL_ID: &str = "sena-nana.LiliaCode.NativePreview";

#[cfg(windows)]
pub fn configure() -> Result<(), String> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;

    unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(APP_USER_MODEL_ID)) }.map_err(
        |error| format!("failed to configure Native Preview application identity: {error}"),
    )
}

#[cfg(not(windows))]
pub fn configure() -> Result<(), String> {
    Ok(())
}
