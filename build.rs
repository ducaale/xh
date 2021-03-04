use std::env;
use std::path::Path;

use syntect::dumps::*;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSetBuilder;

fn main() {
    println!("cargo:rerun-if-changed=assets");
    let out_dir = env::var_os("OUT_DIR").unwrap();

    let mut builder = SyntaxSetBuilder::new();
    builder.add_plain_text_syntax();
    builder.add_from_folder("assets", true).unwrap();
    let ss = builder.build();
    dump_to_file(&ss, Path::new(&out_dir).join("syntax.packdump")).unwrap();

    let ts = ThemeSet::load_from_folder("assets").unwrap();
    dump_to_file(&ts, Path::new(&out_dir).join("themepack.themedump")).unwrap();
}
