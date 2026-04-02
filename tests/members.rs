use std::path::{Path, PathBuf};
use std::rc::Rc;

use assert2::check;
use docstr::docstr;

use zig_analyzer::*;

#[test]
fn test_field() {
    check(
        docstr!(
            /// foo: usize = undefined,
            /// ^~~ (usize)([runtime value])
        ),
        "foo: usize = undefined",
    );
}

#[test]
fn test_variable() {
    check(
        docstr!(
            /// var foo: usize = undefined;
            ///     ^~~ (usize)([runtime value])
        ),
        "var foo: usize = undefined",
    );
}

#[test]
fn test_function() {
    check(
        docstr!(
            /// fn foo(bar: usize, baz: bool) void {}
            ///    ^~~ (fn (usize, bool) void)([unknown value])
        ),
        "fn foo(bar: usize, baz: bool) void",
    );
}

#[test]
fn test_function_parameter() {
    check(
        docstr!(
            /// fn foo(bar: usize, baz: bool) void {}
            ///        ^~~ (usize)([runtime value])
        ),
        "bar: usize",
    );
    check(
        docstr!(
            /// fn foo(bar: usize, baz: bool) void {
            ///     _ = bar;
            ///         ^~~ (usize)([runtime value])
            /// }
        ),
        "bar: usize",
    );
}

#[test]
fn test_optional_payload() {
    check(
        docstr!(
            /// var foo: ?usize = undefined;
            /// const bar = if (foo) |payload| undefined else undefined;
            ///                       ^~~~~~~ (usize)([runtime value])
        ),
        "payload",
    );
    check(
        docstr!(
            /// var foo: ?usize = undefined;
            /// const bar = if (foo) |payload| payload else undefined;
            ///                                ^~~~~~~ (usize)([runtime value])
        ),
        "payload",
    );
    check(
        docstr!(
            /// var foo: ?usize = undefined;
            /// const bar = while (foo) |payload| undefined else undefined;
            ///                          ^~~~~~~ (usize)([runtime value])
        ),
        "payload",
    );
    check(
        docstr!(
            /// var foo: ?usize = undefined;
            /// const bar = while (foo) |payload| payload else undefined;
            ///                                   ^~~~~~~ (usize)([runtime value])
        ),
        "payload",
    );
}

#[test]
fn test_error_union_payload() {
    check(
        docstr!(
            /// var foo: error{Foo}!usize = undefined;
            /// const bar = if (foo) |payload| undefined else |err| undefined;
            ///                       ^~~~~~~ (usize)([runtime value])
        ),
        "payload",
    );
    check(
        docstr!(
            /// var foo: error{Foo}!usize = undefined;
            /// const bar = if (foo) |payload| payload else |err| undefined;
            ///                                ^~~~~~~ (usize)([runtime value])
        ),
        "payload",
    );
    check(
        docstr!(
            /// var foo: error{Foo}!usize = undefined;
            /// const bar = while (foo) |payload| undefined else |err| undefined;
            ///                          ^~~~~~~ (usize)([runtime value])
        ),
        "payload",
    );
    check(
        docstr!(
            /// var foo: error{Foo}!usize = undefined;
            /// const bar = while (foo) |payload| payload else |err| undefined;
            ///                                   ^~~~~~~ (usize)([runtime value])
        ),
        "payload",
    );
}

#[test]
fn test_error_union_error() {
    check(
        docstr!(
            /// var foo: error{Foo}!usize = undefined;
            /// const bar = if (foo) |payload| undefined else |err| undefined;
            ///                                                ^~~ (error{Foo})([runtime value])
        ),
        "err",
    );
    check(
        docstr!(
            /// var foo: error{Foo}!usize = undefined;
            /// const bar = if (foo) |payload| undefined else |err| err;
            ///                                                     ^~~ (error{Foo})([runtime value])
        ),
        "err",
    );
    check(
        docstr!(
            /// var foo: error{Foo}!usize = undefined;
            /// const bar = while (foo) |payload| undefined else |err| undefined;
            ///                                                   ^~~ (error{Foo})([runtime value])
        ),
        "err",
    );
    check(
        docstr!(
            /// var foo: error{Foo}!usize = undefined;
            /// const bar = while (foo) |payload| undefined else |err| err;
            ///                                                        ^~~ (error{Foo})([runtime value])
        ),
        "err",
    );
    check(
        docstr!(
            /// var foo: error{Foo}!usize = undefined;
            /// const bar = foo catch |err| undefined;
            ///                        ^~~ (error{Foo})([runtime value])
        ),
        "err",
    );
    check(
        docstr!(
            /// var foo: error{Foo}!usize = undefined;
            /// const bar = foo catch |err| err;
            ///                             ^~~ (error{Foo})([runtime value])
        ),
        "err",
    );
}

fn check(annotated: &str, expected_def: &str) {
    let mut source = String::new();
    let mut opt_annotation = None;
    for (idx, s) in annotated.lines().enumerate() {
        let trimmed = s.trim_start();
        if !trimmed.starts_with('^') {
            source.push_str(s);
            source.push('\n');
            continue;
        }
        let line = idx - 1;
        let character = s.len() - trimmed.len();
        let mut length = 1;
        for c in trimmed.chars().skip(1) {
            if c == '~' {
                length += 1;
            } else {
                break;
            }
        }
        let expr_info = trimmed[length..].trim_start();
        let ty = parse_parenthesized(expr_info);
        let val = parse_parenthesized(&expr_info[2 + ty.len()..]);
        opt_annotation = Some((line, character, ty, val));
    }
    let (line, character, expected_ty, expected_val) = opt_annotation.expect("no annotation found");

    let mut ip = InternPool::new();
    let mut cache = AnalyzerCache::new();
    let mut documents = DocumentStore::new();
    let env = Env::find().expect("unable to find Zig");

    let path = Rc::<Path>::from(PathBuf::from("/foo/bar.zig"));
    let bytes = source.into_bytes();
    let document = documents.parse(path.clone(), Some(bytes)).unwrap();
    let tree = document.tree().clone();
    let handle = Handle(path.clone(), tree.clone());

    let token_index = document.position_to_token(line as u32, character as u32);
    let container = document.enclosing_container(token_index).unwrap();
    let doc_node = document.enclosing_nodes(token_index).last().unwrap();

    let this = Node(handle.clone(), container.index);
    let node = Node(handle.clone(), doc_node.index);

    let mut analyzer = Analyzer {
        ip: &mut ip,
        cache: &mut cache,
        documents: &mut documents,
        std_dir: Some(&env.std_dir),
        this,
    };
    let mut member_info = None;
    let opt_expr = analyzer.resolve_from_token(&node, token_index, Some(&mut member_info));

    let Expr(ty, val) = opt_expr.expect("failed to resolve token");
    check!(expected_ty == format!("{}", ty.display(&ip)));
    check!(expected_val == format!("{}", val.display(&ip)));

    let (handle, member) = member_info.unwrap();
    let Handle(_, tree) = handle;
    let def = str::from_utf8(member.def_slice(&tree)).unwrap();
    check!(expected_def == def);
}

fn parse_parenthesized(s: &str) -> &str {
    match s.chars().next() {
        Some('(') => {}
        Some(c) => panic!("expected '(' but got '{c}'"),
        None => panic!("expected '('"),
    }
    let mut length = 0;
    let mut parens = 1;
    for c in s.chars().skip(1) {
        match c {
            '(' => parens += 1,
            ')' => parens -= 1,
            _ => {}
        }
        if parens == 0 {
            break;
        }
        length += 1;
    }
    if parens != 0 {
        panic!("expected ')'")
    }
    &s[1..1 + length]
}
