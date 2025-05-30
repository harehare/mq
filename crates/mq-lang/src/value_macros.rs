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
                    Self::Array(a) => format!(
                        "[{}]",
                        a.iter()
                            .map(|v| format!("{:?}", v))
                            .collect::<Vec<String>>()
                            .join(", ")
                    ),
                    Self::Markdown(m, ..) => m.to_string(),
                    Self::None => "".to_string(),
                    Self::Function(..) => "function".to_string(),
                    Self::NativeFunction(_) => "native_function".to_string(),
                    Self::Dict(map) => {
                        let items = map
                            .iter()
                            .map(|(k, v)| format!("\"{}\": {}", k, v.string()))
                            .collect::<Vec<String>>()
                            .join(", ");
                        format!("{{{}}}", items)
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
