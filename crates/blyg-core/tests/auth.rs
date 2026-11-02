//! Signing in: the owner token, or the studio password (a session cookie
//! from `{blyg-url}/studio/login`), against the mock owner API.

mod common;

use blyg_core::api::auth::Credential;
use blyg_core::api::{Api, ConnectError, verify_connection};
use blyg_core::*;
use common::*;

fn password() -> Credential {
    Credential::Password(PASSWORD.into())
}

#[test]
fn a_password_signs_in_once_and_everything_syncs() {
    let env = Env::new();
    env.mock.state().no_bearer = true; // a stock server: cookie only
    let b = env.open_cred(
        SyncOptions {
            start_worker: false,
            ..fast()
        },
        password(),
    );
    let id = b
        .create_draft(Kind::Fragment, "signed in by password")
        .unwrap();
    b.sync_now().unwrap();
    let out = b.publish(&id, None).unwrap();
    assert_eq!(out.version, 1);
    let st = env.mock.state();
    assert_eq!(st.logins, 1, "one sign-in for the session");
    assert_eq!(st.items.len(), 1);
    assert!(st.log.iter().any(|l| l == "POST /studio/login"));
}

#[test]
fn an_ended_session_signs_in_again_without_bothering_anyone() {
    let env = Env::new();
    env.mock.state().no_bearer = true;
    let b = env.open_cred(
        SyncOptions {
            start_worker: false,
            ..fast()
        },
        password(),
    );
    b.sync_now().unwrap();
    // 30 days later, or the server's secret changed.
    env.mock.state().sessions.clear();
    let id = b
        .create_draft(Kind::Fragment, "after the session ended")
        .unwrap();
    b.sync_now().unwrap();
    assert!(b.item(&id).unwrap().server_id.is_some());
    assert_eq!(env.mock.state().logins, 2);
}

#[test]
fn the_connect_check_signs_in_and_names_a_wrong_password() {
    let env = Env::new();
    env.mock.state().no_bearer = true;
    assert_eq!(verify_connection(&env.mock.url, password()), Ok(0));
    assert_eq!(
        verify_connection(&env.mock.url, Credential::Password("nope".into())),
        Err(ConnectError::WrongPassword)
    );
    // A stock server ignores the token.
    assert_eq!(
        verify_connection(&env.mock.url, TOKEN),
        Err(ConnectError::WrongToken)
    );
}

#[test]
fn the_server_generation_provider_path_uses_the_same_sign_in() {
    let env = Env::new();
    env.mock.state().no_bearer = true;
    let api = Api::new(&env.mock.url, password());
    // No /generate in the mock: a 404 after signing in, not a 401.
    assert!(matches!(
        api.generate("NOPE", 0, Some(1)),
        Err(CoreError::NotFound)
    ));
    assert_eq!(env.mock.state().logins, 1);
}
