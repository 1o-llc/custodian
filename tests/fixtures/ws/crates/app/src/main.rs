fn main() {
    let c = core_lib::Config::new("app");
    match core_lib::socket() {
        Ok(s) => println!("{} {}", c.name, core_lib::serve_metrics(&s)),
        Err(e) => eprintln!("[{}{:04}] {}", core_lib::Code::PREFIX, e.code(), e.message()),
    }
}
