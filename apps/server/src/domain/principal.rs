//! Who is calling, and what that lets them do in a project.

use super::ApiError;
use rupicola_api::{Role, TokenKind};

/// The verified owner of the bearer token on a request.
pub struct Principal {
    pub kind: TokenKind,
    pub handle: Option<String>,
    pub admin: bool,
    pub project: Option<String>,
    pub(crate) digest: String,
}

impl Principal {
    /// The person behind a session or machine token; devices act for no one.
    pub fn person(&self) -> Result<&str, ApiError> {
        match (self.kind, self.handle.as_deref()) {
            (TokenKind::Session | TokenKind::Machine, Some(handle)) => Ok(handle),
            _ => Err(ApiError::Forbidden),
        }
    }

    /// The handle of a signed-in person, for the routes only a person may use.
    pub fn signed_in(&self) -> Result<&str, ApiError> {
        match (&self.kind, self.handle.as_deref()) {
            (TokenKind::Session, Some(handle)) => Ok(handle),
            _ => Err(ApiError::Forbidden),
        }
    }

    /// The name written in the audit trail for what this caller does.
    pub fn actor(&self) -> &str {
        self.handle.as_deref().unwrap_or("device")
    }
}

/// What the caller may do in a project. A person gets their role (an administrator
/// is an owner everywhere); a device only reaches its own project and may only
/// publish. Anything else is [`ApiError::NotFound`], so private project ids do not
/// leak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Access {
    Person(Role),
    Device,
}

impl Access {
    /// Fails with [`ApiError::Forbidden`] unless a person holds at least `role`.
    pub fn require(self, role: Role) -> Result<(), ApiError> {
        match self {
            Self::Person(held) if held >= role => Ok(()),
            _ => Err(ApiError::Forbidden),
        }
    }

    /// The person's role, or `None` for a device.
    pub fn role(self) -> Option<Role> {
        match self {
            Self::Person(role) => Some(role),
            Self::Device => None,
        }
    }
}
