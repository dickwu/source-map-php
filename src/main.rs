fn main() {
    if let Err(err) = source_map_php::run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}
