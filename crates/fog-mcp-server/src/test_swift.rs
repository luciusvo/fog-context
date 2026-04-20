#[test]
fn test_swift_nodes() {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_swift::LANGUAGE.into()).unwrap();
    println!("SWIFT AST enum: {}", parser.parse("enum MyEnum { case foo }", None).unwrap().root_node().to_sexp());
}
