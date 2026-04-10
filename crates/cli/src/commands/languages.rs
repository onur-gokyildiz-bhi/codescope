use codescope_core::parser::CodeParser;

pub fn run() {
    let parser = CodeParser::new();
    println!("Supported languages:");
    for lang in parser.supported_languages() {
        println!("  - {}", lang);
    }
}
