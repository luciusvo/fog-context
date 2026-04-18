fn main() {
    let mut grammars = vec![];
    
    if std::env::var("CARGO_FEATURE_KOTLIN").is_ok() {
        grammars.push(("kotlin", "grammars/tree-sitter-kotlin/src"));
    }
    if std::env::var("CARGO_FEATURE_SWIFT").is_ok() {
        grammars.push(("swift", "grammars/tree-sitter-swift/src"));
    }
    if std::env::var("CARGO_FEATURE_DART").is_ok() {
        grammars.push(("dart", "grammars/tree-sitter-dart/src"));
    }

    for (name, path) in grammars {
        let mut build = cc::Build::new();
        build.include(path);
        build.file(format!("{}/parser.c", path));
        
        build.flag_if_supported("-Wno-unused-parameter");
        build.flag_if_supported("-Wno-unused-but-set-variable");
        
        let scanner_c = format!("{}/scanner.c", path);
        let scanner_cc = format!("{}/scanner.cc", path);
        
        if std::path::Path::new(&scanner_c).exists() {
            build.file(scanner_c);
        } else if std::path::Path::new(&scanner_cc).exists() {
            build.cpp(true);
            build.file(scanner_cc);
        }

        build.compile(&format!("tree-sitter-{}", name));
    }
}
