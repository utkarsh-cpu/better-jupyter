fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // Helper mode owns stdout for protocol frames; it never prints a result value.
    if args.first().map(String::as_str) == Some("helper") {
        if let Err(error) = kernelledger_runtime::helper::main(&args[1..]) {
            eprintln!("kernelledger-daemon helper: {error:#}");
            std::process::exit(1);
        }
        return;
    }
    match kernelledger_runtime::run(&args) {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("kernelledger-daemon: {error:#}");
            std::process::exit(1);
        }
    }
}
