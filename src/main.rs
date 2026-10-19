//! `custodian`: the words after it read into a declared line, run through xccute with a receipt, and said.

fn main() {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    std::process::exit(custodian::cli::main_argv(&args));
}
