fn main() {
    match kernelledger_runtime::run(&std::env::args().skip(1).collect::<Vec<_>>()) {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("kernelledger-daemon: {error:#}");
            std::process::exit(1);
        }
    }
}
