// Compiled by curl_powershell.rs as a native curl.exe argument probe.
// Length-prefixed UTF-8 records preserve empty arguments and reveal extra calls.
use std::io::Write;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::var_os("XH_CURL_ARGV_FILE").unwrap())
        .unwrap();
    file.write_all(&(args.len() as u32).to_le_bytes()).unwrap();
    for arg in args {
        file.write_all(&(arg.len() as u32).to_le_bytes()).unwrap();
        file.write_all(arg.as_bytes()).unwrap();
    }
}
