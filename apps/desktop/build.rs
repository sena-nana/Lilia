fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    winresource::WindowsResource::new()
        .set_icon("assets/icons/icon.ico")
        .compile()
        .expect("failed to embed the application icon into the Windows executable");
}
