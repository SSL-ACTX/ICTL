use crate::frontend::ast::{BuiltinType, TypeName};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type {
    Integer,
    Bool,
    String,
    Struct(HashMap<String, Type>),
    Topology(HashMap<String, Type>),
    Array(Box<Type>),
    Custom(String),
    Unknown,
}

impl Type {
    pub fn from_typename(type_name: &TypeName) -> Type {
        match type_name {
            TypeName::Builtin(b) => match b {
                BuiltinType::Integer => Type::Integer,
                BuiltinType::Bool => Type::Bool,
                BuiltinType::String => Type::String,
                BuiltinType::Struct => Type::Struct(HashMap::new()),
                BuiltinType::Topology => Type::Topology(HashMap::new()),
                BuiltinType::Array => Type::Array(Box::new(Type::Unknown)),
            },
            TypeName::Custom(name) => Type::Custom(name.clone()),
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Integer)
    }

    pub fn is_bool(&self) -> bool {
        matches!(self, Type::Bool)
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Type::String)
    }
}
