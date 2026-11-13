fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=../../assets/icons/langhian.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icons/langhian.ico");
        if let Err(err) = res.compile() {
            println!("cargo:warning=Failed to compile Windows icon resource: {err}");
        }
    }
}
