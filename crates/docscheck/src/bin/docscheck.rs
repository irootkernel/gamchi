use docscheck::{check_file, ROADMAP_PATH};

fn main() {
    let vs = check_file(ROADMAP_PATH);
    if vs.is_empty() {
        return;
    }
    for v in vs {
        eprintln!("{v}");
    }
    std::process::exit(1);
}
