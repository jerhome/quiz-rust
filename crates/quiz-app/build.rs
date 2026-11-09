// Windows only: embeds the application icon and version info by compiling a .rc file with
// windres (no extra crate). A missing windres only produces a warning, never a build failure.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/icon/quiz-rust.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if let Err(e) = embed_resources() {
        println!("cargo:warning=Windows resources not embedded: {e}");
    }
}

fn embed_resources() -> Result<(), String> {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?);
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").map_err(|e| e.to_string())?);
    let icon = manifest.join("assets/icon/quiz-rust.ico");
    let version = |k: &str| std::env::var(k).unwrap_or_else(|_| "0".into());
    let (major, minor, patch) = (
        version("CARGO_PKG_VERSION_MAJOR"),
        version("CARGO_PKG_VERSION_MINOR"),
        version("CARGO_PKG_VERSION_PATCH"),
    );
    let full = std::env::var("CARGO_PKG_VERSION").unwrap_or_default();
    // The .rc parser wants forward slashes, even for Windows paths.
    let icon_path = icon.display().to_string().replace('\\', "/");
    let rc = format!(
        r#"1 ICON "{icon_path}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},0
PRODUCTVERSION {major},{minor},{patch},0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "ProductName", "Quiz Rust"
      VALUE "FileDescription", "Quiz Rust"
      VALUE "FileVersion", "{full}"
      VALUE "ProductVersion", "{full}"
      VALUE "CompanyName", "Jerome Lamartiniere"
      VALUE "LegalCopyright", "Jerome Lamartiniere"
      VALUE "OriginalFilename", "quiz-rust.exe"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let rc_path = out_dir.join("resources.rc");
    std::fs::write(&rc_path, rc).map_err(|e| e.to_string())?;
    let obj = out_dir.join("resources.o");
    let windres = std::env::var("WINDRES").unwrap_or_else(|_| {
        if cfg!(windows) {
            "windres".into()
        } else {
            "x86_64-w64-mingw32-windres".into()
        }
    });
    let status = Command::new(&windres)
        .args(["-O", "coff", "-i"])
        .arg(&rc_path)
        .arg("-o")
        .arg(&obj)
        .status()
        .map_err(|e| format!("{windres}: {e}"))?;
    if !status.success() {
        return Err(format!("{windres} failed"));
    }
    println!("cargo:rustc-link-arg-bins={}", obj.display());
    Ok(())
}
