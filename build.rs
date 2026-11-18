use std::{env, path::PathBuf, process::Command};
fn main() {
    for file in [
        "quickgamma.rc",
        "quickgamma.ico",
        "resource.h",
        "quickgamma.manifest",
    ] {
        println!("cargo:rerun-if-changed=src/{file}");
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("quickgamma.res");
    let result = Command::new("rc.exe")
        .current_dir("src")
        .args(["/nologo", "/c65001", "/fo"])
        .arg(&out)
        .arg("quickgamma.rc")
        .status()
        .expect("Windows SDK の開発者コマンドプロンプトで実行してください");
    assert!(result.success(), "リソースのコンパイルに失敗しました");
    println!("cargo:rustc-link-arg={}", out.display());
}
