fn main() {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_ruby::LANGUAGE.into()).unwrap();
    let source_code = std::fs::read_to_string("test.rb").unwrap();
    let tree = parser.parse(&source_code, None).unwrap();
    println!("{}", tree.root_node().to_sexp());
}
