fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_manifest_file("windows/octo.exe.manifest");
        res.set("ProductName", "Octo");
        res.set("FileDescription", "Multi-format data viewer and editor");
        res.set("LegalCopyright", "MIT License");
        if std::path::Path::new("assets/octo.ico").exists() {
            res.set_icon("assets/octo.ico");
        }
        res.compile().expect("Failed to compile Windows resources");
    }
}
