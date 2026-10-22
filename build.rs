fn main() {
    slint_build::compile("ui/main.slint").expect("failed to compile Slint UI");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .set("ProductName", "Rusty Browser Launcher")
            .set("FileDescription", "Rusty Browser Launcher")
            .set("CompanyName", "Rusty")
            .set("OriginalFilename", "rusty-browser-launcher.exe")
            .compile()
            .expect("failed to embed Windows resources");
    }
}
