// Zweck:    Übergibt die Buildnummer an das Programm und bettet Icon und Versionsangaben in die EXE ein
// Hardware: keine
// Autor:    Johann Gerner

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=BUILDNUMMER");
    let build: u32 = include_str!("BUILDNUMMER")
        .trim()
        .parse()
        .expect("BUILDNUMMER muss eine positive ganze Zahl enthalten");
    assert!(build > 0, "BUILDNUMMER muss größer als null sein");
    println!("cargo:rustc-env=APP_BUILD_NUMMER={build}");
    #[cfg(windows)]
    {
        let version = format!("{}.{build}", env!("CARGO_PKG_VERSION"));
        let mut resource = winresource::WindowsResource::new();
        resource
            .set("ProductName", "DGM-NT")
            .set("FileDescription", "DGM-NT - Digitales Gelaendemodell")
            .set("CompanyName", "Johann Gerner")
            .set("LegalCopyright", "(c) 2026 Johann Gerner")
            .set("FileVersion", &version)
            .set("ProductVersion", &version);
        println!("cargo:rerun-if-changed=assets/dgm-nt.ico");
        resource.set_icon("assets/dgm-nt.ico");
        if let Err(error) = resource.compile() {
            // ohne Ressourcen-Compiler (rc.exe) entsteht die EXE trotzdem
            println!("cargo:warning=Versionsangaben konnten nicht eingebettet werden: {error}");
        }
    }
}
