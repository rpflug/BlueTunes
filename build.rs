fn main() {
    println!("cargo:rerun-if-changed=assets/bluetunes.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/bluetunes.ico")
            .set("ProductName", "BlueTunes")
            .set("FileDescription", "BlueTunes Music Player")
            .compile()
            .expect("compile Windows application resources");
    }
}
