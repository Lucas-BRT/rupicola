//! The `text_enum!` macro for enumerations stored as text.

use std::fmt;

/// Text that names none of an enumeration's values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownValue(pub String);

impl fmt::Display for UnknownValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown value `{}`", self.0)
    }
}

impl std::error::Error for UnknownValue {}

/// Declares a kebab-case enumeration stored as text: serde, `as_str`, `Display`, `FromStr`,
/// ordered as declared.
macro_rules! text_enum {
    ($(#[$meta:meta])* $name:ident { $($(#[$vmeta:meta])* $variant:ident => $text:literal,)+ }) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, ::serde::Deserialize, PartialEq, Eq, PartialOrd, Ord, ::serde::Serialize,
        )]
        pub enum $name {
            $($(#[$vmeta])* #[serde(rename = $text)] $variant,)+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The text of every value, in declaration order.
            pub const NAMES: &'static [&'static str] = &[$($text),+];

            /// The text stored in the database and used on the wire.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text,)+ }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::std::str::FromStr for $name {
            type Err = $crate::UnknownValue;

            fn from_str(text: &str) -> Result<Self, Self::Err> {
                match text {
                    $($text => Ok(Self::$variant),)+
                    other => Err($crate::UnknownValue(other.to_owned())),
                }
            }
        }
    };
}
