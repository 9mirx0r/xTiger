//! Gives the Windows executables the xTiger icon and file properties.

fn main() {
    println!("cargo:rerun-if-changed=../assets/qubis/xtiger.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../assets/qubis/xtiger.ico");
        res.set("ProductName", "xTiger-CK3");
        res.set("FileDescription", "xTiger-CK3, the Crusader Kings III mod validator");
        res.set("LegalCopyright", "Based on Tiger by amtep. GNU GPL v3.");
        res.compile().expect("could not embed the Windows resources");
    }
}
