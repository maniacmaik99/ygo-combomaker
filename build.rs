//! Build-Skript: bettet unter Windows das App-Icon (assets/icon.ico) und
//! Versionsinfos in die .exe ein. Auf anderen Systemen passiert nichts.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon.ico");

    // Zielsystem prüfen (nicht das Build-System), damit auch Cross-Builds stimmen.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.set("ProductName", "YGO Combomaker");
        res.set("FileDescription", "YGO Combomaker");
        if let Err(e) = res.compile() {
            // Kein harter Fehler: die App funktioniert auch ohne eingebettetes Icon.
            println!("cargo:warning=Windows-Icon konnte nicht eingebettet werden: {e}");
        }
    }
}
