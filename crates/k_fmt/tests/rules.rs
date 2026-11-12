//! Small, direct tests of the formatter's interface and of rules that are
//! clearer inline than as golden files.

#[test]
fn refuses_a_file_with_syntax_errors() {
    let Err(k_fmt::Error::Syntax(diagnostics)) = k_fmt::format("let x = \n") else {
        panic!("a syntax error must be refused");
    };
    assert_eq!(diagnostics[0].code.to_string(), "K0122");
    let message = k_fmt::format("func f() { let a = 1 let b = 2 }\n")
        .unwrap_err()
        .to_string();
    assert_eq!(
        message,
        "the file has 1 syntax error and can't be formatted"
    );
}

#[test]
fn empty_and_blank_files() {
    assert_eq!(k_fmt::format("").unwrap(), "");
    assert_eq!(k_fmt::format("\n\n  \n").unwrap(), "");
    assert_eq!(
        k_fmt::format("// only a comment").unwrap(),
        "// only a comment\n"
    );
}

#[test]
fn line_endings_and_trailing_whitespace() {
    assert_eq!(
        k_fmt::format("let a = 1   \r\nlet b = 2\r\n\r\n\r\n// c  \r\n").unwrap(),
        "let a = 1\nlet b = 2\n\n// c\n"
    );
}

#[test]
fn is_formatted() {
    assert!(k_fmt::is_formatted("let a = 1\n").unwrap());
    assert!(!k_fmt::is_formatted("let a=1\n").unwrap());
    assert!(k_fmt::is_formatted("let a = \n").is_err());
}

#[test]
fn rewrite_leaves_the_layout_alone() {
    let source = "struct S {\n  public let a:Int\n}\n";
    assert_eq!(
        k_fmt::rewrite(source).unwrap(),
        "struct S {\n  let a:Int\n}\n"
    );
}
