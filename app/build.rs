// Bettet das Programmicon (assets/dgm-nt.ico) als Windows-Ressource in dgm-nt.exe ein,
// damit Explorer, Desktop-Verknüpfung und Taskleiste das DGM-NT-Icon zeigen.
fn main() {
    println!("cargo:rerun-if-changed=assets/dgm-nt.ico");
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/dgm-nt.ico")
            .set("ProductName", "DGM-NT")
            .set("FileDescription", "DGM-NT – Digitales Geländemodell");
        res.compile()
            .expect("Windows-Ressource (Programmicon) konnte nicht eingebunden werden");
    }
}
