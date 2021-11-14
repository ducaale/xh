use std::env;
use std::fs::read_dir;
use std::path::Path;

use syntect::dumps::*;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSetBuilder;

fn build_syntax(dir: &str, out: &str) {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let mut builder = SyntaxSetBuilder::new();
    builder.add_from_folder(dir, true).unwrap();
    let ss = builder.build();
    dump_to_file(&ss, Path::new(&out_dir).join(out)).unwrap();
}

fn feature_status(feature: &str) -> String {
    if env::var_os(format!(
        "CARGO_FEATURE_{}",
        feature.to_uppercase().replace("-", "_")
    ))
    .is_some()
    {
        format!("+{}", feature)
    } else {
        format!("-{}", feature)
    }
}

fn features() -> String {
    feature_status("native-tls")
}

fn main() {
    for dir in &["assets", "assets/basic", "assets/large"] {
        println!("cargo:rerun-if-changed={}", dir);
        for entry in read_dir(dir).unwrap() {
            println!(
                "cargo:rerun-if-changed={}",
                entry.unwrap().path().to_str().unwrap()
            );
        }
    }

    build_syntax("assets/basic", "basic.packdump");
    build_syntax("assets/large", "large.packdump");

    let out_dir = env::var_os("OUT_DIR").unwrap();
    let ts = ThemeSet::load_from_folder("assets").unwrap();
    dump_to_file(&ts, Path::new(&out_dir).join("themepack.themedump")).unwrap();

    println!("cargo:rustc-env=XH_FEATURES={}", features());
}
