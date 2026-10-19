// Embeds the app icon and version info into sundial.exe, so Explorer, Task
// Manager and the Start menu show Sundial's icon and name instead of a generic exe.
fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico")
            .set("FileDescription", "Sundial")
            .set("ProductName", "Sundial");
        res.compile().expect("embed Windows resources");
    }
}
