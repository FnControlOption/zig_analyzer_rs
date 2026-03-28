use std::fmt::{Display, Formatter, Result};

use super::*;

impl Expr {
    pub fn display(self, ip: &InternPool) -> impl Display {
        ExprDisplay(self, ip)
    }
}

impl Type {
    pub fn display(self, ip: &InternPool) -> impl Display {
        TypeDisplay(self, ip)
    }
}

impl InternedType {
    pub fn display(&self, ip: &InternPool) -> impl Display {
        InternedTypeDisplay(self, ip)
    }
}

impl Value {
    pub fn display(self, ip: &InternPool) -> impl Display {
        ValueDisplay(self, ip)
    }
}

impl InternedValue {
    pub fn display(&self, ip: &InternPool) -> impl Display {
        InternedValueDisplay(self, ip)
    }
}

struct ExprDisplay<'ip>(Expr, &'ip InternPool);
struct TypeDisplay<'ip>(Type, &'ip InternPool);
struct InternedTypeDisplay<'ty, 'ip>(&'ty InternedType, &'ip InternPool);
struct ValueDisplay<'ip>(Value, &'ip InternPool);
struct InternedValueDisplay<'val, 'ip>(&'val InternedValue, &'ip InternPool);

impl Display for ExprDisplay<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let Self(expr, ip) = self;
        match expr {
            Expr(Type::Unknown, Value::Unknown) => f.write_str("[unknown]"),
            Expr(Type::Type, Value::Type(ty)) => ty.display(ip).fmt(f),
            Expr(ty, val) => write!(f, "@as({}, {})", ty.display(ip), val.display(ip)),
        }
    }
}

impl Display for TypeDisplay<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let &Self(ty, ip) = self;
        match ty {
            Type::Unknown => f.write_str("[unknown]"),
            Type::Type => f.write_str("type"),

            Type::Anyopaque => f.write_str("anyopaque"),
            Type::Bool => f.write_str("bool"),
            Type::ComptimeFloat => f.write_str("comptime_float"),
            Type::ComptimeInt => f.write_str("comptime_int"),
            Type::Noreturn => f.write_str("noreturn"),
            Type::Null => f.write_str("@TypeOf(null)"),
            Type::Undefined => f.write_str("@TypeOf(undefined)"),
            Type::Void => f.write_str("void"),

            Type::Float(bits) => write!(f, "f{bits}"),
            Type::Int(signedness, bits) => match signedness {
                Signedness::Signed => write!(f, "i{bits}"),
                Signedness::Unsigned => write!(f, "u{bits}"),
            },
            Type::Isize => f.write_str("isize"),
            Type::Usize => f.write_str("usize"),

            Type::EnumLiteral => f.write_str("@Type(.enum_literal)"),
            Type::UnionTag(_) => {
                // TODO: container names
                f.write_str(r#"@typeInfo(...).@"union".tag_type.?"#)
            }

            Type::Interned(index) => match ip.get_type(index) {
                Some(interned) => interned.display(ip).fmt(f),
                None => f.write_str("[interned]"),
            },
        }
    }
}

impl Display for InternedTypeDisplay<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let Self(interned, ip) = self;
        match interned {
            InternedType::Optional(child) => write!(f, "?{}", child.display(ip)),
            InternedType::Vector(child) => write!(f, "@Vector(?, {})", child.display(ip)),
            InternedType::Array(ArrayType { sentinel, elem }) => {
                f.write_str("[?")?;
                if let Some(value) = sentinel {
                    write!(f, ":{}", value.display(ip))?;
                }
                f.write_str("]")?;
                elem.display(ip).fmt(f)
            }
            InternedType::Pointer(pointer_type) => {
                if pointer_type.size == PointerSize::One {
                    f.write_str("*")?;
                } else {
                    f.write_str("[")?;
                    match pointer_type.size {
                        PointerSize::One => unreachable!(),
                        PointerSize::Many => f.write_str("*")?,
                        PointerSize::Slice => {}
                        PointerSize::C => f.write_str("*c")?,
                    }
                    if let Some(value) = pointer_type.sentinel {
                        write!(f, ":{}", value.display(ip))?;
                    }
                    f.write_str("]")?;
                }
                if pointer_type.is_allowzero {
                    f.write_str("allowzero ")?;
                }
                if pointer_type.has_align {
                    f.write_str("align(?")?;
                    if pointer_type.has_bit_range_start {
                        f.write_str(":?")?;
                    }
                    if pointer_type.has_bit_range_end {
                        f.write_str(":?")?;
                    }
                    f.write_str(") ")?;
                }
                if pointer_type.has_addrspace {
                    f.write_str("addrspace(?) ")?;
                }
                if pointer_type.is_const {
                    f.write_str("const ")?;
                }
                if pointer_type.is_volatile {
                    f.write_str("volatile ")?;
                }
                pointer_type.child.display(ip).fmt(f)
            }
            InternedType::ErrorSet(names) => {
                f.write_str("error{")?;
                for (i, name) in names.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    f.write_str(&String::from_utf8_lossy(name))?;
                }
                f.write_str("}")
            }
            InternedType::ErrorUnion(lhs, rhs) => {
                write!(f, "{}!{}", lhs.display(ip), rhs.display(ip))
            }
            InternedType::Function(fn_type) => {
                f.write_str("fn (")?;
                for (i, ty) in fn_type.params.iter().enumerate() {
                    if i > 0 {
                        f.write_str(",")?;
                    }
                    ty.display(ip).fmt(f)?;
                }
                f.write_str(") ")?;
                fn_type.return_type.display(ip).fmt(f)
            }
            InternedType::Tuple(types) => {
                if types.is_empty() {
                    f.write_str("@TypeOf(.{})")
                } else {
                    f.write_str("struct { ")?;
                    for (i, ty) in types.iter().enumerate() {
                        if i > 0 {
                            f.write_str(",")?;
                        }
                        ty.display(ip).fmt(f)?;
                    }
                    f.write_str(" }")
                }
            }
            InternedType::Container(container_type) => {
                // TODO: container names
                let source = container_type.source();
                if source.contains(&b'\n') {
                    f.write_str("[container]")
                } else {
                    f.write_str(&String::from_utf8_lossy(source))
                }
            }
            InternedType::Branched(_) => f.write_str("[branched]"),
        }
    }
}

impl Display for ValueDisplay<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let &Self(val, ip) = self;
        match val {
            Value::Unknown => f.write_str("[unknown value]"),
            Value::Runtime => f.write_str("[runtime value]"),
            Value::Undefined => f.write_str("undefined"),
            Value::Void => f.write_str("{}"),
            Value::Null => f.write_str("null"),
            Value::False => f.write_str("false"),
            Value::True => f.write_str("true"),
            Value::Int(int) => write!(f, "{int}"),
            Value::Interned(index) => match ip.get_value(index) {
                Some(interned) => interned.display(ip).fmt(f),
                None => f.write_str("[interned value]"),
            },
            Value::Type(ty) => ty.display(ip).fmt(f),
        }
    }
}

impl Display for InternedValueDisplay<'_, '_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let Self(interned, ip) = self;
        let _ = ip;
        match interned {
            InternedValue::EnumLiteral(name) => write!(f, ".{}", String::from_utf8_lossy(name)),
            InternedValue::ErrorValue(name) => write!(f, "error.{}", String::from_utf8_lossy(name)),
        }
    }
}
