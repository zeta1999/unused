use read_ctags::TagsReader;
use serde_json;

fn main() {
    match TagsReader::default().load() {
        Ok(outcome) => println!("{}", serde_json::to_string(&outcome).unwrap()),
        Err(e) => eprintln!("{}", e),
    }
}
