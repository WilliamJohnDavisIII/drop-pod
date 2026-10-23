// Windows: Drop Pod's icon and name on the .exe (Explorer, shortcuts). Nothing to do elsewhere.
fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/icon/droppod.ico")
            .set("ProductName", "Drop Pod")
            .set("FileDescription", "Drop Pod");
        match res.compile() {
            Ok(()) => {}
            // (a cross-build from macOS or Linux without windres still builds, just without the icon)
            Err(e) if !cfg!(windows) => println!("cargo:warning=no Windows icon embedded: {e}"),
            Err(e) => panic!("embedding the Windows icon: {e}"),
        }
    }
    println!("cargo:rerun-if-changed=../assets/icon/droppod.ico");
}
