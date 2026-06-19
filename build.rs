fn main() {
    // Windows : attacher l'icône .ico à l'exécutable + supprimer la console en release
    #[cfg(target_os = "windows")]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/Oxywall_icon.ico");
        res.compile().unwrap();

        println!("cargo:rustc-link-arg-bins=/SUBSYSTEM:WINDOWS");
        println!("cargo:rustc-link-arg-bins=/ENTRY:mainCRTStartup");
    }
    // Sur Linux/macOS : rien à faire
}