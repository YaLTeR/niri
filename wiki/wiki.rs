use mdbook::MDBook;
use mdbook_alerts;

fn main() {
    let mut md = MDBook::load(".").expect("Unable to load the book");
    md.with_preprocessor(mdbook_alerts::Preprocessor);
    md.build().expect("Building failed");
}
