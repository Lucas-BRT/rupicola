//! Who is signed in, and what they may do. The backend owns accounts, sessions and
//! membership; the panel keeps the backend's session token in a cookie and asks the backend
//! on every request.

mod csrf;
mod errors;
mod session;

pub use csrf::{check_csrf, equal, same_origin};
pub use errors::api_error;
pub use session::{
    Credentials, Csrf, authenticate, change_password, login, login_page, logout, register,
    register_page, role, role_of, sign_out_others, unauthenticated,
};

/// A signed-in person, as the panel sees them for one request.
#[derive(Clone)]
pub struct Account {
    pub handle: String,
    /// Administers the whole backend.
    pub owner: bool,
    /// The backend's session token.
    pub session: String,
    pub csrf: String,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Owner,
    Reviewer,
    Reader,
}

impl From<rupicola_api::Role> for Role {
    fn from(role: rupicola_api::Role) -> Self {
        match role {
            rupicola_api::Role::Owner => Self::Owner,
            rupicola_api::Role::Reviewer => Self::Reviewer,
            rupicola_api::Role::Reader => Self::Reader,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::csrf::csrf_for;
    use super::session::{identify, role_in};
    use super::*;
    use crate::StatusCode;
    use rupicola_api::{Me, Project, TokenKind};
    use rupicola_client::{ClientError, MockApi};

    const SESSION: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    const OTHER_SESSION: &str = "fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210";
    const HANDLE: &str = "ada";
    const PROJECT: &str = "demo";
    const OTHER_PROJECT: &str = "elsewhere";

    fn me(kind: TokenKind, admin: bool) -> Me {
        Me {
            kind,
            handle: Some(HANDLE.to_owned()),
            admin,
            project: None,
        }
    }

    fn account(owner: bool) -> Account {
        Account {
            handle: HANDLE.to_owned(),
            owner,
            session: SESSION.to_owned(),
            csrf: csrf_for(SESSION),
        }
    }

    fn project(id: &str, role: rupicola_api::Role) -> Project {
        Project {
            id: id.to_owned(),
            label: id.to_owned(),
            created_at: 0,
            role: Some(role),
        }
    }

    #[tokio::test]
    async fn a_session_becomes_an_account_with_a_csrf_token_of_its_own() {
        let mut api = MockApi::new();
        api.expect_me()
            .times(1)
            .returning(|_| Ok(me(TokenKind::Session, true)));
        let account = identify(&api, SESSION).await.unwrap();
        assert_eq!(account.handle, HANDLE);
        assert!(account.owner);
        assert_eq!(account.session, SESSION);
        assert_eq!(account.csrf, csrf_for(SESSION));
        assert_ne!(csrf_for(SESSION), csrf_for(OTHER_SESSION));
        assert!(check_csrf(&account, &csrf_for(SESSION)).is_ok());
        assert_eq!(
            check_csrf(&account, &csrf_for(OTHER_SESSION)),
            Err(StatusCode::FORBIDDEN)
        );
    }

    #[tokio::test]
    async fn only_a_persons_session_is_an_account() {
        for kind in [TokenKind::Machine, TokenKind::Device] {
            let mut api = MockApi::new();
            api.expect_me().returning(move |_| Ok(me(kind, false)));
            assert_eq!(
                identify(&api, SESSION).await.err(),
                Some(StatusCode::UNAUTHORIZED)
            );
        }
    }

    #[tokio::test]
    async fn what_the_backend_says_becomes_a_status() {
        let mut api = MockApi::new();
        api.expect_me()
            .returning(|_| Err(ClientError::Unauthorized));
        assert_eq!(
            identify(&api, SESSION).await.err(),
            Some(StatusCode::UNAUTHORIZED)
        );
        let mut api = MockApi::new();
        api.expect_me()
            .returning(|_| Err(ClientError::Unavailable("down".to_owned())));
        assert_eq!(
            identify(&api, SESSION).await.err(),
            Some(StatusCode::BAD_GATEWAY)
        );
    }

    #[test]
    fn an_administrator_owns_every_project_without_membership() {
        assert_eq!(role_in(&[], &account(true), PROJECT), Ok(Role::Owner));
    }

    #[test]
    fn a_member_has_the_role_the_backend_gives_and_a_stranger_finds_nothing() {
        let projects = [project(PROJECT, rupicola_api::Role::Reviewer)];
        assert_eq!(
            role_in(&projects, &account(false), PROJECT),
            Ok(Role::Reviewer)
        );
        assert_eq!(
            role_in(&projects, &account(false), OTHER_PROJECT),
            Err(StatusCode::NOT_FOUND)
        );
    }
}
