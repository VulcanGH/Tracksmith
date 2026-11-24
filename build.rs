use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=packaging/tracksmith.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let numbers = [
        "CARGO_PKG_VERSION_MAJOR",
        "CARGO_PKG_VERSION_MINOR",
        "CARGO_PKG_VERSION_PATCH",
    ]
    .map(|key| env::var(key).unwrap())
    .join(",");
    let icon = root
        .join("packaging/tracksmith.ico")
        .display()
        .to_string()
        .replace('\\', "/");
    let resource = format!(
        r#"
1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numbers},0
PRODUCTVERSION {numbers},0
FILEFLAGSMASK 0x3fL
FILEFLAGS 0
FILEOS 0x40004L
FILETYPE 1
FILESUBTYPE 0
BEGIN
 BLOCK "StringFileInfo"
 BEGIN
  BLOCK "040904B0"
  BEGIN
   VALUE "CompanyName", "VulcanGH\0"
   VALUE "FileDescription", "Tracksmith - Media metadata. Refined.\0"
   VALUE "FileVersion", "{version}\0"
   VALUE "ProductName", "Tracksmith\0"
   VALUE "ProductVersion", "{version}\0"
   VALUE "OriginalFilename", "tracksmith.exe\0"
   VALUE "LegalCopyright", "Copyright (c) 2026 VulcanGH and contributors\0"
  END
 END
 BLOCK "VarFileInfo"
 BEGIN
  VALUE "Translation", 0x409, 1200
 END
END
"#
    );
    let input = out.join("tracksmith.rc");
    fs::write(&input, resource).unwrap();
    let msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let output = out.join(if msvc {
        "tracksmith.res"
    } else {
        "tracksmith.o"
    });
    let mut command = if msvc {
        let sdk = env::var_os("WindowsSdkDir")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env::var_os("ProgramFiles(x86)").unwrap()).join("Windows Kits/10")
            });
        let mut compilers: Vec<_> = fs::read_dir(sdk.join("bin"))
            .expect("Windows SDK is required")
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("x64/rc.exe"))
            .filter(|path| path.is_file())
            .collect();
        compilers.sort();
        let mut cmd = Command::new(
            compilers
                .last()
                .expect("Windows SDK resource compiler is required"),
        );
        cmd.arg("/nologo").arg("/fo").arg(&output).arg(&input);
        cmd
    } else {
        let local = root.join(".tools/w64devkit/bin/windres.exe");
        let mut cmd = Command::new(env::var_os("WINDRES").map(PathBuf::from).unwrap_or_else(
            || {
                if local.is_file() {
                    local
                } else {
                    "windres".into()
                }
            },
        ));
        cmd.arg("--input")
            .arg(&input)
            .args(["--output-format", "coff", "--output"])
            .arg(&output);
        cmd
    };
    assert!(
        command
            .status()
            .expect("Starting Windows resource compiler")
            .success(),
        "Windows resource compilation failed"
    );
    println!("cargo:rustc-link-arg-bin=tracksmith={}", output.display());
}
