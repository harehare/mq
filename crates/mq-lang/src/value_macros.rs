#[macro_export]
macro_rules! impl_value_display {
    ($type:ty) => {
        impl std::fmt::Display for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
                let value = match self {
                    Self::Number(n) => n.to_string(),
                    Self::Bool(b) => b.to_string(),
                    Self::String(s) => s.to_string(),
                    Self::Array(_) => self.string(),
                    Self::Markdown(m, ..) => m.to_string(),
                    Self::None => "".to_string(),
                    Self::Function(..) => "function".to_string(),
                    Self::NativeFunction(_) => "native_function".to_string(),
                    Self::Dict(_) => self.string(),
                };
                write!(f, "{}", value)
            }
        }
    };
}

/// Macro to implement Debug trait with common formatting logic
#[macro_export]
macro_rules! impl_value_debug {
    ($type:ty) => {
        impl std::fmt::Debug for $type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
                let v = match self {
                    Self::None => "None".to_string(),
                    a => a.string(),
                };
                write!(f, "{}", v)
            }
        }
    };
}

/// Macro to implement string method with common formatting logic
#[macro_export]
macro_rules! impl_value_string {
    ($type:ty) => {
        impl $type {
            fn string(&self) -> String {
                match self {
                    Self::Number(n) => n.to_string(),
                    Self::Bool(b) => b.to_string(),
                    Self::String(s) => format!(r#""{}""#, s),
                    Self::Array(a) => {
                        let mut s = String::new();
                        s.push('[');
                        for (i, v) in a.iter().enumerate() {
                            if i > 0 {
                                s.push_str(", ");
                            }
                            // This assumes that the Debug trait is implemented for the items
                            // and that it produces the desired string representation.
                            // If specific formatting is needed, this might need adjustment.
                            std::fmt::write(&mut s, format_args!("{:?}", v)).unwrap();
                        }
                        s.push(']');
                        s
                    }
                    Self::Markdown(m, ..) => m.to_string(),
                    Self::None => "".to_string(),
                    Self::Function(..) => "function".to_string(),
                    Self::NativeFunction(_) => "native_function".to_string(),
                    Self::Dict(map) => {
                        let mut s = String::new();
                        s.push('{');
                        for (i, (k, v)) in map.iter().enumerate() {
                            if i > 0 {
                                s.push_str(", ");
                            }
                            // Similar to Array, this assumes `string()` method on `v`
                            // produces the desired output.
                            // Also, k is directly used, ensure it doesn't need escaping if it can contain special chars.
                            std::fmt::write(&mut s, format_args!("\"{}\": {}", k, v.string())).unwrap();
                        }
                        s.push('}');
                        s
                    }
                }
            }
        }
    };
}

/// Macro to implement both Display and Debug traits
#[macro_export]
macro_rules! impl_value_formatting {
    ($type:ty) => {
        $crate::impl_value_display!($type);
        $crate::impl_value_debug!($type);
        $crate::impl_value_string!($type);
    };
}
