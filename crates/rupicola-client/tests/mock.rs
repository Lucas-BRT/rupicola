//! `MockApi` stands in for the backend wherever code takes an `Api`.

#![cfg(feature = "mock")]

use rupicola_api::{Credentials, SessionToken};
use rupicola_client::{Api, ClientError, MockApi};

const HANDLE: &str = "ada";
const PASSWORD: &str = "correct horse battery";
const TOKEN: &str = "mock-token";

async fn sign_in_through(api: &dyn Api) -> Result<String, ClientError> {
    let form = Credentials {
        handle: HANDLE.to_owned(),
        password: PASSWORD.to_owned(),
    };
    api.sign_in(&form).await.map(|session| session.token)
}

#[tokio::test]
async fn a_test_scripts_the_answer_and_the_call_is_checked() {
    let mut api = MockApi::new();
    api.expect_sign_in()
        .withf(|form| form.handle == HANDLE)
        .times(1)
        .returning(|_| {
            Ok(SessionToken {
                token: TOKEN.to_owned(),
                expires_at: 0,
            })
        });
    assert_eq!(sign_in_through(&api).await.unwrap(), TOKEN);
}

#[tokio::test]
async fn an_error_can_be_scripted_too() {
    let mut api = MockApi::new();
    api.expect_sign_in()
        .returning(|_| Err(ClientError::Unauthorized));
    assert_eq!(
        sign_in_through(&api).await.err(),
        Some(ClientError::Unauthorized)
    );
}
