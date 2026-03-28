use std::cell::LazyCell;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::fmt::{Debug, Formatter};
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::rc::Rc;
use std::str::FromStr;

use ordermap::{OrderMap, OrderSet};
use zig_ast::*;

mod display;
mod document;

pub use document::*;

#[derive(Clone)]
pub struct Handle(pub Rc<Path>, pub Rc<Ast>);

impl Handle {
    pub fn path(&self) -> &Rc<Path> {
        &self.0
    }

    pub fn tree(&self) -> &Rc<Ast> {
        &self.1
    }
}

impl Debug for Handle {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Handle").field(&self.path()).finish()
    }
}

impl PartialEq for Handle {
    fn eq(&self, other: &Self) -> bool {
        self.path() == other.path()
    }
}

impl Eq for Handle {}

impl Hash for Handle {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.path().hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Node(pub Handle, pub NodeIndex);

impl Node {
    pub fn from(handle: &Handle, index: NodeIndex) -> Self {
        Self(handle.clone(), index)
    }

    pub fn handle(&self) -> &Handle {
        &self.0
    }

    pub fn index(&self) -> NodeIndex {
        self.1
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Binding {
    Unknown,
    Constant(Expr),
    Variable(Expr),
}

impl Binding {
    pub fn new(is_const: bool, expr: Expr) -> Self {
        if is_const {
            return Self::Constant(expr);
        }
        Self::Variable(expr)
    }

    pub fn is_unknown(self) -> bool {
        self == Self::Unknown
    }

    pub fn is_constant(self) -> bool {
        matches!(self, Self::Constant(_))
    }

    pub fn is_variable(self) -> bool {
        matches!(self, Self::Variable(_))
    }

    pub fn map<F: FnOnce(Expr) -> Expr>(self, f: F) -> Self {
        match self {
            Self::Unknown => Self::Unknown,
            Self::Constant(expr) => Self::Constant(f(expr)),
            Self::Variable(expr) => Self::Variable(f(expr)),
        }
    }

    pub fn and_then<F: FnOnce(bool, Expr) -> Binding>(self, f: F) -> Self {
        match self {
            Self::Unknown => return Self::Unknown,
            Self::Constant(expr) => f(true, expr),
            Self::Variable(expr) => f(false, expr),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Expr(pub Type, pub Value);

impl Expr {
    pub fn type_of(self) -> Type {
        self.0
    }

    pub fn value(self) -> Value {
        self.1
    }
}

impl From<Type> for Expr {
    fn from(value: Type) -> Self {
        Self(Type::Type, Value::Type(value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    Unknown,
    Type,

    // TODO: add Frame + Anyframe ?
    Anyopaque,
    Bool,
    ComptimeFloat,
    ComptimeInt,
    Noreturn,
    Null,
    Undefined,
    Void,

    Float(u16),
    Int(Signedness, u16),
    Isize,
    Usize,

    EnumLiteral,
    UnionTag(u32),

    Interned(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Signedness {
    Signed,
    Unsigned,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InternedType {
    Optional(Type),
    ErrorSet(BTreeSet<Vec<u8>>),
    ErrorUnion(Type, Type),
    Vector(Type),
    Array(ArrayType),
    Pointer(PointerType),
    Function(FunctionType),
    Tuple(Vec<Type>),
    Container(ContainerType),
    Branched(BTreeSet<TypeOrd>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ArrayType {
    pub sentinel: Option<Value>,
    pub elem: Type,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointerType {
    pub size: PointerSize,
    pub sentinel: Option<Value>,
    pub is_allowzero: bool,
    pub has_align: bool,
    pub has_bit_range_start: bool,
    pub has_bit_range_end: bool,
    pub has_addrspace: bool,
    pub is_const: bool,
    pub is_volatile: bool,
    pub child: Type,
}

impl PointerType {
    pub fn simple(size: PointerSize, child: Type) -> Self {
        Self {
            size,
            sentinel: None,
            is_allowzero: false,
            has_align: false,
            has_bit_range_start: false,
            has_bit_range_end: false,
            has_addrspace: false,
            is_const: false,
            is_volatile: false,
            child,
        }
    }

    pub fn simple_const(size: PointerSize, child: Type) -> Self {
        Self {
            size,
            sentinel: None,
            is_allowzero: false,
            has_align: false,
            has_bit_range_start: false,
            has_bit_range_end: false,
            has_addrspace: false,
            is_const: true,
            is_volatile: false,
            child,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FunctionType {
    pub params: Vec<Type>,
    pub has_callconv: bool,
    pub return_type: Type,
}

#[derive(Clone, Debug)]
pub struct ContainerType {
    this: Node,
    scope: Option<Rc<Scope>>,
}

impl ContainerType {
    pub fn new(this: Node, documents: &mut DocumentStore) -> Self {
        let path = this.handle().path().clone();
        let scope = documents
            .get_or_parse(path)
            .and_then(|doc| doc.get(this.index()));
        ContainerType { this, scope }
    }

    pub fn this(&self) -> &Node {
        &self.this
    }

    pub fn scope(&self) -> Option<&Rc<Scope>> {
        self.scope.as_ref()
    }

    pub fn source(&self) -> &[u8] {
        let node = self.this();
        let tree = node.handle().tree();
        tree.node_source(node.index())
    }
}

impl PartialEq for ContainerType {
    fn eq(&self, other: &Self) -> bool {
        self.this == other.this
    }
}

impl Eq for ContainerType {}

impl Hash for ContainerType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.this.hash(state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeOrd(pub Type);

impl TypeOrd {
    pub fn ordinal(self) -> u8 {
        #[cfg_attr(rustfmt, rustfmt::skip)]
        match self.0 {
            Type::Unknown       => 0,
            Type::Type          => 1,

            Type::Anyopaque     => 2,
            Type::Bool          => 3,
            Type::ComptimeFloat => 4,
            Type::ComptimeInt   => 5,
            Type::Noreturn      => 6,
            Type::Null          => 7,
            Type::Undefined     => 8,
            Type::Void          => 9,

            Type::Float(_)      => 10,
            Type::Int(_, _)     => 11,
            Type::Isize         => 12,
            Type::Usize         => 13,

            Type::EnumLiteral   => 14,
            Type::UnionTag(_)   => 15,

            Type::Interned(_)   => u8::MAX,
        }
    }
}

impl Ord for TypeOrd {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.ordinal().cmp(&other.ordinal()) {
            Ordering::Equal => match (self.0, other.0) {
                (Type::Float(a_bits), Type::Float(b_bits)) => a_bits.cmp(&b_bits),
                (Type::Int(a_sign, a_bits), Type::Int(b_sign, b_bits)) => {
                    match (a_sign as u8).cmp(&(b_sign as u8)) {
                        Ordering::Equal => a_bits.cmp(&b_bits),
                        ordering => ordering,
                    }
                }
                (Type::Interned(a_index), Type::Interned(b_index)) => a_index.cmp(&b_index),
                _ => Ordering::Equal,
            },
            ordering => ordering,
        }
    }
}

impl PartialOrd for TypeOrd {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl From<Type> for TypeOrd {
    fn from(value: Type) -> Self {
        Self(value)
    }
}

impl From<TypeOrd> for Type {
    fn from(value: TypeOrd) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Value {
    Unknown,
    Runtime,
    Undefined,
    Void,
    Null,
    False,
    True,
    Int(i32),
    Interned(u32),
    Type(Type),
}

impl Value {
    pub fn is_unknown(self) -> bool {
        self == Self::Unknown
    }

    pub fn is_runtime(self) -> bool {
        self == Self::Runtime
    }

    pub fn to_unknown(self) -> Self {
        match self {
            Self::Runtime => Self::Runtime,
            _ => Self::Unknown,
        }
    }
}

impl From<Type> for Value {
    fn from(value: Type) -> Self {
        Self::Type(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InternedValue {
    EnumLiteral(Vec<u8>),
    ErrorValue(Vec<u8>),
}

pub struct InternPool {
    types: OrderSet<InternedType>,
    values: OrderSet<InternedValue>,
}

impl InternPool {
    pub fn new() -> Self {
        Self {
            types: OrderSet::new(),
            values: OrderSet::new(),
        }
    }

    pub fn intern_type(&mut self, interned: InternedType) -> u32 {
        if let Some(index) = self.types.get_index_of(&interned) {
            return index as u32;
        }
        let index = self.types.len().try_into().expect("intern pool full");
        self.types.insert(interned);
        index
    }

    pub fn intern_value(&mut self, interned: InternedValue) -> u32 {
        if let Some(index) = self.values.get_index_of(&interned) {
            return index as u32;
        }
        let index = self.values.len().try_into().expect("intern pool full");
        self.values.insert(interned);
        index
    }

    pub fn get_type(&self, index: u32) -> Option<&InternedType> {
        self.types.get_index(index as usize)
    }

    pub fn get_value(&self, index: u32) -> Option<&InternedValue> {
        self.values.get_index(index as usize)
    }
}

fn parse_str<F: FromStr>(bytes: &[u8]) -> Option<F> {
    let Ok(str) = str::from_utf8(bytes) else {
        return None;
    };
    let Ok(index) = str.parse() else {
        return None;
    };
    Some(index)
}

const PRIMITIVES: LazyCell<HashMap<&'static [u8], Expr>> = LazyCell::new(|| {
    let mut map = HashMap::<&'static [u8], Expr>::new();
    map.insert(b"anyopaque", Expr::from(Type::Anyopaque));
    map.insert(b"bool", Expr::from(Type::Bool));
    map.insert(b"comptime_int", Expr::from(Type::ComptimeInt));
    map.insert(b"comptime_float", Expr::from(Type::ComptimeFloat));
    map.insert(b"f128", Expr::from(Type::Float(128)));
    map.insert(b"f16", Expr::from(Type::Float(16)));
    map.insert(b"f32", Expr::from(Type::Float(32)));
    map.insert(b"f64", Expr::from(Type::Float(64)));
    map.insert(b"f80", Expr::from(Type::Float(80)));
    map.insert(b"false", Expr(Type::Bool, Value::False));
    map.insert(b"isize", Expr::from(Type::Isize));
    map.insert(b"noreturn", Expr::from(Type::Noreturn));
    map.insert(b"null", Expr(Type::Null, Value::Null));
    map.insert(b"true", Expr(Type::Bool, Value::True));
    map.insert(b"type", Expr::from(Type::Type));
    map.insert(b"undefined", Expr(Type::Undefined, Value::Undefined));
    map.insert(b"usize", Expr::from(Type::Usize));
    map.insert(b"void", Expr::from(Type::Void));
    map
});

pub struct Analyzer<'ip, 'cache, 'doc> {
    ip: &'ip mut InternPool,
    cache: &'cache mut OrderMap<Node, Binding>,
    documents: &'doc mut DocumentStore,
    this: Node,
}

impl<'ip, 'cache, 'doc> Analyzer<'ip, 'cache, 'doc> {
    pub fn new(
        ip: &'ip mut InternPool,
        cache: &'cache mut OrderMap<Node, Binding>,
        documents: &'doc mut DocumentStore,
        this: Node,
    ) -> Self {
        Self {
            ip,
            cache,
            documents,
            this,
        }
    }

    pub fn this(&self) -> &Node {
        &self.this
    }

    pub fn resolve_type(&mut self, node: Node) -> Type {
        match self.resolve_expr(node) {
            Expr(Type::Type, Value::Type(ty)) => ty,
            _ => Type::Unknown,
        }
    }

    pub fn resolve_expr(&mut self, node: Node) -> Expr {
        match self.resolve_binding(node) {
            Binding::Unknown => Expr(Type::Unknown, Value::Unknown),
            Binding::Constant(expr) => expr,
            Binding::Variable(expr) => expr,
        }
    }

    // +----------------------------+
    // |          Bindings          |
    // +----------------------------+

    pub fn resolve_binding(&mut self, node: Node) -> Binding {
        if let Some(&binding) = self.cache.get(&node) {
            return binding;
        }
        self.cache.insert(node.clone(), Binding::Unknown);
        let binding = self.resolve_binding_uncached(&node);
        self.cache.insert(node, binding);
        binding
    }

    fn resolve_binding_uncached(&mut self, node: &Node) -> Binding {
        let tree = node.handle().tree();
        match tree.node_tag(node.index()) {
            NodeTag::Identifier => self.resolve_identifier(node),
            NodeTag::UnwrapOptional => self.resolve_unwrap_optional(node),
            NodeTag::Orelse => self.resolve_orelse(node),
            NodeTag::Try => self.resolve_try(node),
            NodeTag::Catch => self.resolve_catch(node),
            NodeTag::Deref => self.resolve_deref(node),
            NodeTag::ArrayAccess => self.resolve_array_access(node),
            NodeTag::SliceOpen | NodeTag::Slice | NodeTag::SliceSentinel => {
                self.resolve_slice(node)
            }
            NodeTag::FieldAccess => self.resolve_member_access(node),
            _ => Binding::Constant(self.resolve_expr_uncached(node)),
        }
    }

    fn resolve_identifier(&mut self, node: &Node) -> Binding {
        let handle = node.handle();
        let tree = handle.tree();
        let bytes = tree.node_source(node.index());

        if let Some(&expr) = PRIMITIVES.get(bytes) {
            return Binding::Constant(expr);
        }

        if let [head @ (b'i' | b'u'), tail @ ..] = bytes
            && let Some(bits) = parse_str(tail)
        {
            let signedness = match head {
                b'i' => Signedness::Signed,
                b'u' => Signedness::Unsigned,
                _ => unreachable!(),
            };
            return Binding::Constant(Expr::from(Type::Int(signedness, bits)));
        }

        let Some(document) = self.documents.get(handle.path()) else {
            return Binding::Unknown;
        };
        assert!(Rc::ptr_eq(document.tree(), handle.tree()));
        let mut decl_info = None;
        let token = tree.node_main_token(node.index());
        for (_, scope) in document.enclosing_scopes(token) {
            let Some(&decl) = scope.decls.get(bytes) else {
                continue;
            };
            let decl_token = tree.node_main_token(decl.node_index());
            let Some((this_index, _)) = document.enclosing_container(decl_token) else {
                return Binding::Unknown;
            };
            let this = Node::from(handle, this_index);
            decl_info = Some((this, decl));
            break;
        }
        let Some((this, decl)) = decl_info else {
            return Binding::Unknown;
        };
        let mut analyzer = Analyzer::new(self.ip, self.cache, self.documents, this);
        analyzer.resolve_decl_access_this(decl)
    }

    fn resolve_unwrap_optional(&mut self, node: &Node) -> Binding {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndToken(lhs, _) = unsafe { tree.node_data_unchecked(node.index()) };
        let binding = self.resolve_binding(Node::from(handle, lhs));
        binding.map(|expr| {
            let Expr(ty, val) = expr;
            let Type::Interned(index) = ty else {
                return Expr(Type::Unknown, val.to_unknown());
            };
            let Some(&InternedType::Optional(child)) = self.ip.get_type(index) else {
                return Expr(Type::Unknown, val.to_unknown());
            };
            Expr(child, val)
        })
    }

    fn resolve_orelse(&mut self, node: &Node) -> Binding {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndNode(lhs, rhs) = unsafe { tree.node_data_unchecked(node.index()) };
        let lhs_binding = self.resolve_binding(Node::from(handle, lhs)).map(|expr| {
            let Expr(ty, val) = expr;
            let Type::Interned(index) = ty else {
                return Expr(Type::Unknown, val.to_unknown());
            };
            let Some(&InternedType::Optional(child)) = self.ip.get_type(index) else {
                return Expr(Type::Unknown, val.to_unknown());
            };
            Expr(child, val)
        });
        let rhs_binding = self.resolve_binding(Node::from(handle, rhs));
        match (lhs_binding, rhs_binding) {
            (Binding::Variable(lhs_expr), Binding::Variable(rhs_expr)) => {
                let expr = self.resolve_branching_expressions(&[lhs_expr, rhs_expr]);
                Binding::Variable(expr)
            }
            (Binding::Constant(lhs_expr), Binding::Constant(rhs_expr))
            | (Binding::Constant(lhs_expr), Binding::Variable(rhs_expr))
            | (Binding::Variable(lhs_expr), Binding::Constant(rhs_expr)) => {
                let expr = self.resolve_branching_expressions(&[lhs_expr, rhs_expr]);
                Binding::Constant(expr)
            }
            (Binding::Unknown, _) | (_, Binding::Unknown) => Binding::Unknown,
        }
    }

    fn resolve_try(&mut self, node: &Node) -> Binding {
        let handle = node.handle();
        let tree = handle.tree();
        let expr_index: NodeIndex = unsafe { tree.node_data_unchecked(node.index()) };
        let binding = self.resolve_binding(Node::from(handle, expr_index));
        binding.map(|expr| {
            let Expr(ty, val) = expr;
            let Type::Interned(index) = ty else {
                return Expr(Type::Unknown, val.to_unknown());
            };
            let Some(&InternedType::ErrorUnion(_, ty)) = self.ip.get_type(index) else {
                return Expr(Type::Unknown, val.to_unknown());
            };
            Expr(ty, val)
        })
    }

    fn resolve_catch(&mut self, node: &Node) -> Binding {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndNode(lhs, rhs) = unsafe { tree.node_data_unchecked(node.index()) };
        let lhs_binding = self.resolve_binding(Node::from(handle, lhs)).map(|expr| {
            let Expr(ty, val) = expr;
            let Type::Interned(index) = ty else {
                return Expr(Type::Unknown, val.to_unknown());
            };
            let Some(&InternedType::ErrorUnion(_, ty)) = self.ip.get_type(index) else {
                return Expr(Type::Unknown, val.to_unknown());
            };
            Expr(ty, val)
        });
        let rhs_binding = self.resolve_binding(Node::from(handle, rhs));
        match (lhs_binding, rhs_binding) {
            (Binding::Variable(lhs_expr), Binding::Variable(rhs_expr)) => {
                let expr = self.resolve_branching_expressions(&[lhs_expr, rhs_expr]);
                Binding::Variable(expr)
            }
            (Binding::Constant(lhs_expr), Binding::Constant(rhs_expr))
            | (Binding::Constant(lhs_expr), Binding::Variable(rhs_expr))
            | (Binding::Variable(lhs_expr), Binding::Constant(rhs_expr)) => {
                let expr = self.resolve_branching_expressions(&[lhs_expr, rhs_expr]);
                Binding::Constant(expr)
            }
            (Binding::Unknown, _) | (_, Binding::Unknown) => Binding::Unknown,
        }
    }

    fn resolve_deref(&mut self, node: &Node) -> Binding {
        let handle = node.handle();
        let tree = handle.tree();
        let expr_index: NodeIndex = unsafe { tree.node_data_unchecked(node.index()) };
        let Expr(ty, val) = self.resolve_expr(Node::from(handle, expr_index));
        let Type::Interned(index) = ty else {
            return Binding::Unknown;
        };
        let Some(&InternedType::Pointer(pointer_type)) = self.ip.get_type(index) else {
            return Binding::Unknown;
        };
        match pointer_type.size {
            PointerSize::One | PointerSize::C => {}
            PointerSize::Many | PointerSize::Slice => return Binding::Unknown,
        };
        Binding::new(
            pointer_type.is_const,
            Expr(pointer_type.child, val.to_unknown()),
        )
    }

    fn resolve_array_access(&mut self, node: &Node) -> Binding {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndNode(lhs, _) = unsafe { tree.node_data_unchecked(node.index()) };
        let binding = self.resolve_binding(Node::from(handle, lhs));
        binding.and_then(|is_const, expr| {
            let Expr(ty, val) = expr;
            let Type::Interned(index) = ty else {
                return Binding::Unknown;
            };
            let Some(interned) = self.ip.get_type(index) else {
                return Binding::Unknown;
            };
            let expr = match interned {
                &InternedType::Vector(child) => Expr(child, val.to_unknown()),
                InternedType::Array(array_type) => Expr(array_type.elem, val.to_unknown()),
                InternedType::Pointer(pointer_type) => {
                    let is_const = pointer_type.is_const;
                    if pointer_type.size == PointerSize::Slice {
                        let expr = Expr(pointer_type.child, val.to_unknown());
                        return Binding::new(is_const, expr);
                    }
                    if pointer_type.size != PointerSize::One {
                        return Binding::Unknown;
                    }
                    let Type::Interned(index) = pointer_type.child else {
                        return Binding::Unknown;
                    };
                    let Some(interned) = self.ip.get_type(index) else {
                        return Binding::Unknown;
                    };
                    let ty = match interned {
                        &InternedType::Vector(child) => child,
                        InternedType::Array(array_type) => array_type.elem,
                        _ => return Binding::Unknown,
                    };
                    return Binding::new(is_const, Expr(ty, val.to_unknown()));
                }
                _ => return Binding::Unknown,
            };
            Binding::new(is_const, expr)
        })
    }

    fn resolve_slice(&mut self, node: &Node) -> Binding {
        let _ = node;
        Binding::Unknown // todo!()
    }

    fn resolve_member_access(&mut self, node: &Node) -> Binding {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndToken(lhs, token) = unsafe { tree.node_data_unchecked(node.index()) };
        let binding = self.resolve_binding(Node::from(handle, lhs));
        binding.and_then(|is_const, expr| {
            let expr = match expr {
                Expr(Type::Type, Value::Type(ty)) => {
                    return self.resolve_decl_access(ty, tree.token_slice(token));
                }
                _ => self.resolve_field_access(expr, tree.token_slice(token)),
            };
            Binding::new(is_const, expr)
        })
    }

    fn resolve_decl_access(&mut self, ty: Type, name: &[u8]) -> Binding {
        let Type::Interned(index) = ty else {
            return Binding::Unknown;
        };
        let Some(interned) = self.ip.get_type(index) else {
            return Binding::Unknown;
        };
        let InternedType::Container(container_type) = interned else {
            return Binding::Unknown;
        };
        let Some(scope) = container_type.scope() else {
            return Binding::Unknown;
        };
        let Some(&decl) = scope.decls.get(name) else {
            return Binding::Unknown;
        };
        let this = container_type.this().clone();
        let mut analyzer = Analyzer::new(self.ip, self.cache, self.documents, this);
        analyzer.resolve_decl_access_this(decl)
    }

    fn resolve_decl_access_this(&mut self, decl: Declaration) -> Binding {
        match decl {
            Declaration::Variable(variable) => self.resolve_decl_access_variable(variable),
            Declaration::Function(function) => {
                Binding::Constant(self.resolve_decl_access_function(function))
            }
        }
    }

    fn resolve_decl_access_variable(&mut self, variable: NodeIndex) -> Binding {
        let handle = self.this.handle().clone();
        let tree = handle.tree();
        let var_decl: full::VarDecl = tree.full_node(variable).unwrap();
        let is_const = tree.token_tag(var_decl.ast.mut_token) == TokenTag::KeywordConst;
        let expr = match (
            var_decl.ast.type_node.to_option(),
            var_decl.ast.init_node.to_option(),
        ) {
            (Some(ty_index), init) => {
                let ty = self.resolve_type(Node(handle.clone(), ty_index));
                match (is_const, init) {
                    (true, Some(init_index)) => {
                        let expr = self.resolve_expr(Node(handle, init_index));
                        Expr(ty, expr.value())
                    }
                    (true, None) => Expr(ty, Value::Unknown),
                    (false, _) => Expr(ty, Value::Runtime),
                }
            }
            (None, Some(init_index)) => {
                let Expr(ty, val) = self.resolve_expr(Node(handle, init_index));
                match is_const {
                    true => Expr(ty, val),
                    false => Expr(ty, Value::Runtime),
                }
            }
            (None, None) => match is_const {
                true => Expr(Type::Unknown, Value::Unknown),
                false => Expr(Type::Unknown, Value::Runtime),
            },
        };
        Binding::new(is_const, expr)
    }

    fn resolve_decl_access_function(&mut self, function: NodeIndex) -> Expr {
        let handle = self.this.handle().clone();
        let node = Node(handle, function);
        let ty = self.resolve_fn_proto(&node);
        Expr(ty, Value::Unknown)
    }

    fn resolve_field_access(&mut self, expr: Expr, name: &[u8]) -> Expr {
        let Expr(ty, val) = expr;
        let Type::Interned(index) = ty else {
            return Expr(Type::Unknown, val.to_unknown());
        };
        let Some(mut interned) = self.ip.get_type(index) else {
            return Expr(Type::Unknown, val.to_unknown());
        };
        if let InternedType::Pointer(pointer_type) = interned
            && pointer_type.size == PointerSize::One
        {
            let Type::Interned(index) = pointer_type.child else {
                return Expr(Type::Unknown, val.to_unknown());
            };
            interned = match self.ip.get_type(index) {
                Some(interned) => interned,
                None => return Expr(Type::Unknown, val.to_unknown()),
            };
        }
        match interned {
            InternedType::Array(_) => match name {
                b"len" => Expr(Type::Usize, val.to_unknown()),
                _ => Expr(Type::Unknown, val.to_unknown()),
            },
            InternedType::Pointer(pointer_type) if pointer_type.size == PointerSize::Slice => {
                match name {
                    b"len" => Expr(Type::Usize, val.to_unknown()),
                    b"ptr" => {
                        let mut pointer_type = pointer_type.clone();
                        pointer_type.size = PointerSize::Many;
                        let interned = InternedType::Pointer(pointer_type);
                        let index = self.ip.intern_type(interned);
                        Expr(Type::Interned(index), val.to_unknown())
                    }
                    _ => Expr(Type::Unknown, val.to_unknown()),
                }
            }
            InternedType::Tuple(types) => match name {
                b"len" => match types.len().try_into() {
                    Ok(len) => Expr(Type::Usize, Value::Int(len)),
                    Err(_) => Expr(Type::Usize, val.to_unknown()),
                },
                [b'@', b'"', slice @ .., b'"'] => match parse_str::<usize>(slice) {
                    Some(index) => Expr(types[index], val.to_unknown()),
                    None => Expr(Type::Unknown, val.to_unknown()),
                },
                _ => Expr(Type::Unknown, val.to_unknown()),
            },
            InternedType::Container(container_type) => {
                let Some(scope) = container_type.scope() else {
                    return Expr(Type::Unknown, val.to_unknown());
                };
                let Some(&field) = scope.fields.get(name) else {
                    return Expr(Type::Unknown, val.to_unknown());
                };
                let this = container_type.this().clone();
                let mut analyzer = Analyzer::new(self.ip, self.cache, self.documents, this);
                analyzer.resolve_field_access_container(field, val.to_unknown())
            }
            _ => Expr(Type::Unknown, val.to_unknown()),
        }
    }

    fn resolve_field_access_container(&mut self, field: NodeIndex, unknown: Value) -> Expr {
        let handle = self.this.handle().clone();
        let tree = handle.tree();
        let container_field: full::ContainerField = tree.full_node(field).unwrap();
        let Some(ty_index) = container_field.ast.type_expr.to_option() else {
            return Expr(Type::Unknown, unknown);
        };
        let ty = self.resolve_type(Node(handle, ty_index));
        Expr(ty, unknown)
    }

    // +-------------------------------+
    // |          Expressions          |
    // +-------------------------------+

    fn resolve_expr_uncached(&mut self, node: &Node) -> Expr {
        let tree = node.handle().tree();
        match tree.node_tag(node.index()) {
            NodeTag::Identifier
            | NodeTag::UnwrapOptional
            | NodeTag::Try
            | NodeTag::Deref
            | NodeTag::ArrayAccess
            | NodeTag::SliceOpen
            | NodeTag::Slice
            | NodeTag::SliceSentinel
            | NodeTag::FieldAccess => unreachable!(),
            NodeTag::AddressOf => self.resolve_address_of(node),
            NodeTag::EnumLiteral => self.resolve_enum_literal(node),
            NodeTag::ErrorValue => self.resolve_error_value(node),
            NodeTag::BuiltinCallTwo
            | NodeTag::BuiltinCallTwoComma
            | NodeTag::BuiltinCall
            | NodeTag::BuiltinCallComma => self.resolve_builtin_call(node),
            NodeTag::IfSimple | NodeTag::If => self.resolve_if(node),
            NodeTag::NumberLiteral => self.resolve_number_literal(node),
            NodeTag::CharLiteral => self.resolve_char_literal(),
            NodeTag::StringLiteral | NodeTag::MultilineStringLiteral => {
                self.resolve_string_literal()
            }
            NodeTag::ArrayInitOne
            | NodeTag::ArrayInitOneComma
            | NodeTag::ArrayInitDotTwo
            | NodeTag::ArrayInitDotTwoComma
            | NodeTag::ArrayInitDot
            | NodeTag::ArrayInitDotComma
            | NodeTag::ArrayInit
            | NodeTag::ArrayInitComma => self.resolve_array_init(node),
            NodeTag::StructInitOne
            | NodeTag::StructInitOneComma
            | NodeTag::StructInitDotTwo
            | NodeTag::StructInitDotTwoComma
            | NodeTag::StructInitDot
            | NodeTag::StructInitDotComma
            | NodeTag::StructInit
            | NodeTag::StructInitComma => self.resolve_struct_init(node),
            NodeTag::GroupedExpression => self.resolve_grouped_expression(node),
            NodeTag::Comptime => self.resolve_comptime(node),
            NodeTag::Nosuspend => self.resolve_nosuspend(node),
            NodeTag::EqualEqual
            | NodeTag::BangEqual
            | NodeTag::LessThan
            | NodeTag::GreaterThan
            | NodeTag::LessOrEqual
            | NodeTag::GreaterOrEqual => self.resolve_comparison(node),
            NodeTag::BoolAnd | NodeTag::BoolOr | NodeTag::BoolNot => self.resolve_bool_op(node),
            NodeTag::ArrayMult => self.resolve_array_mult(node),
            NodeTag::ArrayCat => self.resolve_array_cat(node),
            NodeTag::CallOne | NodeTag::CallOneComma | NodeTag::Call | NodeTag::CallComma => {
                self.resolve_call(node)
            }
            NodeTag::Continue | NodeTag::Break | NodeTag::Return | NodeTag::UnreachableLiteral => {
                Expr(Type::Noreturn, Value::Unknown)
            }
            NodeTag::OptionalType
            | NodeTag::ErrorSetDecl
            | NodeTag::MergeErrorSets
            | NodeTag::ErrorUnion
            | NodeTag::ArrayType
            | NodeTag::ArrayTypeSentinel
            | NodeTag::PtrTypeAligned
            | NodeTag::PtrTypeSentinel
            | NodeTag::PtrType
            | NodeTag::PtrTypeBitRange
            | NodeTag::FnProtoSimple
            | NodeTag::FnProtoMulti
            | NodeTag::FnProtoOne
            | NodeTag::FnProto
            | NodeTag::Root
            | NodeTag::ContainerDecl
            | NodeTag::ContainerDeclTrailing
            | NodeTag::ContainerDeclTwo
            | NodeTag::ContainerDeclTwoTrailing
            | NodeTag::ContainerDeclArg
            | NodeTag::ContainerDeclArgTrailing
            | NodeTag::TaggedUnion
            | NodeTag::TaggedUnionTrailing
            | NodeTag::TaggedUnionTwo
            | NodeTag::TaggedUnionTwoTrailing
            | NodeTag::TaggedUnionEnumTag
            | NodeTag::TaggedUnionEnumTagTrailing => Expr::from(self.resolve_type_uncached(node)),
            _ => Expr(Type::Unknown, Value::Unknown), // tag => todo!("{tag:?}"),
        }
    }

    fn resolve_address_of(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let index: NodeIndex = unsafe { tree.node_data_unchecked(node.index()) };
        let binding = self.resolve_binding(Node::from(handle, index));
        match binding {
            Binding::Unknown => Expr(Type::Unknown, Value::Unknown),
            Binding::Constant(Expr(ty, val)) => {
                let pointer_type = PointerType::simple_const(PointerSize::One, ty);
                let index = self.ip.intern_type(InternedType::Pointer(pointer_type));
                Expr(Type::Interned(index), val.to_unknown())
            }
            Binding::Variable(Expr(ty, val)) => {
                let pointer_type = PointerType::simple(PointerSize::One, ty);
                let index = self.ip.intern_type(InternedType::Pointer(pointer_type));
                Expr(Type::Interned(index), val.to_unknown())
            }
        }
    }

    fn resolve_enum_literal(&mut self, node: &Node) -> Expr {
        let tree = node.handle().tree();
        let token = tree.node_main_token(node.index());
        let name = tree.token_slice(token);
        let interned = InternedValue::EnumLiteral(Vec::from(name));
        let index = self.ip.intern_value(interned);
        Expr(Type::EnumLiteral, Value::Interned(index))
    }

    fn resolve_error_value(&mut self, node: &Node) -> Expr {
        let tree = node.handle().tree();
        let mut token = tree.node_main_token(node.index());
        token.0 += 2;
        if token.0 >= tree.token_count() {
            return Expr(Type::Unknown, Value::Unknown);
        }
        let name = tree.token_slice(token);
        let interned_type = InternedType::ErrorSet(BTreeSet::from([Vec::from(name)]));
        let interned_value = InternedValue::ErrorValue(Vec::from(name));
        let type_index = self.ip.intern_type(interned_type);
        let value_index = self.ip.intern_value(interned_value);
        Expr(Type::Interned(type_index), Value::Interned(value_index))
    }

    fn resolve_builtin_call(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let args_buf = tree.builtin_call_params(node.index()).unwrap();
        let args = args_buf.get();
        match tree.builtin_call_tag(node.index()).unwrap() {
            BuiltinFnTag::r#as => self.resolve_builtin_call_as(handle, args),
            BuiltinFnTag::import => self.resolve_builtin_call_import(handle, args),

            BuiltinFnTag::FieldType => Expr::from(Type::Unknown),
            BuiltinFnTag::This => Expr::from(self.resolve_builtin_call_this()),
            BuiltinFnTag::Type => Expr::from(Type::Unknown),
            BuiltinFnTag::TypeOf => Expr::from(self.resolve_builtin_call_type_of(handle, args)),
            BuiltinFnTag::Vector => Expr::from(self.resolve_builtin_call_vector(handle, args)),

            _ => Expr(Type::Unknown, Value::Unknown), // tag => todo!("{tag:?}"),
        }
    }

    fn resolve_builtin_call_as(&mut self, handle: &Handle, args: &[NodeIndex]) -> Expr {
        let &[head, ref tail @ ..] = args else {
            return Expr(Type::Unknown, Value::Unknown);
        };
        let ty = self.resolve_type(Node::from(handle, head));
        let val = match tail {
            &[expr_index] => {
                let expr = self.resolve_expr(Node::from(handle, expr_index));
                expr.value()
            }
            _ => Value::Unknown,
        };
        Expr(ty, val)
    }

    fn resolve_builtin_call_import(&mut self, handle: &Handle, args: &[NodeIndex]) -> Expr {
        let &[arg, ..] = args else {
            return Expr(Type::Unknown, Value::Unknown);
        };
        let tree = handle.tree();
        let Some(string) = tree.parse_string_literal(arg) else {
            return Expr(Type::Unknown, Value::Unknown);
        };
        match string.as_bytes() {
            b"std" => Expr::from(Type::Unknown),     // TODO
            b"builtin" => Expr::from(Type::Unknown), // TODO
            bytes if bytes.ends_with(b".zig") => {
                let Ok(str) = std::str::from_utf8(bytes) else {
                    return Expr::from(Type::Unknown);
                };
                let Some(dir) = handle.path().parent() else {
                    return Expr::from(Type::Unknown);
                };
                let path: Rc<Path> = Rc::from(dir.join(str));
                let Some(document) = self.documents.get_or_parse(path.clone()) else {
                    return Expr::from(Type::Unknown);
                };
                let tree = document.tree();
                let handle = Handle(path.clone(), tree.clone());
                let this = Node(handle, NodeIndex::ROOT);
                let scope = document.get(NodeIndex::ROOT);
                let container_type = ContainerType { this, scope };
                let interned = InternedType::Container(container_type);
                let index = self.ip.intern_type(interned);
                Expr::from(Type::Interned(index))
            }
            bytes if bytes.ends_with(b".zon") => Expr(Type::Unknown, Value::Unknown),
            _ => Expr::from(Type::Unknown), // bytes => todo!("{}", String::from_utf8_lossy(bytes)),
        }
    }

    fn resolve_builtin_call_this(&mut self) -> Type {
        let this = self.this.clone();
        let container_type = ContainerType::new(this, self.documents);
        let interned = InternedType::Container(container_type);
        let index = self.ip.intern_type(interned);
        Type::Interned(index)
    }

    fn resolve_builtin_call_type_of(&mut self, handle: &Handle, args: &[NodeIndex]) -> Type {
        if args.is_empty() {
            return Type::Unknown;
        }
        let mut peer_types = Vec::with_capacity(args.len());
        for &arg in args {
            let expr = self.resolve_expr(Node::from(handle, arg));
            peer_types.push(expr.type_of());
        }
        self.resolve_peer_types(&peer_types)
    }

    fn resolve_builtin_call_vector(&mut self, handle: &Handle, args: &[NodeIndex]) -> Type {
        let child = match args {
            &[_, child_index, ..] => self.resolve_type(Node::from(handle, child_index)),
            _ => Type::Unknown,
        };
        let index = self.ip.intern_type(InternedType::Vector(child));
        Type::Interned(index)
    }

    fn resolve_if(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let full_if: full::If = tree.full_node(node.index()).unwrap();
        let then_expr = self.resolve_expr(Node::from(handle, full_if.ast.then_expr));
        let else_expr = match full_if.ast.else_expr.to_option() {
            Some(else_index) => self.resolve_expr(Node::from(handle, else_index)),
            None => Expr(Type::Void, Value::Void),
        };
        self.resolve_branching_expressions(&[then_expr, else_expr])
    }

    fn resolve_number_literal(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let bytes = tree.node_source(node.index());
        for &byte in bytes {
            if byte == b'.' {
                return Expr(Type::ComptimeFloat, Value::Unknown);
            }
        }
        if let Some(int) = parse_str(bytes) {
            return Expr(Type::ComptimeInt, Value::Int(int));
        }
        Expr(Type::ComptimeInt, Value::Unknown)
    }

    fn resolve_char_literal(&mut self) -> Expr {
        Expr(Type::ComptimeInt, Value::Unknown)
    }

    fn resolve_string_literal(&mut self) -> Expr {
        let sentinel = Some(Value::Int(0));
        let elem = Type::Int(Signedness::Unsigned, 8);
        let array_type = ArrayType { sentinel, elem };
        let index = self.ip.intern_type(InternedType::Array(array_type));
        let pointer_type = PointerType::simple_const(PointerSize::One, Type::Interned(index));
        let index = self.ip.intern_type(InternedType::Pointer(pointer_type));
        Expr(Type::Interned(index), Value::Unknown)
    }

    fn resolve_array_init(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let buffered = tree.full_node_buffered(node.index()).unwrap();
        let array_init: &full::ArrayInit = buffered.get();
        let unknown = 'unknown: {
            for &element in array_init.ast.elements() {
                let expr = self.resolve_expr(Node::from(handle, element));
                if expr.value().is_runtime() {
                    break 'unknown Value::Runtime;
                }
            }
            Value::Unknown
        };
        if let Some(ty_index) = array_init.ast.type_expr.to_option() {
            let ty = self.resolve_type(Node::from(handle, ty_index));
            return Expr(ty, unknown);
        }
        let mut types = Vec::with_capacity(array_init.ast.elements_len);
        for &element in array_init.ast.elements() {
            let expr = self.resolve_expr(Node::from(handle, element));
            types.push(expr.type_of());
        }
        let index = self.ip.intern_type(InternedType::Tuple(types));
        Expr(Type::Interned(index), unknown)
    }

    fn resolve_struct_init(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let buffered = tree.full_node_buffered(node.index()).unwrap();
        let struct_init: &full::StructInit = buffered.get();
        let unknown = 'unknown: {
            for &field in struct_init.ast.fields() {
                let expr = self.resolve_expr(Node::from(handle, field));
                if expr.value().is_runtime() {
                    break 'unknown Value::Runtime;
                }
            }
            Value::Unknown
        };
        let Some(ty_index) = struct_init.ast.type_expr.to_option() else {
            if struct_init.ast.fields_len == 0 {
                let index = self.ip.intern_type(InternedType::Tuple(vec![]));
                return Expr(Type::Interned(index), unknown);
            }
            return Expr(Type::Unknown, unknown);
        };
        let ty = self.resolve_type(Node::from(handle, ty_index));
        Expr(ty, unknown)
    }

    fn resolve_grouped_expression(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndToken(lhs, _) = unsafe { tree.node_data_unchecked(node.index()) };
        self.resolve_expr(Node::from(handle, lhs))
    }

    fn resolve_comptime(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let index: NodeIndex = unsafe { tree.node_data_unchecked(node.index()) };
        self.resolve_expr(Node::from(handle, index))
    }

    fn resolve_nosuspend(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let index: NodeIndex = unsafe { tree.node_data_unchecked(node.index()) };
        self.resolve_expr(Node::from(handle, index))
    }

    fn resolve_comparison(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndNode(lhs, rhs) = unsafe { tree.node_data_unchecked(node.index()) };
        let Expr(lhs_ty, lhs_val) = self.resolve_expr(Node::from(handle, lhs));
        let Expr(rhs_ty, rhs_val) = self.resolve_expr(Node::from(handle, rhs));
        let unknown = match (lhs_val, rhs_val) {
            (Value::Runtime, _) | (_, Value::Runtime) => Value::Runtime,
            _ => Value::Unknown,
        };
        for ty in [lhs_ty, rhs_ty] {
            let Type::Interned(index) = ty else {
                continue;
            };
            let Some(InternedType::Vector(_)) = self.ip.get_type(index) else {
                continue;
            };
            let interned = InternedType::Vector(Type::Bool);
            let index = self.ip.intern_type(interned);
            return Expr(Type::Interned(index), unknown);
        }
        Expr(Type::Bool, unknown)
    }

    fn resolve_bool_op(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndNode(lhs, rhs) = unsafe { tree.node_data_unchecked(node.index()) };
        let Expr(_lhs_ty, lhs_val) = self.resolve_expr(Node::from(handle, lhs));
        let Expr(_rhs_ty, rhs_val) = self.resolve_expr(Node::from(handle, rhs));
        let unknown = match (lhs_val, rhs_val) {
            (Value::Runtime, _) | (_, Value::Runtime) => Value::Runtime,
            _ => Value::Unknown,
        };
        Expr(Type::Bool, unknown)
    }

    fn resolve_array_mult(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndNode(lhs, rhs) = unsafe { tree.node_data_unchecked(node.index()) };
        let Expr(lhs_ty, lhs_val) = self.resolve_expr(Node::from(handle, lhs));
        let Expr(_rhs_ty, rhs_val) = self.resolve_expr(Node::from(handle, rhs));
        let Type::Interned(lhs_index) = lhs_ty else {
            return Expr(Type::Unknown, lhs_val.to_unknown());
        };
        let Some(lhs_interned) = self.ip.get_type(lhs_index) else {
            return Expr(Type::Unknown, lhs_val.to_unknown());
        };
        match lhs_interned {
            &InternedType::Array(_) => {
                // This technically should have a different type, but we don't
                // keep track of array lengths.
                Expr(Type::Interned(lhs_index), lhs_val.to_unknown())
            }
            &InternedType::Pointer(pointer_type) => {
                if pointer_type.size != PointerSize::One {
                    return Expr(Type::Unknown, lhs_val.to_unknown());
                }
                let Type::Interned(index) = pointer_type.child else {
                    return Expr(Type::Unknown, lhs_val.to_unknown());
                };
                let Some(interned) = self.ip.get_type(index) else {
                    return Expr(Type::Unknown, lhs_val.to_unknown());
                };
                let &InternedType::Array(_) = interned else {
                    return Expr(Type::Unknown, lhs_val.to_unknown());
                };
                Expr(Type::Interned(lhs_index), lhs_val.to_unknown())
            }
            InternedType::Tuple(types) => {
                let Value::Int(rhs_int) = rhs_val else {
                    return Expr(Type::Unknown, lhs_val.to_unknown());
                };
                let Ok(mult) = rhs_int.try_into() else {
                    return Expr(Type::Unknown, lhs_val.to_unknown());
                };
                let mut new_types = Vec::with_capacity(types.len() * mult);
                for _ in 0..mult {
                    new_types.extend_from_slice(&types);
                }
                let index = self.ip.intern_type(InternedType::Tuple(new_types));
                Expr(Type::Interned(index), lhs_val.to_unknown())
            }
            _ => Expr(Type::Unknown, lhs_val.to_unknown()),
        }
    }

    fn resolve_array_cat(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndNode(lhs, rhs) = unsafe { tree.node_data_unchecked(node.index()) };
        let Expr(lhs_ty, lhs_val) = self.resolve_expr(Node::from(handle, lhs));
        let Expr(rhs_ty, rhs_val) = self.resolve_expr(Node::from(handle, rhs));
        let unknown = match (lhs_val, rhs_val) {
            (Value::Runtime, _) | (_, Value::Runtime) => Value::Runtime,
            _ => Value::Unknown,
        };
        let Type::Interned(lhs_index) = lhs_ty else {
            return Expr(Type::Unknown, unknown);
        };
        let Type::Interned(rhs_index) = rhs_ty else {
            return Expr(Type::Unknown, unknown);
        };
        let Some(lhs_interned) = self.ip.get_type(lhs_index) else {
            return Expr(Type::Unknown, unknown);
        };
        let Some(rhs_interned) = self.ip.get_type(rhs_index) else {
            return Expr(Type::Unknown, unknown);
        };
        if let InternedType::Tuple(lhs_types) = lhs_interned
            && let InternedType::Tuple(rhs_types) = rhs_interned
        {
            let mut types = Vec::with_capacity(lhs_types.len() + rhs_types.len());
            types.extend_from_slice(&lhs_types);
            types.extend_from_slice(&rhs_types);
            let index = self.ip.intern_type(InternedType::Tuple(types));
            return Expr(Type::Interned(index), unknown);
        }
        let mut is_pointer = false;
        let mut final_sentinel = None;
        let mut peer_types = Vec::with_capacity(2);
        for interned in [lhs_interned, rhs_interned] {
            match interned {
                &InternedType::Array(ArrayType { sentinel, elem }) => {
                    if sentinel.is_some() {
                        final_sentinel = final_sentinel.map_or(sentinel, |_| None);
                    }
                    peer_types.push(elem);
                }
                InternedType::Pointer(pointer_type) => {
                    if pointer_type.size != PointerSize::One {
                        return Expr(Type::Unknown, unknown);
                    }
                    let Type::Interned(index) = pointer_type.child else {
                        return Expr(Type::Unknown, unknown);
                    };
                    let Some(interned) = self.ip.get_type(index) else {
                        return Expr(Type::Unknown, unknown);
                    };
                    let &InternedType::Array(ArrayType { sentinel, elem }) = interned else {
                        return Expr(Type::Unknown, unknown);
                    };
                    is_pointer = true;
                    if sentinel.is_some() {
                        final_sentinel = final_sentinel.map_or(sentinel, |_| None);
                    }
                    peer_types.push(elem);
                }
                InternedType::Tuple(types) => {
                    peer_types.extend_from_slice(types);
                }
                _ => return Expr(Type::Unknown, unknown),
            }
        }
        let sentinel = final_sentinel;
        let elem = self.resolve_peer_types(&peer_types);
        let array_type = ArrayType { sentinel, elem };
        let index = self.ip.intern_type(InternedType::Array(array_type));
        if is_pointer {
            let pointer_type = PointerType::simple_const(PointerSize::One, Type::Interned(index));
            let index = self.ip.intern_type(InternedType::Pointer(pointer_type));
            return Expr(Type::Interned(index), unknown);
        }
        Expr(Type::Interned(index), unknown)
    }

    fn resolve_call(&mut self, node: &Node) -> Expr {
        let handle = node.handle();
        let tree = handle.tree();
        let buffered = tree.full_node_buffered(node.index()).unwrap();
        let call: &full::Call = buffered.get();
        let mut fn_expr = self.resolve_expr(Node::from(handle, call.ast.fn_expr));
        'method: {
            if fn_expr.type_of() != Type::Unknown {
                break 'method;
            }
            if tree.node_tag(call.ast.fn_expr) != NodeTag::FieldAccess {
                break 'method;
            }
            let NodeAndToken(lhs, token) = unsafe { tree.node_data_unchecked(call.ast.fn_expr) };
            let lhs_expr = self.resolve_expr(Node::from(handle, lhs));
            let type_of_lhs = lhs_expr.type_of();
            let decl_name = tree.token_slice(token);
            let Binding::Constant(expr) = self.resolve_decl_access(type_of_lhs, decl_name) else {
                break 'method;
            };
            fn_expr = expr;
            // The first parameter should technically be type_of_lhs, but let's
            // leave that for the compiler to complain about.
        }
        let Expr(ty, val) = fn_expr;
        let Type::Interned(index) = ty else {
            return Expr(Type::Unknown, val.to_unknown());
        };
        let Some(interned) = self.ip.get_type(index) else {
            return Expr(Type::Unknown, val.to_unknown());
        };
        let fn_type = match interned {
            InternedType::Function(fn_type) => fn_type,
            InternedType::Pointer(pointer_type) => {
                if pointer_type.size != PointerSize::One {
                    return Expr(Type::Unknown, val.to_unknown());
                }
                let Type::Interned(index) = pointer_type.child else {
                    return Expr(Type::Unknown, val.to_unknown());
                };
                let Some(InternedType::Function(fn_type)) = self.ip.get_type(index) else {
                    return Expr(Type::Unknown, val.to_unknown());
                };
                fn_type
            }
            _ => return Expr(Type::Unknown, val.to_unknown()),
        };
        Expr(fn_type.return_type, val.to_unknown())
    }

    // +-------------------------+
    // |          Types          |
    // +-------------------------+

    fn resolve_type_uncached(&mut self, node: &Node) -> Type {
        let tree = node.handle().tree();
        match tree.node_tag(node.index()) {
            NodeTag::OptionalType => self.resolve_optional_type(node),
            NodeTag::ErrorSetDecl => self.resolve_error_set(node),
            NodeTag::MergeErrorSets => self.resolve_merge_error_sets(node),
            NodeTag::ErrorUnion => self.resolve_error_union(node),
            NodeTag::ArrayType | NodeTag::ArrayTypeSentinel => self.resolve_array_type(node),
            NodeTag::PtrTypeAligned
            | NodeTag::PtrTypeSentinel
            | NodeTag::PtrType
            | NodeTag::PtrTypeBitRange => self.resolve_ptr_type(node),
            NodeTag::FnProtoSimple
            | NodeTag::FnProtoMulti
            | NodeTag::FnProtoOne
            | NodeTag::FnProto => self.resolve_fn_proto(node),
            NodeTag::Root
            | NodeTag::ContainerDecl
            | NodeTag::ContainerDeclTrailing
            | NodeTag::ContainerDeclTwo
            | NodeTag::ContainerDeclTwoTrailing
            | NodeTag::ContainerDeclArg
            | NodeTag::ContainerDeclArgTrailing
            | NodeTag::TaggedUnion
            | NodeTag::TaggedUnionTrailing
            | NodeTag::TaggedUnionTwo
            | NodeTag::TaggedUnionTwoTrailing
            | NodeTag::TaggedUnionEnumTag
            | NodeTag::TaggedUnionEnumTagTrailing => self.resolve_container_decl(node),
            _ => unreachable!(),
        }
    }

    fn resolve_optional_type(&mut self, node: &Node) -> Type {
        let handle = node.handle();
        let tree = handle.tree();
        let expr_index: NodeIndex = unsafe { tree.node_data_unchecked(node.index()) };
        let child = self.resolve_type(Node::from(handle, expr_index));
        let index = self.ip.intern_type(InternedType::Optional(child));
        Type::Interned(index)
    }

    fn resolve_error_set(&mut self, node: &Node) -> Type {
        let handle = node.handle();
        let tree = handle.tree();
        let TokenAndToken(TokenIndex(lbrace), TokenIndex(rbrace)) =
            unsafe { tree.node_data_unchecked(node.index()) };
        let mut names = BTreeSet::new();
        for idx in lbrace + 1..rbrace {
            let token = TokenIndex(idx);
            if tree.token_tag(token) != TokenTag::Identifier {
                continue;
            }
            names.insert(Vec::from(tree.token_slice(token)));
        }
        let index = self.ip.intern_type(InternedType::ErrorSet(names));
        Type::Interned(index)
    }

    fn resolve_merge_error_sets(&mut self, node: &Node) -> Type {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndNode(lhs, rhs) = unsafe { tree.node_data_unchecked(node.index()) };
        let lhs_ty = self.resolve_type(Node::from(handle, lhs));
        let Type::Interned(lhs_index) = lhs_ty else {
            return Type::Unknown;
        };
        let rhs_ty = self.resolve_type(Node::from(handle, rhs));
        let Type::Interned(rhs_index) = rhs_ty else {
            return Type::Unknown;
        };
        let Some(InternedType::ErrorSet(lhs_names)) = self.ip.get_type(lhs_index) else {
            return Type::Unknown;
        };
        let Some(InternedType::ErrorSet(rhs_names)) = self.ip.get_type(rhs_index) else {
            return Type::Unknown;
        };
        let mut merged_names = BTreeSet::new();
        merged_names.extend(lhs_names.clone());
        merged_names.extend(rhs_names.clone());
        let index = self.ip.intern_type(InternedType::ErrorSet(merged_names));
        Type::Interned(index)
    }

    fn resolve_error_union(&mut self, node: &Node) -> Type {
        let handle = node.handle();
        let tree = handle.tree();
        let NodeAndNode(lhs, rhs) = unsafe { tree.node_data_unchecked(node.index()) };
        let lhs_ty = self.resolve_type(Node::from(handle, lhs));
        let rhs_ty = self.resolve_type(Node::from(handle, rhs));
        let interned = InternedType::ErrorUnion(lhs_ty, rhs_ty);
        let index = self.ip.intern_type(interned);
        Type::Interned(index)
    }

    fn resolve_array_type(&mut self, node: &Node) -> Type {
        let handle = node.handle();
        let tree = handle.tree();
        let array: full::ArrayType = tree.full_node(node.index()).unwrap();
        let sentinel_index = array.ast.sentinel.to_option();
        let sentinel = sentinel_index.map(|index| {
            let expr = self.resolve_expr(Node::from(handle, index));
            expr.value()
        });
        let elem = self.resolve_type(Node::from(handle, array.ast.elem_type));
        let array_type = ArrayType { sentinel, elem };
        let index = self.ip.intern_type(InternedType::Array(array_type));
        Type::Interned(index)
    }

    fn resolve_ptr_type(&mut self, node: &Node) -> Type {
        let handle = node.handle();
        let tree = handle.tree();
        let ptr: full::PtrType = tree.full_node(node.index()).unwrap();
        let sentinel_index = ptr.ast.sentinel.to_option();
        let sentinel = sentinel_index.map(|index| {
            let expr = self.resolve_expr(Node::from(handle, index));
            expr.value()
        });
        let child = self.resolve_type(Node::from(handle, ptr.ast.child_type));
        let index = self.ip.intern_type(InternedType::Pointer(PointerType {
            size: ptr.size,
            sentinel,
            is_allowzero: !ptr.allowzero_token.is_none(),
            has_align: !ptr.ast.align_node.is_none(),
            has_bit_range_start: !ptr.ast.bit_range_start.is_none(),
            has_bit_range_end: !ptr.ast.bit_range_end.is_none(),
            has_addrspace: match ptr.ast.addrspace_node.to_option() {
                Some(index) => tree.node_source(index) != b".generic",
                None => false,
            },
            is_const: !ptr.const_token.is_none(),
            is_volatile: !ptr.volatile_token.is_none(),
            child,
        }));
        Type::Interned(index)
    }

    fn resolve_fn_proto(&mut self, node: &Node) -> Type {
        let handle = node.handle();
        let tree = handle.tree();
        let fn_proto_buf = tree.full_node_buffered(node.index()).unwrap();
        let fn_proto: &full::FnProto = fn_proto_buf.get();
        // TODO: align, addrspace, section (?)
        let fn_type = FunctionType {
            params: {
                let mut params = Vec::with_capacity(fn_proto.ast.params_len);
                for &index in fn_proto.ast.params() {
                    params.push(self.resolve_type(Node::from(handle, index)));
                }
                params
            },
            has_callconv: !fn_proto.ast.callconv_expr.is_none(),
            return_type: {
                (fn_proto.ast.return_type.to_option())
                    .map(|index| self.resolve_type(Node::from(handle, index)))
                    .unwrap_or(Type::Unknown)
            },
        };
        let index = self.ip.intern_type(InternedType::Function(fn_type));
        Type::Interned(index)
    }

    fn resolve_container_decl(&mut self, node: &Node) -> Type {
        let handle = node.handle();
        let tree = handle.tree();
        let buffered = tree.full_node_buffered(node.index()).unwrap();
        let container_decl: &full::ContainerDecl = buffered.get();
        'tuple: {
            if tree.token_tag(container_decl.ast.main_token) != TokenTag::KeywordStruct {
                break 'tuple;
            }
            let members = container_decl.ast.members();
            if members.is_empty() {
                break 'tuple;
            }
            let mut types = Vec::new();
            for &member in members {
                let container_field: full::ContainerField = match tree.full_node(member) {
                    Some(container_field) => container_field,
                    None => break 'tuple,
                };
                if !container_field.ast.tuple_like {
                    break 'tuple;
                }
                let field_ty = {
                    (container_field.ast.type_expr.to_option())
                        .map(|index| self.resolve_type(Node::from(handle, index)))
                        .unwrap_or(Type::Unknown)
                };
                types.push(field_ty);
            }
            let index = self.ip.intern_type(InternedType::Tuple(types));
            return Type::Interned(index);
        }
        let this = node.clone();
        let container_type = ContainerType::new(this, self.documents);
        let interned = InternedType::Container(container_type);
        let index = self.ip.intern_type(interned);
        Type::Interned(index)
    }

    // +----------------------------------+
    // |          Helper methods          |
    // +----------------------------------+

    pub fn resolve_union_tag(&mut self, ty: Type) -> Option<Type> {
        let Type::Interned(index) = ty else {
            return None;
        };
        let interned = self.ip.get_type(index)?;
        let InternedType::Container(container_type) = interned else {
            return None;
        };
        let node = container_type.this();
        let handle = node.handle();
        let tree = handle.tree();
        let buffered = tree.full_node_buffered(node.index()).unwrap();
        let container_decl: &full::ContainerDecl = buffered.get();
        if !container_decl.ast.enum_token.is_none() {
            // Tagged union with inferred tag type:
            //     union(enum) { ... }
            return Some(Type::UnionTag(index));
        }
        let Some(arg) = container_decl.ast.arg.to_option() else {
            // Bare union:
            //     union { ... }
            return None;
        };
        // Tagged union with explicit tag type:
        //     union(Foo) { ... }
        Some(self.resolve_type(Node::from(handle, arg)))
    }

    // +-----------------------------------+
    // |          Branching Types          |
    // +-----------------------------------+

    pub fn resolve_branching_expressions(&mut self, branching_expressions: &[Expr]) -> Expr {
        match branching_expressions {
            &[] => return Expr(Type::Unknown, Value::Unknown),
            &[expr] => return expr,
            _ => {}
        }

        'same_expr: {
            let expr = branching_expressions[0];
            for &other in &branching_expressions[1..] {
                if other != expr {
                    break 'same_expr;
                }
            }
            return expr;
        }

        let mut branches = Vec::with_capacity(branching_expressions.len());
        let mut unknown = Value::Unknown;
        for &expr in branching_expressions.iter() {
            let Expr(ty, val) = expr;
            branches.push(ty);
            if val == Value::Runtime {
                unknown = Value::Runtime;
            }
        }
        let mut ty = self.resolve_peer_types(&branches);
        let mut val = unknown;
        match ty {
            Type::Type => {
                branches.clear();
                for &expr in branching_expressions.iter() {
                    match expr.value() {
                        Value::Type(t) => branches.push(t),
                        _ => return Expr(Type::Type, Value::Unknown),
                    }
                }
                val = Value::Type(self.resolve_branching_types(&branches));
            }
            Type::Unknown => {
                ty = self.resolve_branching_types(&branches);
            }
            _ => {}
        }
        Expr(ty, val)
    }

    pub fn resolve_branching_types(&mut self, branching_types: &[Type]) -> Type {
        match branching_types {
            &[] => return Type::Unknown,
            &[ty] => return ty,
            _ => {}
        }

        'same_type: {
            let ty = branching_types[0];
            for &other in &branching_types[1..] {
                if other != ty {
                    break 'same_type;
                }
            }
            return ty;
        }

        if let Some(ty) = self.resolve_branching_types_nested(branching_types) {
            return ty;
        }

        let mut merged_types = BTreeSet::new();
        for &ty in branching_types {
            'branched: {
                let Type::Interned(index) = ty else {
                    break 'branched;
                };
                let Some(interned) = self.ip.get_type(index) else {
                    break 'branched;
                };
                let InternedType::Branched(types) = interned else {
                    break 'branched;
                };
                merged_types.extend(types.iter().copied());
            }
            merged_types.insert(TypeOrd(ty));
        }
        let index = self.ip.intern_type(InternedType::Branched(merged_types));
        Type::Interned(index)
    }

    fn resolve_branching_types_nested(&mut self, branching_types: &[Type]) -> Option<Type> {
        macro_rules! get {
            ($ty:expr, $pat:pat) => {
                let Type::Interned(index) = $ty else {
                    return None;
                };
                let Some(interned) = self.ip.get_type(index) else {
                    return None;
                };
                let $pat = interned else {
                    return None;
                };
            };
        }
        let a = branching_types[0];
        let Type::Interned(a_index) = a else {
            return None;
        };
        let Some(a_interned) = self.ip.get_type(a_index) else {
            return None;
        };
        let interned = match a_interned {
            &InternedType::Optional(a_ty) => {
                let mut branches = Vec::with_capacity(branching_types.len());
                branches.push(a_ty);
                for &b in &branching_types[1..] {
                    get!(b, &InternedType::Optional(b_ty));
                    branches.push(b_ty);
                }
                let child = self.resolve_branching_types(&branches);
                InternedType::Optional(child)
            }
            InternedType::ErrorSet(_) => {
                return None;
            }
            &InternedType::ErrorUnion(a_error, a_payload) => {
                let mut error_branches = Vec::with_capacity(branching_types.len());
                let mut payload_branches = Vec::with_capacity(branching_types.len());
                error_branches.push(a_error);
                payload_branches.push(a_payload);
                for &b in &branching_types[1..] {
                    get!(b, &InternedType::ErrorUnion(b_error, b_payload));
                    error_branches.push(b_error);
                    payload_branches.push(b_payload);
                }
                let error = self.resolve_branching_types(&error_branches);
                let payload = self.resolve_branching_types(&payload_branches);
                InternedType::ErrorUnion(error, payload)
            }
            &InternedType::Vector(a_ty) => {
                let mut branches = Vec::with_capacity(branching_types.len());
                branches.push(a_ty);
                for &b in &branching_types[1..] {
                    get!(b, &InternedType::Vector(b_ty));
                    branches.push(b_ty);
                }
                let child = self.resolve_branching_types(&branches);
                InternedType::Vector(child)
            }
            &InternedType::Array(a_array) => {
                let sentinel = a_array.sentinel;
                for &b in &branching_types[1..] {
                    get!(b, &InternedType::Array(b_array));
                    if sentinel != b_array.sentinel {
                        return None;
                    }
                }
                let mut branches = Vec::with_capacity(branching_types.len());
                branches.push(a_array.elem);
                for &b in &branching_types[1..] {
                    get!(b, &InternedType::Array(b_array));
                    branches.push(b_array.elem);
                }
                let elem = self.resolve_branching_types(&branches);
                InternedType::Array(ArrayType { sentinel, elem })
            }
            &InternedType::Pointer(a_pointer) => {
                let mut pointer = a_pointer;
                pointer.child = Type::Unknown;
                for &b in &branching_types[1..] {
                    get!(b, &InternedType::Pointer(mut b_pointer));
                    b_pointer.child = Type::Unknown;
                    if pointer != b_pointer {
                        return None;
                    }
                }
                let mut branches = Vec::with_capacity(branching_types.len());
                branches.push(a_pointer.child);
                for &b in &branching_types[1..] {
                    get!(b, &InternedType::Pointer(b_pointer));
                    branches.push(b_pointer.child);
                }
                pointer.child = self.resolve_branching_types(&branches);
                InternedType::Pointer(pointer)
            }
            InternedType::Function(a_function) => {
                let param_count = a_function.params.len();
                let has_callconv = a_function.has_callconv;
                for &b in &branching_types[1..] {
                    get!(b, InternedType::Function(b_function));
                    if param_count != b_function.params.len() {
                        return None;
                    }
                    if has_callconv != b_function.has_callconv {
                        return None;
                    }
                }
                let mut return_branches = Vec::with_capacity(branching_types.len());
                return_branches.push(a_function.return_type);
                for &b in &branching_types[1..] {
                    get!(b, InternedType::Function(b_function));
                    return_branches.push(b_function.return_type);
                }
                let mut params = a_function.params.clone();
                let mut param_branches = Vec::with_capacity(branching_types.len());
                for i in 0..param_count {
                    param_branches.clear();
                    param_branches.push(params[i]);
                    for &b in &branching_types[1..] {
                        get!(b, InternedType::Function(b_function));
                        param_branches.push(b_function.params[i]);
                    }
                    params[i] = self.resolve_branching_types(&param_branches);
                }
                let return_type = self.resolve_branching_types(&return_branches);
                InternedType::Function(FunctionType {
                    params,
                    has_callconv,
                    return_type,
                })
            }
            InternedType::Tuple(a_types) => {
                let field_count = a_types.len();
                for &b in &branching_types[1..] {
                    get!(b, InternedType::Tuple(b_types));
                    if field_count != b_types.len() {
                        return None;
                    }
                }
                let mut types = a_types.clone();
                let mut branches = Vec::with_capacity(branching_types.len());
                for i in 0..field_count {
                    branches.clear();
                    branches.push(types[i]);
                    for &b in &branching_types[1..] {
                        get!(b, InternedType::Tuple(b_types));
                        branches.push(b_types[i]);
                    }
                    types[i] = self.resolve_branching_types(&branches);
                }
                InternedType::Tuple(types)
            }
            InternedType::Container(_) => {
                return None;
            }
            InternedType::Branched(_) => {
                return None;
            }
        };
        let index = self.ip.intern_type(interned);
        Some(Type::Interned(index))
    }

    // +------------------------------+
    // |          Peer Types          |
    // +------------------------------+

    // Based on https://codeberg.org/ziglang/zig/src/tag/0.15.2/src/Sema.zig#L33038
    pub fn resolve_peer_types(&mut self, peer_types: &[Type]) -> Type {
        match peer_types {
            &[] => return Type::Noreturn,
            &[ty] => return ty,
            _ => {}
        }

        'same_type: {
            let ty = peer_types[0];
            for &other in &peer_types[1..] {
                if other != ty {
                    break 'same_type;
                }
            }
            return ty;
        }

        let mut peer_types: Vec<_> = peer_types.iter().copied().map(Some).collect();
        match self.resolve_peer_types_inner(&mut peer_types) {
            Ok(ty) => ty,
            Err(()) => Type::Unknown,
        }
    }

    pub fn resolve_peer_types_inner(&mut self, peer_tys: &mut [Option<Type>]) -> Result<Type, ()> {
        let mut strat_reason = 0;
        let mut s = PeerResolveStrategy::Unknown;
        for (i, &opt_ty) in peer_tys.iter().enumerate() {
            if let Some(ty) = opt_ty {
                let other = PeerResolveStrategy::select(ty, self.ip);
                s = s.merge(other, &mut strat_reason, i)
            }
        }

        if s == PeerResolveStrategy::Unknown {
            s = PeerResolveStrategy::Exact;
        } else {
            for opt_ty in peer_tys.iter_mut() {
                let Some(ty) = opt_ty else {
                    continue;
                };
                match ty {
                    Type::Noreturn | Type::Undefined => *opt_ty = None,
                    _ => {}
                }
            }
        }

        match s {
            PeerResolveStrategy::Unknown => unreachable!(),

            PeerResolveStrategy::ErrorSet => {
                let mut final_names = BTreeSet::new();
                for opt_ty in peer_tys.iter() {
                    let &Some(ty) = opt_ty else {
                        continue;
                    };
                    let Type::Interned(index) = ty else {
                        return Err(());
                    };
                    let Some(interned) = self.ip.get_type(index) else {
                        return Err(());
                    };
                    let InternedType::ErrorSet(names) = interned else {
                        return Err(());
                    };
                    final_names.extend(names.clone());
                }
                let interned = InternedType::ErrorSet(final_names);
                let index = self.ip.intern_type(interned);
                Ok(Type::Interned(index))
            }

            PeerResolveStrategy::ErrorUnion => {
                let mut final_names = Some(BTreeSet::new()); // None if error set is unknown
                for opt_ty in peer_tys.iter_mut() {
                    let &mut Some(ty) = opt_ty else {
                        continue;
                    };
                    let names = match ty {
                        Type::Interned(index) => match self.ip.get_type(index) {
                            Some(InternedType::ErrorSet(names)) => {
                                *opt_ty = None;
                                Some(names)
                            }
                            Some(&InternedType::ErrorUnion(set_ty, payload)) => {
                                *opt_ty = Some(payload);
                                match set_ty {
                                    Type::Interned(index) => match self.ip.get_type(index) {
                                        Some(InternedType::ErrorSet(names)) => Some(names),
                                        _ => None,
                                    },
                                    _ => None,
                                }
                            }
                            _ => continue,
                        },
                        _ => continue,
                    };
                    match (&mut final_names, names) {
                        (Some(final_names), Some(names)) => {
                            final_names.extend(names.clone());
                        }
                        (Some(_), None) => final_names = None,
                        (None, _) => {}
                    }
                }
                let set_ty = match final_names {
                    Some(names) => {
                        let interned = InternedType::ErrorSet(names);
                        let index = self.ip.intern_type(interned);
                        Type::Interned(index)
                    }
                    None => Type::Unknown,
                };
                let final_payload = self.resolve_peer_types_inner(peer_tys)?;
                let interned = InternedType::ErrorUnion(set_ty, final_payload);
                let index = self.ip.intern_type(interned);
                Ok(Type::Interned(index))
            }

            PeerResolveStrategy::Nullable => {
                for opt_ty in peer_tys.iter() {
                    let &Some(ty) = opt_ty else {
                        continue;
                    };
                    if ty != Type::Null {
                        return Err(());
                    }
                }
                Ok(Type::Null)
            }

            PeerResolveStrategy::Optional => {
                for opt_ty in peer_tys.iter_mut() {
                    let &mut Some(ty) = opt_ty else {
                        continue;
                    };
                    match ty {
                        Type::Null => {
                            *opt_ty = None;
                        }
                        Type::Interned(index) => match self.ip.get_type(index) {
                            Some(&InternedType::Optional(child)) => {
                                *opt_ty = Some(child);
                            }
                            _ => {}
                        },
                        _ => {}
                    }
                }
                let child_ty = self.resolve_peer_types_inner(peer_tys)?;
                let interned = InternedType::Optional(child_ty);
                let index = self.ip.intern_type(interned);
                Ok(Type::Interned(index))
            }

            PeerResolveStrategy::Array => {
                let mut opt_opt_len = None; // Some(None) if array was found but length is unknown
                let mut sentinel = None;
                let mut opt_elem_ty = None;

                for opt_ty in peer_tys.iter() {
                    let &Some(ty) = opt_ty else {
                        continue;
                    };
                    let Type::Interned(index) = ty else {
                        return Err(());
                    };
                    let Some(interned) = self.ip.get_type(index) else {
                        return Err(());
                    };
                    let (peer_sentinel, peer_elem_ty) = match interned {
                        InternedType::Tuple(field_types) => {
                            if opt_opt_len.is_some() {
                                // Ignore length
                            } else {
                                opt_opt_len = Some(Some(field_types.len()));
                            }
                            sentinel = None;
                            continue;
                        }
                        &InternedType::Array(ArrayType { sentinel, elem }) => (sentinel, elem),
                        &InternedType::Vector(_) => {
                            // This should be handled by the Vector strategy below
                            unreachable!();
                        }
                        _ => return Err(()),
                    };

                    let Some(elem_ty) = opt_elem_ty else {
                        if opt_opt_len.is_none() {
                            opt_opt_len = Some(None);
                            sentinel = peer_sentinel;
                        }
                        opt_elem_ty = Some(peer_elem_ty);
                        continue;
                    };

                    // Ignore length

                    if peer_elem_ty != elem_ty {
                        // TODO: implement coercion
                        return Err(());
                    }

                    if let Some(cur_sent) = sentinel {
                        if let Some(peer_sent) = peer_sentinel {
                            if peer_sent != cur_sent {
                                sentinel = None;
                            }
                        } else {
                            sentinel = None;
                        }
                    }
                }

                assert!(opt_opt_len.is_some());

                let elem = opt_elem_ty.unwrap();
                let interned = InternedType::Array(ArrayType { sentinel, elem });
                let index = self.ip.intern_type(interned);
                Ok(Type::Interned(index))
            }

            PeerResolveStrategy::Vector => {
                let mut opt_opt_len = None; // Some(None) if array/vector was found but length is unknown
                for opt_ty in peer_tys.iter_mut() {
                    let &mut Some(ty) = opt_ty else {
                        continue;
                    };
                    let Type::Interned(index) = ty else {
                        return Err(());
                    };
                    let Some(interned) = self.ip.get_type(index) else {
                        return Err(());
                    };
                    let child_ty = match interned {
                        InternedType::Tuple(field_types) => {
                            if opt_opt_len.is_some() {
                                // Ignore length
                            } else {
                                opt_opt_len = Some(Some(field_types.len()));
                            }
                            *opt_ty = None;
                            continue;
                        }
                        &InternedType::Array(array_type) => array_type.elem,
                        &InternedType::Vector(child) => child,
                        _ => return Err(()),
                    };

                    if opt_opt_len.is_some() {
                        // Ignore length
                    } else {
                        opt_opt_len = Some(None);
                    }

                    *opt_ty = Some(child_ty);
                }

                let child_ty = self.resolve_peer_types_inner(peer_tys)?;
                let interned = InternedType::Vector(child_ty);
                let index = self.ip.intern_type(interned);
                Ok(Type::Interned(index))
            }

            PeerResolveStrategy::CPtr => {
                Err(()) // todo!("{s:?}");
            }

            PeerResolveStrategy::Ptr => {
                Err(()) // todo!("{s:?}");
            }

            PeerResolveStrategy::Func => {
                Err(()) // todo!("{s:?}");
            }

            PeerResolveStrategy::EnumOrUnion => {
                #[derive(Clone, Copy)]
                enum EnumOrUnion {
                    EnumLiteral,
                    Enum(Type),
                    Union(Type),
                }
                let mut opt_cur_eou = None;

                for opt_ty in peer_tys.iter() {
                    let &Some(ty) = opt_ty else {
                        continue;
                    };
                    let eou = match ty {
                        Type::EnumLiteral => EnumOrUnion::EnumLiteral,
                        Type::UnionTag(_) => EnumOrUnion::Enum(ty),
                        Type::Interned(index) => match self.ip.get_type(index) {
                            Some(InternedType::Container(container_type)) => {
                                let node = container_type.this();
                                let tree = node.handle().tree();
                                match tree.token_tag(tree.node_main_token(node.index())) {
                                    TokenTag::KeywordEnum => EnumOrUnion::Enum(ty),
                                    TokenTag::KeywordUnion => EnumOrUnion::Union(ty),
                                    _ => return Err(()),
                                }
                            }
                            _ => return Err(()),
                        },
                        _ => return Err(()),
                    };
                    let Some(cur_eou) = opt_cur_eou else {
                        opt_cur_eou = Some(eou);
                        continue;
                    };
                    match cur_eou {
                        EnumOrUnion::EnumLiteral => {
                            opt_cur_eou = Some(eou);
                        }
                        EnumOrUnion::Enum(cur_ty) => match eou {
                            EnumOrUnion::EnumLiteral => {}
                            EnumOrUnion::Enum(ty) => {
                                if cur_ty != ty {
                                    return Err(());
                                }
                            }
                            EnumOrUnion::Union(ty) => {
                                let tag_ty = self.resolve_union_tag(ty);
                                if tag_ty != Some(cur_ty) {
                                    return Err(());
                                }
                                opt_cur_eou = Some(eou);
                            }
                        },
                        EnumOrUnion::Union(cur_ty) => match eou {
                            EnumOrUnion::EnumLiteral => {}
                            EnumOrUnion::Enum(ty) => {
                                let cur_tag_ty = self.resolve_union_tag(cur_ty);
                                if Some(ty) != cur_tag_ty {
                                    return Err(());
                                }
                            }
                            EnumOrUnion::Union(ty) => {
                                if cur_ty != ty {
                                    return Err(());
                                }
                            }
                        },
                    }
                }
                let cur_eou = opt_cur_eou.unwrap();
                let cur_ty = match cur_eou {
                    EnumOrUnion::EnumLiteral => Type::EnumLiteral,
                    EnumOrUnion::Enum(ty) => ty,
                    EnumOrUnion::Union(ty) => ty,
                };
                Ok(cur_ty)
            }

            PeerResolveStrategy::ComptimeInt => {
                for opt_ty in peer_tys.iter() {
                    let &Some(ty) = opt_ty else {
                        continue;
                    };
                    match ty {
                        Type::ComptimeInt => {}
                        _ => return Err(()),
                    }
                }
                Ok(Type::ComptimeInt)
            }

            PeerResolveStrategy::ComptimeFloat => {
                for opt_ty in peer_tys.iter() {
                    let &Some(ty) = opt_ty else {
                        continue;
                    };
                    match ty {
                        Type::ComptimeInt | Type::ComptimeFloat => {}
                        _ => return Err(()),
                    }
                }
                Ok(Type::ComptimeFloat)
            }

            PeerResolveStrategy::FixedInt => {
                let mut largest_unsigned = None;
                let mut largest_signed = None;

                // Legacy behavior involving comptime integers is not implemented

                for opt_ty in peer_tys.iter() {
                    let &Some(ty) = opt_ty else {
                        continue;
                    };

                    let (signedness, bits) = match ty {
                        Type::ComptimeInt => {
                            // Regardless of value, refine this to fixed-width int
                            continue;
                        }
                        Type::Int(signedness, bits) => (signedness, bits),
                        _ => return Err(()),
                    };

                    let largest_info = match signedness {
                        Signedness::Unsigned => &mut largest_unsigned,
                        Signedness::Signed => &mut largest_signed,
                    };

                    let Some((_, largest_bits)) = *largest_info else {
                        *largest_info = Some((ty, bits));
                        continue;
                    };

                    if bits > largest_bits {
                        *largest_info = Some((ty, bits))
                    }
                }

                match (largest_unsigned, largest_signed) {
                    (None, None) => unreachable!(),

                    (Some((ty_unsigned, _bits_unsigned)), None) => Ok(ty_unsigned),

                    (None, Some((ty_signed, _bits_signed))) => Ok(ty_signed),

                    (Some((_ty_unsigned, bits_unsigned)), Some((ty_signed, bits_signed))) => {
                        if bits_signed > bits_unsigned {
                            return Ok(ty_signed);
                        }

                        Err(())
                    }
                }
            }

            PeerResolveStrategy::FixedFloat => {
                let mut largest_info = None;

                for opt_ty in peer_tys.iter() {
                    let &Some(ty) = opt_ty else {
                        continue;
                    };
                    match ty {
                        Type::ComptimeFloat | Type::ComptimeInt => {}
                        Type::Int(_, _) => {
                            // Regardless of value, refine this to float
                        }
                        Type::Float(bits) => match largest_info {
                            Some((cur_ty, cur_bits)) => {
                                if cur_ty == ty {
                                    continue;
                                }
                                let bits = u16::max(cur_bits, bits);
                                largest_info = Some((Type::Float(bits), bits));
                            }
                            None => {
                                largest_info = Some((ty, bits));
                            }
                        },
                        _ => return Err(()),
                    }
                }

                let (cur_ty, _) = largest_info.unwrap();
                Ok(cur_ty)
            }

            PeerResolveStrategy::Tuple => {
                let mut opt_field_count = None;

                for opt_ty in peer_tys.iter() {
                    let &Some(ty) = opt_ty else {
                        continue;
                    };
                    let Type::Interned(index) = ty else {
                        return Err(());
                    };
                    let Some(interned) = self.ip.get_type(index) else {
                        return Err(());
                    };
                    let InternedType::Tuple(field_types) = interned else {
                        return Err(());
                    };
                    let Some(field_count) = opt_field_count else {
                        opt_field_count = Some(field_types.len());
                        continue;
                    };
                    if field_types.len() != field_count {
                        return Err(());
                    }
                }

                let field_count = opt_field_count.unwrap();
                let mut field_types = Vec::with_capacity(field_count);
                let mut sub_peer_tys = Vec::with_capacity(peer_tys.len());

                for field_index in 0..field_count {
                    sub_peer_tys.clear();
                    for opt_ty in peer_tys.iter() {
                        let &Some(ty) = opt_ty else {
                            sub_peer_tys.push(None);
                            continue;
                        };
                        let Type::Interned(index) = ty else {
                            unreachable!()
                        };
                        let Some(interned) = self.ip.get_type(index) else {
                            unreachable!()
                        };
                        let InternedType::Tuple(sub_field_types) = interned else {
                            unreachable!()
                        };
                        sub_peer_tys.push(Some(sub_field_types[field_index]));
                    }

                    field_types.push(self.resolve_peer_types_inner(&mut sub_peer_tys)?);

                    // Comptime fields are not implemented
                }

                let interned = InternedType::Tuple(field_types);
                let index = self.ip.intern_type(interned);
                Ok(Type::Interned(index))
            }

            PeerResolveStrategy::Exact => {
                let mut expect_ty = None;
                for opt_ty in peer_tys.iter() {
                    let &Some(ty) = opt_ty else {
                        continue;
                    };
                    match expect_ty {
                        Some(expect) => {
                            if ty != expect {
                                return Err(());
                            }
                        }
                        None => {
                            expect_ty = Some(ty);
                        }
                    }
                }
                Ok(expect_ty.unwrap())
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PeerResolveStrategy {
    Unknown,
    ErrorSet,
    ErrorUnion,
    Nullable,
    Optional,
    Array,
    Vector,
    CPtr,
    Ptr,
    Func,
    EnumOrUnion,
    ComptimeInt,
    ComptimeFloat,
    FixedInt,
    FixedFloat,
    Tuple,
    Exact,
}

impl PeerResolveStrategy {
    fn merge(self, other: Self, reason_peer: &mut usize, other_peer_idx: usize) -> Self {
        let s0_is_self = self as u8 <= other as u8;
        let (s0, s1) = match s0_is_self {
            true => (self, other),
            false => (other, self),
        };

        enum ReasonMethod {
            AllS0,
            AllS1,
            Either,
        }

        use PeerResolveStrategy::*;
        use ReasonMethod::*;

        let (reason_method, strat) = match s0 {
            Unknown => (AllS1, s1),
            ErrorSet => match s1 {
                ErrorSet => (Either, ErrorSet),
                _ => (AllS0, ErrorUnion),
            },
            ErrorUnion => match s1 {
                ErrorUnion => (Either, ErrorUnion),
                _ => (AllS0, ErrorUnion),
            },
            Nullable => match s1 {
                Nullable => (Either, Nullable),
                CPtr => (AllS1, CPtr),
                _ => (AllS0, Optional),
            },
            Optional => match s1 {
                Optional => (Either, Optional),
                CPtr => (AllS1, CPtr),
                _ => (AllS0, Optional),
            },
            Array => match s1 {
                Array => (Either, Array),
                Vector => (AllS1, Vector),
                _ => (AllS0, Array),
            },
            Vector => match s1 {
                Vector => (Either, Vector),
                _ => (AllS0, Vector),
            },
            CPtr => match s1 {
                CPtr => (Either, CPtr),
                _ => (AllS0, CPtr),
            },
            Ptr => match s1 {
                Ptr => (Either, Ptr),
                _ => (AllS0, Ptr),
            },
            Func => match s1 {
                Func => (Either, Func),
                _ => (AllS1, s1),
            },
            EnumOrUnion => match s1 {
                EnumOrUnion => (Either, EnumOrUnion),
                _ => (AllS0, EnumOrUnion),
            },
            ComptimeInt => match s1 {
                ComptimeInt => (Either, ComptimeInt),
                _ => (AllS1, s1),
            },
            ComptimeFloat => match s1 {
                ComptimeFloat => (Either, ComptimeFloat),
                _ => (AllS1, s1),
            },
            FixedInt => match s1 {
                FixedInt => (Either, FixedInt),
                _ => (AllS1, s1),
            },
            FixedFloat => match s1 {
                FixedFloat => (Either, FixedFloat),
                _ => (AllS1, s1),
            },
            Tuple => match s1 {
                Exact => (AllS1, Exact),
                _ => (AllS0, Tuple),
            },
            Exact => (AllS0, Exact),
        };

        match reason_method {
            AllS0 => {
                if !s0_is_self {
                    *reason_peer = other_peer_idx;
                }
            }
            AllS1 => {
                if s0_is_self {
                    *reason_peer = other_peer_idx;
                }
            }
            Either => {
                *reason_peer = std::cmp::min(*reason_peer, other_peer_idx);
            }
        }

        strat
    }

    fn select(ty: Type, ip: &InternPool) -> Self {
        match ty {
            Type::Unknown => {
                // Peer type resolution with unknown type should fail
                Self::Exact
            }
            Type::Anyopaque => {
                // TODO: what does PTR with anyopaque mean?
                Self::Exact
            }
            Type::Type | Type::Void | Type::Bool => Self::Exact,
            Type::Noreturn | Type::Undefined => Self::Unknown,
            Type::Null => Self::Nullable,
            Type::ComptimeInt => Self::ComptimeInt,
            Type::Int(_, _) => Self::FixedInt,
            Type::Isize | Type::Usize => {
                // These are technically not fixed-width integers
                Self::Exact
            }
            Type::ComptimeFloat => Self::ComptimeFloat,
            Type::Float(_) => Self::FixedFloat,
            Type::EnumLiteral | Type::UnionTag(_) => Self::EnumOrUnion,
            Type::Interned(index) => {
                let Some(interned) = ip.get_type(index) else {
                    // Maybe we should panic instead
                    return Self::Exact;
                };
                match interned {
                    InternedType::Pointer(pointer_type) => match pointer_type.size {
                        PointerSize::C => Self::CPtr,
                        _ => Self::Ptr,
                    },
                    InternedType::Array(_) => Self::Array,
                    InternedType::Vector(_) => Self::Vector,
                    InternedType::Optional(_) => Self::Optional,
                    InternedType::ErrorSet(_) => Self::ErrorSet,
                    InternedType::ErrorUnion(_, _) => Self::ErrorUnion,
                    InternedType::Tuple(_) => Self::Tuple,
                    InternedType::Container(container_type) => {
                        let node = container_type.this();
                        let tree = node.handle().tree();
                        match tree.token_tag(tree.node_main_token(node.index())) {
                            TokenTag::KeywordEnum | TokenTag::KeywordUnion => Self::EnumOrUnion,
                            TokenTag::KeywordOpaque | TokenTag::KeywordStruct => Self::Exact,
                            _ => unreachable!(),
                        }
                    }
                    InternedType::Function(_) => Self::Func,
                    InternedType::Branched(_) => {
                        // TODO: what does PTR with branching types mean?
                        Self::Exact
                    }
                }
            }
        }
    }
}
