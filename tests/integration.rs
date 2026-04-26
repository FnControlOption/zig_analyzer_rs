use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use assert2::check;
use docstr::docstr;
use zig_ast::*;

use zig_analyzer::*;

struct Test {
    ip: InternPool,
    cache: AnalyzerCache,
    documents: DocumentStore,
    env: Env,
    counter: usize,
}

impl Test {
    fn new() -> Self {
        Self {
            ip: InternPool::new(),
            cache: AnalyzerCache::new(),
            documents: DocumentStore::new(),
            env: Env::find().expect("unable to find Zig"),
            counter: 0,
        }
    }

    fn analyzer(&mut self) -> Analyzer<'_, '_, '_, '_> {
        Analyzer {
            ip: &mut self.ip,
            cache: &mut self.cache,
            documents: &mut self.documents,
            std_dir: Some(&self.env.std_dir),
        }
    }

    fn intern_type(&mut self, interned: InternedType) -> Type {
        Type::Interned(self.ip.intern_type(interned))
    }

    fn format_type(&self, ty: Type) -> String {
        format!("{}", ty.display(&self.ip))
    }

    fn format_value(&self, value: Value) -> String {
        format!("{}", value.display(&self.ip))
    }

    fn parse_source<T: Into<Vec<u8>>>(&mut self, source: T, path: Option<PathBuf>) -> Node {
        let path = Rc::<Path>::from(path.unwrap_or_else(|| {
            self.counter += 1;
            PathBuf::from(format!("/foo/bar{}.zig", self.counter))
        }));
        let source = source.into();
        let document = self.documents.parse(path.clone(), Some(source)).unwrap();
        let tree = document.tree();
        let handle = Handle(path.clone(), tree.clone());
        Node(handle, NodeIndex::ROOT)
    }

    fn parse_expression<S: AsRef<str>>(&mut self, s: S, path: Option<PathBuf>) -> Node {
        let source = format!("const foo = {};", s.as_ref());
        let root = self.parse_source(source, path);
        let handle = root.handle();
        let tree = handle.tree();
        let index = tree.root_decls().next().unwrap();
        let var_decl: full::VarDecl = tree.full_node(index).unwrap();
        let init = var_decl.ast.init_node.to_option().unwrap();
        Node::from(handle, init)
    }

    fn resolve_binding<S: AsRef<str>>(&mut self, s: S) -> Binding {
        let node = self.parse_expression(s, None);
        self.analyzer().resolve_binding(node)
    }

    fn resolve_expr<S: AsRef<str>>(&mut self, s: S) -> Expr {
        let node = self.parse_expression(s, None);
        self.analyzer().resolve_expr(node)
    }

    fn resolve_type<S: AsRef<str>>(&mut self, s: S) -> Type {
        let node = self.parse_expression(s, None);
        self.analyzer().resolve_type(node)
    }

    fn resolve_peer_types(&mut self, peer_types: &[Type]) -> Type {
        self.analyzer().resolve_peer_types(peer_types)
    }
}

fn decl(var: &str, ty: &str) -> String {
    format!("struct {{ {var} foo: {ty} = undefined; }}.foo")
}

// +----------------------------+
// |          Bindings          |
// +----------------------------+

#[test]
fn test_identifier() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Type, Value::Type(Type::Type))) == test.resolve_binding("type")
    }
    check! {
        Binding::Constant(Expr(Type::Bool, Value::True)) == test.resolve_binding("true")
    }
    check! {
        Binding::Constant(Expr(Type::Undefined, Value::Undefined)) == test.resolve_binding("undefined")
    }
    check! {
        Binding::Constant(Expr(Type::Type, Value::Type(Type::Int(Signedness::Signed, 32)))) == test.resolve_binding("i32")
    }
    check! {
        Binding::Constant(Expr(Type::Type, Value::Type(Type::Int(Signedness::Unsigned, 31)))) == test.resolve_binding("u31")
    }
    check! {
        Binding::Constant(Expr(Type::Type, Value::Type(Type::Float(80)))) == test.resolve_binding("f80")
    }
}

#[test]
fn test_identifier_decl() {
    let mut test = Test::new();

    let source = docstr!(
        /// const a: @This() = undefined;
        /// const b = a;
    );
    let root = test.parse_source(source, None);
    let handle = root.handle();
    let tree = handle.tree();
    let root_decls: Vec<_> = tree.root_decls().collect();

    let index = root_decls[1];
    let var_decl: full::VarDecl = tree.full_node(index).unwrap();
    let init = var_decl.ast.init_node.to_option().unwrap();
    let b_node = Node::from(handle, init);

    check! {
        let Expr(Type::Interned(index), Value::Undefined) = test.analyzer().resolve_expr(b_node)
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Container(container_type) = interned
        && container_type.this().index() == NodeIndex::ROOT
    }
}

#[test]
fn test_unwrap_optional() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Undefined)) == test.resolve_binding("@as(?usize, undefined).?")
    }
}

#[test]
fn test_unwrap_optional_variable() {
    let mut test = Test::new();
    let var_opt_usize = decl("var", "?usize");
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Runtime)) == test.resolve_binding(format!("{var_opt_usize}.?"))
    }
}

#[test]
fn test_orelse() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Undefined)) == test.resolve_binding("@as(?usize, undefined) orelse @as(usize, undefined)")
    }
}

#[test]
fn test_orelse_variable() {
    let mut test = Test::new();
    let var_opt_usize = decl("var", "?usize");
    let var_usize = decl("var", "usize");
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Runtime)) == test.resolve_binding(format!("{var_opt_usize} orelse {var_usize}"))
    }
}

#[test]
fn test_orelse_peer_types() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Int(Signedness::Signed, 32), Value::Unknown)) == test.resolve_binding("@as(?i16, undefined) orelse @as(i32, undefined)")
    }
}

#[test]
fn test_try() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Undefined)) == test.resolve_binding("try @as(error{Foo}!usize, undefined)")
    }
}

#[test]
fn test_try_variable() {
    let mut test = Test::new();
    let var_error_union_usize = decl("var", "error{Foo}!usize");
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Runtime)) == test.resolve_binding(format!("try {var_error_union_usize}"))
    }
}

#[test]
fn test_catch() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Undefined)) == test.resolve_binding("@as(error{Foo}!usize, undefined) catch @as(usize, undefined)")
    }
}

#[test]
fn test_catch_variable() {
    let mut test = Test::new();
    let var_error_union_usize = decl("var", "error{Foo}!usize");
    let var_usize = decl("var", "usize");
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Runtime)) == test.resolve_binding(format!("{var_error_union_usize} catch {var_usize}"))
    }
}

#[test]
fn test_catch_peer_types() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Int(Signedness::Signed, 32), Value::Unknown)) == test.resolve_binding("@as(error{Foo}!i16, undefined) catch @as(i32, undefined)")
    }
}

#[test]
fn test_deref() {
    let mut test = Test::new();
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as(*usize, undefined).*")
    }
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as(*const usize, undefined).*")
    }
}

#[test]
fn test_array_access() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as(@Vector(3, usize), undefined)[1]")
    }
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as([3]usize, undefined)[1]")
    }
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as(*@Vector(3, usize), undefined)[1]")
    }
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as(*[3]usize, undefined)[1]")
    }
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as([]usize, undefined)[1]")
    }
}

#[test]
#[ignore]
fn test_slice() {
    let mut test = Test::new();
    let var_usize = decl("var", "usize");
    let pointer_ty = test.resolve_type("*[2]usize");
    let slice_ty = test.resolve_type("[]usize");
    check! {
        Binding::Constant(Expr(pointer_ty, Value::Unknown)) == test.resolve_binding("@as([]usize, undefined)[1..3]")
    }
    check! {
        Binding::Constant(Expr(slice_ty, Value::Unknown)) == test.resolve_binding(format!("@as([]usize, undefined)[1..{var_usize}]"))
    }
    check! {
        Binding::Constant(Expr(slice_ty, Value::Unknown)) == test.resolve_binding(format!("@as([]usize, undefined)[{var_usize}..3]"))
    }
    check! {
        Binding::Constant(Expr(slice_ty, Value::Unknown)) == test.resolve_binding(format!("@as([]usize, undefined)[{var_usize}..{var_usize}]"))
    }
}

#[test]
fn test_decl_access_var() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Undefined)) == test.resolve_binding("struct { const foo: usize = undefined; }.foo")
    }
    check! {
        Binding::Unknown == test.resolve_binding("@as(struct { const foo: usize = undefined; }, undefined).foo")
    }
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Runtime)) == test.resolve_binding("struct { var foo: usize = undefined; }.foo")
    }
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Undefined)) == test.resolve_binding("struct { const foo = @as(usize, undefined); }.foo")
    }
    check! {
        let ty = test.resolve_type("struct { const Foo = @This(); }.Foo")
        && test.format_type(ty) == "Foo"
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Container(_) = interned
    }
}

#[test]
fn test_decl_access_fn() {
    let mut test = Test::new();
    check! {
        let Binding::Constant(Expr(ty, value)) = test.resolve_binding("struct { fn foo(_: usize) void {} }.foo")
        && ty == test.resolve_type("fn (usize) void")
        && test.format_value(value) == "[function 'foo']"
    }
    check! {
        Binding::Unknown == test.resolve_binding("@as(struct { fn foo(_: usize) void {} }, undefined).foo")
    }
    check! {
        let Binding::Constant(Expr(ty, value)) = test.resolve_binding("struct { fn foo(_: @This()) void {} }.foo")
        && test.format_value(value) == "[function 'foo']"
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Function(function_type) = interned
        && let FunctionType {
            params,
            has_callconv: false,
            return_type: Type::Void
        } = function_type
        && let &[param] = params.as_slice()
        && test.format_type(param) == "struct { fn foo(_: @This()) void {} }"
        && let Type::Interned(index) = param
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Container(_) = interned
    }
}

#[test]
fn test_field_access_array() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as([3]usize, undefined).len")
    }
}

#[test]
fn test_field_access_slice() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as([]usize, undefined).len")
    }
    check! {
        let ptr_ty = test.resolve_type("[*]usize")
        && Binding::Constant(Expr(ptr_ty, Value::Unknown)) == test.resolve_binding("@as([]usize, undefined).ptr")
    }
}

#[test]
fn test_field_access_tuple() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Int(2))) == test.resolve_binding("@as(struct { usize, bool }, undefined).len")
    }
    check! {
        Binding::Constant(Expr(Type::Bool, Value::Unknown)) == test.resolve_binding(r#"@as(struct { usize, bool }, undefined).@"1""#)
    }
}

#[test]
fn test_field_access_container() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as(struct { foo: usize }, undefined).foo")
    }
    check! {
        let Binding::Constant(Expr(ty, value)) = test.resolve_binding("@as(struct { foo: @This() }, undefined).foo")
        && test.format_type(ty) == "struct { foo: @This() }"
        && let (Type::Interned(index), Value::Unknown) = (ty, value)
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Container(_) = interned
    }
}

#[test]
fn test_field_access_variable_container() {
    let mut test = Test::new();
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Runtime)) == test.resolve_binding("struct { var foo: struct { bar: usize } = undefined; }.foo.bar")
    }
}

#[test]
fn test_field_access_pointer() {
    let mut test = Test::new();
    check! {
        Binding::Constant(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as(*const struct { foo: usize }, undefined).foo")
    }
    check! {
        Binding::Variable(Expr(Type::Usize, Value::Unknown)) == test.resolve_binding("@as(*struct { foo: usize }, undefined).foo")
    }
}

// +-------------------------------+
// |          Expressions          |
// +-------------------------------+

#[test]
fn test_address_of() {
    let mut test = Test::new();
    check! {
        let pointer_ty = test.resolve_type("*const usize")
        && Expr(pointer_ty, Value::Unknown) == test.resolve_expr("&@as(usize, undefined)")
    }
    check! {
        let pointer_ty = test.resolve_type("*usize")
        && Expr(pointer_ty, Value::Runtime) == test.resolve_expr("&struct { var foo: usize = undefined; }.foo")
    }
}

#[test]
fn test_enum_literal() {
    let mut test = Test::new();
    check! {
        let Expr(Type::EnumLiteral, Value::Interned(index)) = test.resolve_expr(".foo")
        && let Some(interned) = test.ip.get_value(index)
        && let InternedValue::EnumLiteral(name) = interned
        && name == "foo".as_bytes()
    }
}

#[test]
fn test_error_value() {
    let mut test = Test::new();
    check! {
        let Expr(Type::Interned(ty_index), Value::Interned(value_index)) = test.resolve_expr("error.Foo")
        && let Some(interned) = test.ip.get_type(ty_index)
        && let InternedType::ErrorSet(names) = interned
        && names == &BTreeSet::from(["Foo".into()])
        && let Some(interned) = test.ip.get_value(value_index)
        && let InternedValue::ErrorValue(name) = interned
        && name == "Foo".as_bytes()
    }
}

#[test]
fn test_builtin_call_as() {
    let mut test = Test::new();
    check! {
        Expr(Type::Type, Value::Type(Type::Bool)) == test.resolve_expr("@as(type, bool)")
    }
    check! {
        Expr(Type::Type, Value::Undefined) == test.resolve_expr("@as(type, undefined)")
    }
    check! {
        Expr(Type::Bool, Value::Undefined) == test.resolve_expr("@as(bool, undefined)")
    }
    check! {
        Expr(Type::Bool, Value::Type(Type::Isize)) == test.resolve_expr("@as(bool, isize)")
    }
    check! {
        Expr(Type::Unknown, Value::Undefined) == test.resolve_expr("@as(undefined, undefined)")
    }
    check! {
        let Expr(Type::Interned(index), Value::Type(Type::Bool)) = test.resolve_expr("@as(?type, bool)")
        && let Some(interned) = test.ip.get_type(index)
        && &InternedType::Optional(Type::Type) == interned
    }
}

#[test]
fn test_builtin_call_import() {
    let tempdir = tempfile::tempdir().unwrap();
    let dir_path = tempdir.path();
    let foo_path = dir_path.join("foo.zig");
    let bar_path = dir_path.join("bar.zig");

    std::fs::write(&foo_path, "foo: f32").unwrap();

    let mut test = Test::new();
    let node = test.parse_expression(r#"@import("foo.zig");"#, Some(bar_path));

    check! {
        let Expr(Type::Type, Value::Type(ty)) = test.analyzer().resolve_expr(node)
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Container(container_type) = interned
        && container_type.this().handle().path().as_ref() == foo_path
        && container_type.this().index() == NodeIndex::ROOT
    }
}

#[test]
fn test_builtin_call_import_std() {
    let mut test = Test::new();
    check! {
        let Expr(Type::Type, Value::Type(ty)) = test.resolve_expr(r#"@import("std")"#)
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Container(container_type) = interned
        && container_type.this().handle().path().parent() == Some(test.env.std_dir.as_path())
        && container_type.this().index() == NodeIndex::ROOT
    }
}

#[test]
fn test_builtin_call_this() {
    let mut test = Test::new();
    check! {
        let ty = test.resolve_type("@This()")
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Container(container_type) = interned
        && container_type.this().index() == NodeIndex::ROOT
    }
}

#[test]
fn test_builtin_call_type_of() {
    let mut test = Test::new();
    check! {
        Expr(Type::Type, Value::Type(Type::Type)) == test.resolve_expr("@TypeOf(bool)")
    }
    check! {
        Expr(Type::Type, Value::Type(Type::Bool)) == test.resolve_expr("@TypeOf(true)")
    }
}

#[test]
fn test_builtin_call_type_of_peer_types() {
    let mut test = Test::new();
    check! {
        test.resolve_expr("?usize") == test.resolve_expr("@TypeOf(@as(usize, undefined), @as(?usize, undefined))")
    }
}

#[test]
fn test_builtin_call_vector() {
    let mut test = Test::new();
    check! {
        let ty = test.resolve_type("@Vector(3, usize)")
        && test.format_type(ty) == "@Vector(?, usize)"
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && &InternedType::Vector(Type::Usize) == interned
    }
}

#[test]
fn test_if() {
    let mut test = Test::new();
    check! {
        Expr(Type::Void, Value::Unknown) == test.resolve_expr("if (undefined) @as(void, undefined)")
    }
    check! {
        let ty = test.resolve_type("if (undefined) usize else bool")
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Branched(types) = interned
        && types == &BTreeSet::from([TypeOrd(Type::Usize), TypeOrd(Type::Bool)])
    }
}

#[test]
fn test_if_nested_branched_type() {
    let mut test = Test::new();
    check! {
        let branched_ty = test.resolve_type("if (undefined) usize else bool")
        && let ty = test.resolve_type("if (undefined) ?usize else ?bool")
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && &InternedType::Optional(branched_ty) == interned
    }
}

#[test]
fn test_if_peer_types() {
    let mut test = Test::new();
    check! {
        let optional_ty = test.resolve_type("?usize")
        && Expr(optional_ty, Value::Unknown) == test.resolve_expr("if (undefined) @as(usize, undefined) else @as(?usize, undefined)")
    }
}

#[test]
fn test_number_literal() {
    let mut test = Test::new();
    check! {
        Expr(Type::ComptimeInt, Value::Int(1)) == test.resolve_expr("1")
    }
    check! {
        Expr(Type::ComptimeFloat, Value::Unknown) == test.resolve_expr("2.3")
    }
}

#[test]
fn test_char_literal() {
    let mut test = Test::new();
    check! {
        Expr(Type::ComptimeInt, Value::Unknown) == test.resolve_expr("'e'")
    }
}

#[test]
fn test_string_literal() {
    let mut test = Test::new();
    check! {
        let pointer_ty = test.resolve_type("*const [3:0]u8")
        && Expr(pointer_ty, Value::Unknown) == test.resolve_expr(r#""foo""#)
    }
}

#[test]
fn test_multiline_string_literal() {
    let mut test = Test::new();
    check! {
        let pointer_ty = test.resolve_type("*const [7:0]u8")
        && Expr(pointer_ty, Value::Unknown) == test.resolve_expr(docstr!(
            /// \\foo
            /// \\bar
            ///
        ))
    }
}

#[test]
fn test_array_init() {
    let mut test = Test::new();
    check! {
        let array_ty = test.resolve_type("[3]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("[_]usize{ 1, 2, 3 }")
    }
}

#[test]
fn test_array_init_dot() {
    let mut test = Test::new();
    check! {
        let tuple_ty = test.resolve_type("struct { comptime_int, comptime_float }")
        && Expr(tuple_ty, Value::Unknown) == test.resolve_expr(".{ 1, 2.3 }")
    }
}

#[test]
fn test_struct_init() {
    let mut test = Test::new();
    check! {
        let Expr(ty, value) = test.resolve_expr("struct { foo: usize }{ .foo = 1 }")
        && test.format_type(ty) == "struct { foo: usize }"
        && let (Type::Interned(index), Value::Unknown) = (ty, value)
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Container(_) = interned
    }
}

#[test]
fn test_struct_init_dot() {
    let mut test = Test::new();
    check! {
        let Expr(ty, Value::Unknown) = test.resolve_expr(".{}")
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && &InternedType::Tuple(vec![]) == interned
    }
    check! {
        Expr(Type::Unknown, Value::Unknown) == test.resolve_expr(".{ .foo = 1 }")
    }
}

#[test]
fn test_grouped_expression() {
    let mut test = Test::new();
    check! {
        Expr(Type::Type, Value::Type(Type::Bool)) == test.resolve_expr("(bool)")
    }
}

#[test]
fn test_comptime() {
    let mut test = Test::new();
    check! {
        Expr(Type::Type, Value::Type(Type::Bool)) == test.resolve_expr("comptime bool")
    }
}

#[test]
fn test_nosuspend() {
    let mut test = Test::new();
    check! {
        Expr(Type::Type, Value::Type(Type::Bool)) == test.resolve_expr("nosuspend bool")
    }
}

#[test]
fn test_comparison() {
    let mut test = Test::new();
    check! {
        Expr(Type::Bool, Value::Unknown) == test.resolve_expr("undefined < undefined")
    }
}

#[test]
fn test_comparison_vector() {
    let mut test = Test::new();
    check! {
        let vector_ty = test.resolve_type("@Vector(3, bool)")
        && Expr(vector_ty, Value::Unknown) == test.resolve_expr("@as(@Vector(3, usize), undefined) < undefined")
    }
    check! {
        let vector_ty = test.resolve_type("@Vector(3, bool)")
        && Expr(vector_ty, Value::Unknown) == test.resolve_expr("undefined < @as(@Vector(3, usize), undefined)")
    }
}

#[test]
fn test_array_mult() {
    let mut test = Test::new();
    check! {
        let array_ty = test.resolve_type("[6]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as([2]usize, undefined) ** 3")
    }
    check! {
        let array_ty = test.resolve_type("[6:0]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as([2:0]usize, undefined) ** 3")
    }
}

#[test]
fn test_array_mult_pointer() {
    let mut test = Test::new();
    check! {
        let array_ty = test.resolve_type("*const [6]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as(*const [2]usize, undefined) ** 3")
    }
}

#[test]
fn test_array_mult_tuple() {
    let mut test = Test::new();
    check! {
        let tuple_ty = test.resolve_type("struct { i32, f64, i32, f64 }")
        && Expr(tuple_ty, Value::Unknown) == test.resolve_expr("@as(struct { i32, f64 }, undefined) ** 2")
    }
    check! {
        let tuple_ty = test.resolve_type("struct { i32, f64, i32, f64, i32, f64 }")
        && Expr(tuple_ty, Value::Unknown) == test.resolve_expr("@as(struct { i32, f64 }, undefined) ** 3")
    }
}

#[test]
fn test_array_cat() {
    let mut test = Test::new();
    check! {
        let array_ty = test.resolve_type("[5]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as([2]usize, undefined) ++ @as([3]usize, undefined)")
    }
    check! {
        let array_ty = test.resolve_type("[5:0]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as([2:0]usize, undefined) ++ @as([3]usize, undefined)")
    }
    check! {
        let array_ty = test.resolve_type("[5:1]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as([2]usize, undefined) ++ @as([3:1]usize, undefined)")
    }
    check! {
        let array_ty = test.resolve_type("[5]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as([2:0]usize, undefined) ++ @as([3:1]usize, undefined)")
    }
}

#[test]
fn test_array_cat_pointers() {
    let mut test = Test::new();
    check! {
        let array_ty = test.resolve_type("*const [5]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as(*const [2]usize, undefined) ++ @as(*const [3]usize, undefined)")
    }
    check! {
        let array_ty = test.resolve_type("*const [5]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as(*[2]usize, undefined) ++ @as(*[3]usize, undefined)")
    }
    check! {
        let array_ty = test.resolve_type("*const [5]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as(*[2]usize, undefined) ++ @as([3]usize, undefined)")
    }
    check! {
        let array_ty = test.resolve_type("*const [5]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as(*const [2]usize, undefined) ++ @as([3]usize, undefined)")
    }
    check! {
        let array_ty = test.resolve_type("*const [5]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as([2]usize, undefined) ++ @as(*[3]usize, undefined)")
    }
    check! {
        let array_ty = test.resolve_type("*const [5]usize")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as([2]usize, undefined) ++ @as(*const [3]usize, undefined)")
    }
}

#[test]
fn test_array_cat_tuples() {
    let mut test = Test::new();
    check! {
        let tuple_ty = test.resolve_type("struct { i32, f64, f80, u16 }")
        && Expr(tuple_ty, Value::Unknown) == test.resolve_expr("@as(struct { i32, f64 }, undefined) ++ @as(struct { f80, u16 }, undefined)")
    }
}

#[test]
fn test_array_cat_peer_types() {
    let mut test = Test::new();
    check! {
        let array_ty = test.resolve_type("[5]u64")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as([2]u64, undefined) ++ @as([3]u16, undefined)")
    }
    check! {
        let array_ty = test.resolve_type("[5]u64")
        && Expr(array_ty, Value::Unknown) == test.resolve_expr("@as([2]u64, undefined) ++ @as(struct { u32, u16, u8 }, undefined)")
    }
}

#[test]
fn test_call() {
    let mut test = Test::new();
    check! {
        Expr(Type::Void, Value::Unknown) == test.resolve_expr("@as(fn () void, undefined)()")
    }
}

#[test]
fn test_call_pointer() {
    let mut test = Test::new();
    check! {
        Expr(Type::Void, Value::Unknown) == test.resolve_expr("@as(*const fn () void, undefined)()")
    }
}

#[test]
fn test_call_method() {
    let mut test = Test::new();
    check! {
        Expr(Type::Void, Value::Unknown) == test.resolve_expr("@as(struct { fn foo(_: @This()) void {} }, undefined).foo()")
    }
    check! {
        Expr(Type::Void, Value::Unknown) == test.resolve_expr("@as(*const struct { fn foo(_: @This()) void {} }, undefined).foo()")
    }
}

#[test]
fn test_noreturn() {
    let mut test = Test::new();
    check! {
        Expr(Type::Noreturn, Value::Unknown) == test.resolve_expr("continue")
    }
    check! {
        Expr(Type::Noreturn, Value::Unknown) == test.resolve_expr("break")
    }
    check! {
        Expr(Type::Noreturn, Value::Unknown) == test.resolve_expr("return")
    }
    check! {
        Expr(Type::Noreturn, Value::Unknown) == test.resolve_expr("unreachable")
    }
}

// +-------------------------+
// |          Types          |
// +-------------------------+

#[test]
fn test_optional_type() {
    let mut test = Test::new();
    check! {
        let ty = test.resolve_type("?usize")
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && &InternedType::Optional(Type::Usize) == interned
    }
}

#[test]
fn test_error_set() {
    let mut test = Test::new();
    check! {
        let ty = test.resolve_type("error{ Foo, Bar }")
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::ErrorSet(names) = interned
        && names == &BTreeSet::from(["Foo".into(), "Bar".into()])
    }
}

#[test]
fn test_merge_error_sets() {
    let mut test = Test::new();
    check! {
        let ty = test.resolve_type("error{ Foo, Bar } || error{ Baz }")
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::ErrorSet(names) = interned
        && names == &BTreeSet::from(["Foo".into(), "Bar".into(), "Baz".into()])
    }
}

#[test]
fn test_error_union() {
    let mut test = Test::new();
    check! {
        let error_ty = test.resolve_type("error{Foo}")
        && let ty = test.resolve_type("error{Foo}!usize")
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && &InternedType::ErrorUnion(error_ty, Type::Usize) == interned
    }
}

#[test]
fn test_array_type() {
    let mut test = Test::new();
    check! {
        let ty = test.resolve_type("[3]usize")
        && test.format_type(ty) == "[?]usize"
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Array(array_type) = interned
        && array_type == &ArrayType {
            sentinel: None,
            elem: Type::Usize,
        }
    }
}

#[test]
fn test_ptr_type() {
    let mut test = Test::new();
    check! {
        let ty = test.resolve_type("*usize")
        && test.format_type(ty) == "*usize"
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Pointer(pointer_type) = interned
        && pointer_type == &PointerType::simple(PointerSize::One, Type::Usize)
    }
    check! {
        let ty = test.resolve_type("*usize")
        && let generic_ty = test.resolve_type("*addrspace(.generic) usize")
        && ty == generic_ty
    }
    check! {
        let ty = test.resolve_type("*allowzero align(8:10:5) addrspace(.flash) const volatile u9")
        && test.format_type(ty) == "*allowzero align(?:?:?) addrspace(?) const volatile u9"
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Pointer(pointer_type) = interned
        && pointer_type == &PointerType {
            size: PointerSize::One,
            sentinel: None,
            is_allowzero: true,
            has_align: true,
            has_bit_range_start: true,
            has_bit_range_end: true,
            has_addrspace: true,
            is_const: true,
            is_volatile: true,
            child: Type::Int(Signedness::Unsigned, 9),
        }
    }
    check! {
        let ty = test.resolve_type("[*:0]allowzero align(8) addrspace(.flash) const volatile u9")
        && test.format_type(ty) == "[*:0]allowzero align(?) addrspace(?) const volatile u9"
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Pointer(pointer_type) = interned
        && pointer_type == &PointerType {
            size: PointerSize::Many,
            sentinel: Some(Value::Int(0)),
            is_allowzero: true,
            has_align: true,
            has_bit_range_start: false,
            has_bit_range_end: false,
            has_addrspace: true,
            is_const: true,
            is_volatile: true,
            child: Type::Int(Signedness::Unsigned, 9),
        }
    }
}

#[test]
fn test_fn_proto() {
    let mut test = Test::new();
    check! {
        let ty = test.resolve_type("fn (usize) bool")
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Function(function_type) = interned
        && function_type == &FunctionType {
            params: vec![Type::Usize],
            has_callconv: false,
            return_type: Type::Bool
        }
    }
}

#[test]
fn test_container() {
    let mut test = Test::new();
    check! {
        let ty = test.resolve_type("struct {}")
        && test.format_type(ty) == "struct {}"
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && let InternedType::Container(_) = interned
    }
}

#[test]
fn test_container_tuple() {
    let mut test = Test::new();
    check! {
        let ty = test.resolve_type("struct { usize, bool }")
        && let Type::Interned(index) = ty
        && let Some(interned) = test.ip.get_type(index)
        && &InternedType::Tuple(vec![Type::Usize, Type::Bool]) == interned
    }
}

// +------------------------------+
// |          Peer Types          |
// +------------------------------+

#[test]
fn test_peer_types_error_set() {
    let mut test = Test::new();

    let error_foo = test.intern_type(InternedType::ErrorSet(["Foo".into()].into()));
    let error_bar = test.intern_type(InternedType::ErrorSet(["Bar".into()].into()));

    let error_bar_foo =
        test.intern_type(InternedType::ErrorSet(["Bar".into(), "Foo".into()].into()));

    check! {
        test.resolve_peer_types(&[error_foo, error_bar]) == error_bar_foo
    }
}

#[test]
fn test_peer_types_error_union() {
    let mut test = Test::new();

    let error_foo = test.intern_type(InternedType::ErrorSet(["Foo".into()].into()));
    let error_bar = test.intern_type(InternedType::ErrorSet(["Bar".into()].into()));

    let error_bar_foo =
        test.intern_type(InternedType::ErrorSet(["Bar".into(), "Foo".into()].into()));

    let error_foo_union_bool = test.intern_type(InternedType::ErrorUnion(error_foo, Type::Bool));
    let error_bar_union_bool = test.intern_type(InternedType::ErrorUnion(error_bar, Type::Bool));

    let error_bar_foo_union_bool =
        test.intern_type(InternedType::ErrorUnion(error_bar_foo, Type::Bool));

    let undefined_union_bool =
        test.intern_type(InternedType::ErrorUnion(Type::Undefined, Type::Bool));

    let unknown_union_bool = test.intern_type(InternedType::ErrorUnion(Type::Unknown, Type::Bool));

    check! {
        test.resolve_peer_types(&[error_foo, Type::Bool]) == error_foo_union_bool
    }
    check! {
        test.resolve_peer_types(&[error_foo, error_bar_union_bool]) == error_bar_foo_union_bool
    }
    check! {
        test.resolve_peer_types(&[error_foo, undefined_union_bool]) == unknown_union_bool
    }
}

#[test]
fn test_peer_types_nullable() {
    let mut test = Test::new();
    check! {
        test.resolve_peer_types(&[Type::Noreturn, Type::Null]) == Type::Null
    }
}

#[test]
fn test_peer_types_optional() {
    let mut test = Test::new();
    let optional_bool = test.intern_type(InternedType::Optional(Type::Bool));
    check! {
        test.resolve_peer_types(&[Type::Bool, optional_bool]) == optional_bool
    }
}

#[test]
fn test_peer_types_array() {
    let mut test = Test::new();

    let tuple_i16_i16 = test.intern_type(InternedType::Tuple(vec![
        Type::Int(Signedness::Signed, 16),
        Type::Int(Signedness::Signed, 16),
    ]));

    let array_i16 = test.intern_type(InternedType::Array(ArrayType {
        sentinel: None,
        elem: Type::Int(Signedness::Signed, 16),
    }));

    let array_sentinel_0_i16 = test.intern_type(InternedType::Array(ArrayType {
        sentinel: Some(Value::Int(0)),
        elem: Type::Int(Signedness::Signed, 16),
    }));

    let array_sentinel_1_i16 = test.intern_type(InternedType::Array(ArrayType {
        sentinel: Some(Value::Int(1)),
        elem: Type::Int(Signedness::Signed, 16),
    }));

    check! {
        test.resolve_peer_types(&[tuple_i16_i16, array_i16]) == array_i16
    }
    check! {
        test.resolve_peer_types(&[tuple_i16_i16, array_sentinel_0_i16]) == array_i16
    }
    check! {
        test.resolve_peer_types(&[array_sentinel_0_i16, array_i16]) == array_i16
    }
    check! {
        test.resolve_peer_types(&[array_sentinel_0_i16, array_sentinel_1_i16]) == array_i16
    }
}

#[test]
fn test_peer_types_vector() {
    let mut test = Test::new();

    let vector_i16 = test.intern_type(InternedType::Vector(Type::Int(Signedness::Signed, 16)));

    let array_i16 = test.intern_type(InternedType::Array(ArrayType {
        sentinel: None,
        elem: Type::Int(Signedness::Signed, 16),
    }));

    let array_sentinel_0_i16 = test.intern_type(InternedType::Array(ArrayType {
        sentinel: Some(Value::Int(0)),
        elem: Type::Int(Signedness::Signed, 16),
    }));

    let tuple_i16_i16 = test.intern_type(InternedType::Tuple(vec![
        Type::Int(Signedness::Signed, 16),
        Type::Int(Signedness::Signed, 16),
    ]));

    check! {
        test.resolve_peer_types(&[array_i16, vector_i16]) == vector_i16
    }
    check! {
        test.resolve_peer_types(&[array_sentinel_0_i16, vector_i16]) == vector_i16
    }
    check! {
        test.resolve_peer_types(&[tuple_i16_i16, vector_i16]) == vector_i16
    }
}

#[test]
#[ignore]
fn test_peer_types_c_ptr() {
    let mut test = Test::new();
    check! {
        test.resolve_peer_types(&[]) == Type::Unknown
    }
}

#[test]
#[ignore]
fn test_peer_types_ptr() {
    let mut test = Test::new();
    check! {
        test.resolve_peer_types(&[]) == Type::Unknown
    }
}

#[test]
#[ignore]
fn test_peer_types_func() {
    let mut test = Test::new();
    check! {
        test.resolve_peer_types(&[]) == Type::Unknown
    }
}

#[test]
fn test_peer_types_enum_or_union() {
    let mut test = Test::new();

    let enum_foo = test.resolve_type("enum { foo }");
    let enum_bar = test.resolve_type("enum { bar }");

    let union_foo = test.resolve_type("union { foo: f32 }");
    let union_bar = test.resolve_type("union { bar: bool }");

    let (inferred_tagged_union, inferred_tagged_union_tag) = {
        let node = test.parse_expression("union(enum) { foo: f32 }", None);
        let ty = test.analyzer().resolve_type(node);
        let tag = test.analyzer().resolve_union_tag(ty).unwrap();
        check! {
            let Type::UnionTag(_) = tag
        }
        (ty, tag)
    };

    let (explicit_tagged_union, explicit_tagged_union_tag) = {
        let node = test.parse_expression("union( (enum { foo }) ) { foo: f32 }", None);
        let ty = test.analyzer().resolve_type(node);
        let tag = test.analyzer().resolve_union_tag(ty).unwrap();
        check! {
            test.format_type(tag) == "enum { foo }"
        }
        (ty, tag)
    };

    // Enum Literal + Other
    check! {
        test.resolve_peer_types(&[Type::EnumLiteral, Type::Noreturn]) == Type::EnumLiteral
    }
    check! {
        test.resolve_peer_types(&[Type::EnumLiteral, enum_foo]) == enum_foo
    }
    check! {
        test.resolve_peer_types(&[Type::EnumLiteral, union_foo]) == union_foo
    }

    // Enum + Other
    check! {
        test.resolve_peer_types(&[enum_foo, Type::EnumLiteral]) == enum_foo
    }
    check! {
        test.resolve_peer_types(&[enum_foo, enum_bar]) == Type::Unknown
    }
    check! {
        test.resolve_peer_types(&[enum_foo, union_foo]) == Type::Unknown
    }
    check! {
        test.resolve_peer_types(&[
            inferred_tagged_union_tag,
            inferred_tagged_union,
        ]) == inferred_tagged_union
    }
    check! {
        test.resolve_peer_types(&[
            explicit_tagged_union_tag,
            explicit_tagged_union,
        ]) == explicit_tagged_union
    }

    // Union + Other
    check! {
        test.resolve_peer_types(&[union_foo, Type::EnumLiteral]) == union_foo
    }
    check! {
        test.resolve_peer_types(&[union_foo, enum_foo]) == Type::Unknown
    }
    check! {
        test.resolve_peer_types(&[union_foo, union_bar]) == Type::Unknown
    }
    check! {
        test.resolve_peer_types(&[
            inferred_tagged_union,
            inferred_tagged_union_tag,
        ]) == inferred_tagged_union
    }
    check! {
        test.resolve_peer_types(&[
            explicit_tagged_union,
            explicit_tagged_union_tag,
        ]) == explicit_tagged_union
    }
}

#[test]
fn test_peer_types_comptime_int() {
    let mut test = Test::new();
    check! {
        test.resolve_peer_types(&[Type::Noreturn, Type::ComptimeInt]) == Type::ComptimeInt
    }
}

#[test]
fn test_peer_types_comptime_float() {
    let mut test = Test::new();
    check! {
        test.resolve_peer_types(&[Type::ComptimeInt, Type::ComptimeFloat]) == Type::ComptimeFloat
    }
}

#[test]
fn test_peer_types_fixed_int() {
    let mut test = Test::new();
    check! {
        test.resolve_peer_types(&[
            Type::ComptimeInt,
            Type::Int(Signedness::Unsigned, 32),
        ]) == Type::Int(Signedness::Unsigned, 32)
    }
    check! {
        test.resolve_peer_types(&[
            Type::Int(Signedness::Unsigned, 16),
            Type::Int(Signedness::Unsigned, 32),
        ]) == Type::Int(Signedness::Unsigned, 32)
    }
    check! {
        test.resolve_peer_types(&[
            Type::Int(Signedness::Signed, 16),
            Type::Int(Signedness::Signed, 32),
        ]) == Type::Int(Signedness::Signed, 32)
    }
    check! {
        test.resolve_peer_types(&[
            Type::Int(Signedness::Unsigned, 16),
            Type::Int(Signedness::Signed, 17),
        ]) == Type::Int(Signedness::Signed, 17)
    }
    check! {
        test.resolve_peer_types(&[
            Type::Int(Signedness::Unsigned, 17),
            Type::Int(Signedness::Signed, 16),
        ]) == Type::Unknown
    }
    check! {
        test.resolve_peer_types(&[
            Type::Int(Signedness::Unsigned, 16),
            Type::Int(Signedness::Signed, 16),
        ]) == Type::Unknown
    }
}

#[test]
fn test_peer_types_fixed_float() {
    let mut test = Test::new();
    check! {
        test.resolve_peer_types(&[
            Type::ComptimeFloat,
            Type::Float(16),
        ]) == Type::Float(16)
    }
    check! {
        test.resolve_peer_types(&[
            Type::ComptimeInt,
            Type::Float(16),
        ]) == Type::Float(16)
    }
    check! {
        test.resolve_peer_types(&[
            Type::Int(Signedness::Unsigned, 16),
            Type::Float(16),
        ]) == Type::Float(16)
    }
    check! {
        test.resolve_peer_types(&[
            Type::Float(16),
            Type::Float(32),
        ]) == Type::Float(32)
    }
}

#[test]
fn test_peer_types_tuple() {
    let mut test = Test::new();

    let tuple_i16_u32 = test.intern_type(InternedType::Tuple(vec![
        Type::Int(Signedness::Signed, 16),
        Type::Int(Signedness::Unsigned, 32),
    ]));

    let tuple_i32_u16 = test.intern_type(InternedType::Tuple(vec![
        Type::Int(Signedness::Signed, 32),
        Type::Int(Signedness::Unsigned, 16),
    ]));

    let tuple_i32_u32 = test.intern_type(InternedType::Tuple(vec![
        Type::Int(Signedness::Signed, 32),
        Type::Int(Signedness::Unsigned, 32),
    ]));

    check! {
        test.resolve_peer_types(&[tuple_i16_u32, tuple_i32_u16]) == tuple_i32_u32
    }
}

#[test]
fn test_peer_types_exact() {
    let mut test = Test::new();
    let struct_foo = test.resolve_type("struct { foo: f32 }");
    let struct_bar = test.resolve_type("struct { bar: bool }");
    check! {
        test.resolve_peer_types(&[struct_foo, struct_bar]) == Type::Unknown
    }
}
