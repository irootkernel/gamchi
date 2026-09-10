use docscheck::{check_file, check_leftover_names, ROADMAP_PATH};

fn main() {
    let mut vs = check_file(ROADMAP_PATH);
    vs.extend(check_leftover_names("."));
    if vs.is_empty() {
        return;
    }
    for v in vs {
        eprintln!("{v}");
    }
    std::process::exit(1);
}
