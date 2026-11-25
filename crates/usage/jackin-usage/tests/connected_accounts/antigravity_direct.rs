// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use jackin_core::{Clock, EnvSnapshot, HostPath, Provider, Secret, SecretBytes, StdFileSystem};
use jackin_testkit::{FakeClock, Method, Route, UsageServer};
use jackin_usage::accounts::{
    AccountPreferences, AccountRegistry, CandidateIdentity, ConnectionError, FailureKind,
    UsageProvider,
};
use jackin_usage::onboarding::{GoogleRegistrationFile, LoginError, LoginPrompt, OwnedLogin};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use super::{Result, Vault, db};

const CLIENT_ID: &str = "jackin-fixture-desktop.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "fixture-not-a-secret";
const FOREIGN_CLIENT: &str = "jackin-fixture-foreign.apps.googleusercontent.com";
const EMAIL: &str = "ultra@example.com";

fn agent() -> &'static str {
    match (cfg!(target_os = "macos"), cfg!(target_arch = "aarch64")) {
        (true, true) => "antigravity/hub/2.24.0 (aidev_client; os_type=darwin; arch=arm64)",
        (true, false) => "antigravity/hub/2.24.0 (aidev_client; os_type=darwin; arch=amd64)",
        (false, true) => "antigravity/hub/2.24.0 (aidev_client; os_type=linux; arch=arm64)",
        (false, false) => "antigravity/hub/2.24.0 (aidev_client; os_type=linux; arch=amd64)",
    }
}

fn challenge_slot() -> &'static Mutex<String> {
    static SLOT: OnceLock<Mutex<String>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(String::new()))
}

fn token_posts() -> &'static AtomicUsize {
    static POSTS: AtomicUsize = AtomicUsize::new(0);
    &POSTS
}

fn gate() -> &'static tokio::sync::Mutex<()> {
    static GATE: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn summary(buckets: &str) -> String {
    format!(r#"{{"groups":[{{"displayName":"Gemini","buckets":[{buckets}]}}]}}"#)
}

fn window(id: &str, fraction: &str, reset: &str) -> String {
    format!(r#"{{"bucketId":"{id}","remainingFraction":{fraction},"resetTime":"{reset}"}}"#)
}

fn marked(id: &str, fraction: &str, flag: &str) -> String {
    format!(r#"{{"bucketId":"{id}","remainingFraction":{fraction},"disabled":{flag}}}"#)
}

fn all_disabled() -> String {
    summary(
        &[
            marked("gemini-5h", "0.25", "true"),
            marked("gemini-weekly", "0.75", "true"),
            marked("3p-5h", "0.125", "true"),
            marked("3p-weekly", "0.875", "true"),
        ]
        .join(","),
    )
}

fn complete_body() -> String {
    let shared = [
        window("gemini-5h", "0.99965", "2027-03-02T14:00:00Z"),
        window("gemini-weekly", "0.9742", "2027-03-08T09:00:00Z"),
        window("3p-5h", "0.5", "2027-03-02T15:00:00Z"),
        window("3p-weekly", "0.4", "2027-03-08T09:00:00Z"),
    ]
    .join(",");
    format!(
        r#"{{"groups":[{{"displayName":"Gemini","buckets":[{shared}]}},{{"displayName":"Gemini 4 Argon","buckets":[{{"bucketId":"gemini-argon-wtu-weekly","window":"weekly","remainingFraction":0.8,"resetTime":"2027-03-09T00:00:00Z"}}]}}]}}"#
    )
}

fn load_body(tier: &str) -> String {
    format!(
        r#"{{"paidTier":{tier},"cloudaicompanionProject":"proj-1","currentTier":{{"name":"current-name"}}}}"#
    )
}

fn token_json(access: &str) -> Value {
    json!({"access_token": access, "refresh_token": "refresh-1", "expires_in": 3600})
}

fn pkce_token(body: &[u8]) -> Value {
    token_posts().fetch_add(1, Ordering::SeqCst);
    let text = std::str::from_utf8(body).unwrap_or("");
    let verifier = text.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == "code_verifier").then_some(value)
    });
    let challenge = challenge_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone();
    let matches = verifier.is_some_and(|verifier| {
        URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())) == challenge
            && text.contains("grant_type=authorization_code")
            && text.contains(CLIENT_ID)
            && text.contains(CLIENT_SECRET)
    });
    if matches {
        token_json("access-1")
    } else {
        json!({"error": "invalid_grant"})
    }
}

fn counted_refresh(body: &[u8]) -> Value {
    token_posts().fetch_add(1, Ordering::SeqCst);
    let text = std::str::from_utf8(body).unwrap_or("");
    if text.contains("grant_type=refresh_token") && text.contains(CLIENT_SECRET) {
        token_json("access-2")
    } else {
        json!({"error": "invalid_grant"})
    }
}

fn rotated_grant(body: &[u8]) -> Value {
    let text = std::str::from_utf8(body).unwrap_or("");
    if !text.contains("grant_type=refresh_token") || !text.contains(CLIENT_SECRET) {
        return json!({"error": "invalid_grant"});
    }
    let n = token_posts().fetch_add(1, Ordering::SeqCst) + 2;
    json!({
        "access_token": format!("access-{n}"),
        "refresh_token": format!("refresh-{n}"),
        "expires_in": 3600
    })
}

struct Fixture {
    server: UsageServer,
    service: OwnedLogin,
    path: std::path::PathBuf,
    vault: Arc<Vault>,
    clock: Arc<FakeClock>,
    _dir: tempfile::TempDir,
}

impl Fixture {
    async fn open() -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let client = dir.path().join("client.json");
        std::fs::write(
            &client,
            json!({"installed": {"client_id": CLIENT_ID, "client_secret": CLIENT_SECRET}})
                .to_string(),
        )?;
        let cwd = HostPath::parse(dir.path())?;
        let env = EnvSnapshot::from_pairs([(
            "JACKIN_GOOGLE_OAUTH_CLIENT_FILE".into(),
            client.display().to_string(),
        )]);
        let registration = GoogleRegistrationFile::from_environment(
            &env,
            &cwd,
            Arc::new(StdFileSystem::default()),
        )
        .ok_or("missing registration")?;
        let server = UsageServer::start().await?;
        let path = dir.path().join("usage/accounts.sqlite3");
        let vault = Arc::new(Vault::default());
        let clock = Arc::new(FakeClock::new());
        let clock_for_login = Arc::clone(&clock);
        let vault_for_login = Arc::clone(&vault);
        let clock_for_login: Arc<dyn Clock> = clock_for_login;
        let vault_for_login: Arc<dyn jackin_usage::accounts::CredentialVault> = vault_for_login;
        let service = OwnedLogin::new(
            path.clone(),
            server.endpoints(),
            clock_for_login,
            vault_for_login,
        )
        .with_google_registration(Some(registration));
        Ok(Self {
            server,
            service,
            path,
            vault,
            clock,
            _dir: dir,
        })
    }

    fn serve(&self, method: Method, path: &str, route: Route) {
        self.server
            .serve(Provider::Antigravity, method, path, route);
    }

    fn replace(&self, method: Method, path: &str, route: Route) {
        self.server
            .replace(Provider::Antigravity, method, path, route);
    }

    fn prepare_login(&self) {
        token_posts().store(0, Ordering::SeqCst);
        self.serve(
            Method::Post,
            "/token",
            Route::json(200, "{}").responding_with(pkce_token),
        );
        self.serve(
            Method::Get,
            "/userinfo",
            Route::json(
                200,
                &json!({"id": "subject-1", "email": "Ultra@example.com"}).to_string(),
            )
            .requires_header("authorization", "Bearer access-1"),
        );
        self.serve(
            Method::Post,
            "/production/v1internal:loadCodeAssist",
            Route::json(
                200,
                &load_body(r#"{"id":"g1-ultra-tier","name":"ignored"}"#),
            )
            .requires_header("user-agent", agent())
            .requires_json_body(json!({
                "metadata": {
                    "ideType": "ANTIGRAVITY",
                    "platform": "PLATFORM_UNSPECIFIED",
                    "pluginType": "GEMINI"
                }
            })),
        );
        self.serve(
            Method::Post,
            "/daily/v1internal:retrieveUserQuotaSummary",
            Route::json(200, &complete_body())
                .requires_header("authorization", "Bearer access-1")
                .requires_header("user-agent", agent())
                .requires_json_body(json!({"project": "proj-1"})),
        );
    }
}

async fn browser_prompt(
    service: &OwnedLogin,
    replace: Option<Uuid>,
) -> Result<(
    tokio::task::JoinHandle<std::result::Result<Uuid, LoginError>>,
    url::Url,
    tokio::sync::oneshot::Sender<Secret>,
)> {
    let service = service.clone();
    let cancel = CancellationToken::new();
    let (send, mut prompts) = tokio::sync::mpsc::channel(4);
    let task = tokio::spawn(async move {
        service
            .connect(UsageProvider::Antigravity, None, replace, send, &cancel)
            .await
    });
    let prompt = prompts.recv().await.ok_or("missing prompt")?;
    let LoginPrompt::Browser { url, paste } = prompt else {
        return Err(format!("expected browser consent, got {prompt:?}").into());
    };
    Ok((task, url, paste))
}

fn parameter(url: &url::Url, key: &str) -> Result<String> {
    Ok(url
        .query_pairs()
        .find(|(name, _)| name == key)
        .ok_or("missing parameter")?
        .1
        .into_owned())
}

async fn callback(port: u16, path: &str) -> Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await?;
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .await?;
    let mut response = String::new();
    stream.read_to_string(&mut response).await?;
    Ok(response)
}

fn assert_clean(text: &str) {
    assert!(
        !text.contains(CLIENT_SECRET),
        "error text exposed the client secret"
    );
    assert!(!text.contains("access-"), "error text exposed a token");
    assert!(
        !text.contains("refresh-"),
        "error text exposed a refresh token"
    );
}

#[tokio::test]
async fn browser_sign_in_checks_pkce_state_identity_and_duplicates() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    fixture.prepare_login();
    let (task, url, paste) = browser_prompt(&fixture.service, None).await?;
    assert_eq!(parameter(&url, "code_challenge_method")?, "S256");
    assert_eq!(parameter(&url, "client_id")?, CLIENT_ID);
    assert!(!url.as_str().contains(FOREIGN_CLIENT));
    assert!(url.as_str().contains("access_type=offline"));
    *challenge_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = parameter(&url, "code_challenge")?;
    let redirect = url::Url::parse(&parameter(&url, "redirect_uri")?)?;
    let port = redirect.port().ok_or("missing port")?;
    let state = parameter(&url, "state")?;
    assert!(
        callback(port, &format!("{}?code=no&state=wrong", redirect.path()))
            .await?
            .starts_with("HTTP/1.1 400")
    );
    paste
        .send(Secret::new(format!("pasted-code#{state}")))
        .map_err(|_| "paste closed")?;
    let id = task.await??;
    assert_eq!(token_posts().load(Ordering::SeqCst), 1);
    assert!(
        fixture.server.mismatches().is_empty(),
        "{:?}",
        fixture.server.mismatches()
    );
    let registry = db(&fixture.path, &fixture.vault)?;
    let account = registry
        .list()?
        .into_iter()
        .next()
        .ok_or("missing account")?;
    assert_eq!(account.id, id);
    assert_eq!(account.label, "Ultra@example.com");
    let (_, _, snapshot) = registry
        .snapshots()?
        .into_iter()
        .next()
        .ok_or("missing snapshot")?;
    let snapshot = snapshot.ok_or("empty snapshot")?;
    assert_eq!(snapshot["plan"], "Google AI Ultra");
    assert_eq!(
        snapshot["resources"]["gemini-5h"]["remaining_fraction"],
        0.99965
    );
    let (again, _, paste) = browser_prompt(&fixture.service, None).await?;
    paste
        .send(Secret::new("code#wrong-state".to_owned()))
        .map_err(|_| "paste closed")?;
    let error = again.await?;
    assert!(matches!(error, Err(LoginError::Browser(_))));
    assert_clean(&format!("{error:?}"));
    let (third, url, paste) = browser_prompt(&fixture.service, None).await?;
    *challenge_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = parameter(&url, "code_challenge")?;
    let state = parameter(&url, "state")?;
    paste
        .send(Secret::new(format!(
            "http://127.0.0.1/oauth-callback?code=again&state={state}"
        )))
        .map_err(|_| "paste closed")?;
    let duplicate = third.await?;
    assert!(matches!(
        duplicate,
        Err(LoginError::Store(ConnectionError::Duplicate))
    ));
    assert_eq!(registry.list()?.len(), 1);

    let mut prefs = AccountPreferences::default();
    prefs.pinned.insert("gemini-5h".into());
    registry.set_preferences(id, &prefs)?;
    registry.rename(id, "Personal")?;
    let (reconnect, url, paste) = browser_prompt(&fixture.service, Some(id)).await?;
    *challenge_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = parameter(&url, "code_challenge")?;
    let state = parameter(&url, "state")?;
    paste
        .send(Secret::new(format!("again#{state}")))
        .map_err(|_| "paste closed")?;
    assert_eq!(reconnect.await??, id);
    let account = registry
        .list()?
        .into_iter()
        .next()
        .ok_or("missing account")?;
    assert_eq!(account.id, id);
    assert_eq!(account.label, "Personal");
    assert!(account.preferences.pinned.contains("gemini-5h"));

    let path = fixture.path.clone();
    let endpoints = fixture.server.endpoints();
    let vault = Arc::clone(&fixture.vault);
    let clock = Arc::clone(&fixture.clock);
    let vault: Arc<dyn jackin_usage::accounts::CredentialVault> = vault;
    let clock: Arc<dyn Clock> = clock;
    fixture.replace(
        Method::Post,
        "/token",
        Route::json(200, &token_json("access-2").to_string()),
    );
    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(200, &complete_body()).requires_header("authorization", "Bearer access-1"),
    );
    drop(fixture.service);
    let restarted = OwnedLogin::new(path, endpoints, clock, vault);
    let row = restarted
        .with_registry(|db| db.list())
        .await?
        .into_iter()
        .next()
        .ok_or("missing restarted account")?;
    restarted.refresh(&row, &CancellationToken::new()).await?;
    Ok(())
}

#[tokio::test]
async fn identity_lookup_precedes_quota_and_cancel_stores_nothing() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    fixture.serve(
        Method::Post,
        "/token",
        Route::json(200, &token_json("access-1").to_string()),
    );
    fixture.serve(
        Method::Get,
        "/userinfo",
        Route::json(500, r#"{"error":"unavailable"}"#),
    );
    let (task, url, paste) = browser_prompt(&fixture.service, None).await?;
    let state = parameter(&url, "state")?;
    paste
        .send(Secret::new(format!("code#{state}")))
        .map_err(|_| "paste closed")?;
    let error = task
        .await?
        .expect_err("identity failure must stop the sign-in");
    assert_clean(&error.to_string());
    assert!(
        fixture.server.mismatches().is_empty(),
        "quota was requested before identity: {:?}",
        fixture.server.mismatches()
    );
    assert!(
        db(&fixture.path, &fixture.vault).is_err()
            || db(&fixture.path, &fixture.vault)?.list()?.is_empty()
    );

    let cancel = CancellationToken::new();
    let service = fixture.service.clone();
    let token = cancel.clone();
    let (send, mut prompts) = tokio::sync::mpsc::channel(2);
    let pending = tokio::spawn(async move {
        service
            .connect(UsageProvider::Antigravity, None, None, send, &token)
            .await
    });
    let prompt = prompts.recv().await.ok_or("missing prompt")?;
    assert!(matches!(prompt, LoginPrompt::Browser { .. }));
    cancel.cancel();
    assert!(matches!(pending.await?, Err(LoginError::Cancelled)));
    Ok(())
}

fn grant(access: &str, expires: i64) -> Result<SecretBytes> {
    Ok(SecretBytes::new(serde_json::to_vec(&json!({
        "product": "antigravity",
        "client_id": CLIENT_ID,
        "client_secret": CLIENT_SECRET,
        "access": access,
        "refresh": "refresh-1",
        "expires_at": expires,
    }))?))
}

fn quota_hits() -> &'static AtomicUsize {
    static HITS: AtomicUsize = AtomicUsize::new(0);
    &HITS
}

fn count_hit(_: &[u8]) -> Value {
    quota_hits().fetch_add(1, Ordering::SeqCst);
    Value::Null
}

fn production_quota_hits() -> &'static AtomicUsize {
    static HITS: AtomicUsize = AtomicUsize::new(0);
    &HITS
}

fn counted_production(_: &[u8]) -> Value {
    production_quota_hits().fetch_add(1, Ordering::SeqCst);
    serde_json::from_str(&complete_body()).unwrap_or(Value::Null)
}

fn seeded(fixture: &Fixture, email: &str, access: &str, expires: i64) -> Result<Uuid> {
    let mut registry = db(&fixture.path, &fixture.vault)?;
    let now = fixture.clock.now().0;
    Ok(registry.add_with_snapshot(
        &jackin_core::CriticalWrite::begin(),
        UsageProvider::Antigravity,
        "Personal",
        &CandidateIdentity::OAuth(Some(email.into())),
        &grant(access, expires)?,
        (now - 60, &json!({"plan":"Google AI Ultra","resources":{"gemini-5h":{"kind":"quota","unit":"percent","remaining_fraction":0.2,"window_seconds":18000,"resets_at":1803996000}}})),
    )?)
}

#[tokio::test]
async fn refresh_merges_fallback_buckets_and_retains_the_previous_reading() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let id = seeded(
        &fixture,
        EMAIL,
        "old-access",
        fixture.clock.now().0 + 10_000,
    )?;
    let account = db(&fixture.path, &fixture.vault)?
        .list()?
        .into_iter()
        .next()
        .ok_or("missing account")?;
    fixture.serve(
        Method::Post,
        "/production/v1internal:loadCodeAssist",
        Route::json(
            200,
            &load_body(r#"{"id":"unknown-tier","name":"Google AI Pro"}"#),
        ),
    );
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(
            200,
            &summary(&window("3p-5h", "0.6", "2027-03-02T10:00:00Z")),
        )
        .requires_json_body(json!({"project": "proj-1"})),
    );
    let production = summary(
        &[
            window("gemini-5h", "1", "2027-03-02T19:00:00Z"),
            window("gemini-weekly", "1", "2027-03-09T00:00:00Z"),
            window("3p-5h", "0.1", "2027-03-02T19:00:00Z"),
            window("3p-weekly", "0.8", "2027-03-09T00:00:00Z"),
        ]
        .join(","),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(200, &production),
    );
    fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await?;
    let stored = |registry: &AccountRegistry| -> Result<Value> {
        Ok(registry
            .snapshots()?
            .into_iter()
            .find(|(row, _, _)| row.id == id)
            .and_then(|(_, _, value)| value)
            .ok_or("missing snapshot")?)
    };
    let body = stored(&db(&fixture.path, &fixture.vault)?)?;
    assert_eq!(body["plan"], "Google AI Pro");
    assert_eq!(body["resources"]["3p-5h"]["remaining_fraction"], 0.6);
    assert_eq!(body["resources"]["gemini-5h"]["remaining_fraction"], 1.0);
    assert!(body["resources"].get("gemini-argon-wtu-weekly").is_none());
    let observed = db(&fixture.path, &fixture.vault)?
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .and_then(|(_, at, _)| at)
        .ok_or("missing time")?;

    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::status(503),
    );
    fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await?;
    let body = stored(&db(&fixture.path, &fixture.vault)?)?;
    assert_eq!(body["resources"]["3p-weekly"]["remaining_fraction"], 0.8);

    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(200, "{}"),
    );
    fixture.replace(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(200, "{}"),
    );
    let error = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("a malformed summary is not an empty success");
    assert_clean(&error.to_string());
    let (row, at, kept) = db(&fixture.path, &fixture.vault)?
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .ok_or("missing snapshot")?;
    assert_eq!(at, Some(observed));
    assert_eq!(
        kept.ok_or("empty")?["resources"]["3p-weekly"]["remaining_fraction"],
        0.8
    );
    let lines = jackin_usage::presentation::entries(&fixture.service)?;
    let details = lines
        .into_iter()
        .find(|entry| entry.id == id.to_string())
        .ok_or("missing details")?;
    assert!(details.details.iter().any(|line| line.contains("fetched")));
    assert!(
        details
            .details
            .iter()
            .any(|line| line.contains(&error.to_string()))
    );
    assert_eq!(row.id, id);
    fixture.clock.advance(std::time::Duration::from_secs(3601));
    fixture.replace(
        Method::Post,
        "/production/v1internal:loadCodeAssist",
        Route::json(
            200,
            r#"{"currentTier":{"name":"Antigravity Starter Quota"},"cloudaicompanionProject":"proj-1"}"#,
        ),
    );
    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(200, &complete_body()),
    );
    fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await?;
    let named = stored(&db(&fixture.path, &fixture.vault)?)?;
    assert_eq!(named["plan"], "Antigravity Starter Quota");
    assert_eq!(
        named["resources"]["gemini-5h"]["remaining_fraction"],
        0.99965
    );
    assert_eq!(
        db(&fixture.path, &fixture.vault)?.list()?[0].label,
        "Personal"
    );
    Ok(())
}

#[tokio::test]
async fn token_refusal_renews_once_and_subscription_refusal_does_not() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let now = fixture.clock.now().0;
    let id = seeded(&fixture, EMAIL, "old-access", now + 10_000)?;
    let account = db(&fixture.path, &fixture.vault)?.list()?.remove(0);
    token_posts().store(0, Ordering::SeqCst);
    fixture.serve(
        Method::Post,
        "/token",
        Route::json(200, "{}").responding_with(counted_refresh),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:loadCodeAssist",
        Route::json(200, &load_body(r#"{"id":"g1-ultra-tier","name":"x"}"#)),
    );
    let denied = r#"{"error":{"code":403,"status":"PERMISSION_DENIED","details":[{"reason":"SUBSCRIPTION_REQUIRED"}]}}"#;
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(403, denied).requires_json_body(json!({"project": "proj-1"})),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(403, denied),
    );
    let error = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("subscription");
    assert!(error.to_string().contains("subscription"), "{error}");
    assert_eq!(
        db(&fixture.path, &fixture.vault)?
            .failure(id)?
            .ok_or("missing failure")?
            .kind,
        FailureKind::Limitation
    );
    assert_eq!(token_posts().load(Ordering::SeqCst), 0);
    assert_clean(&error.to_string());

    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(403, r#"{"error":{"status":"PERMISSION_DENIED"}}"#),
    );
    fixture.replace(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(403, r#"{"error":{"status":"PERMISSION_DENIED"}}"#),
    );
    let denied = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("provider denial");
    let denial = denied.to_string();
    assert!(denial.contains("HTTP 403"), "{denial}");
    assert!(denial.contains("PERMISSION_DENIED"), "{denial}");
    assert!(!denial.contains("login was rejected"), "{denial}");
    assert!(!denial.contains("subscription"), "{denial}");
    assert_clean(&denial);
    assert_eq!(
        db(&fixture.path, &fixture.vault)?
            .failure(id)?
            .ok_or("missing failure")?
            .kind,
        FailureKind::Service
    );
    let kept: Value = serde_json::from_slice(
        db(&fixture.path, &fixture.vault)?
            .credential(id)?
            .1
            .expose(),
    )?;
    assert_eq!(kept["access"], "old-access");

    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(401, "{}").requires_header("authorization", "Bearer old-access"),
    );
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(200, &complete_body()).requires_header("authorization", "Bearer access-2"),
    );
    fixture.replace(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(401, "{}"),
    );
    token_posts().store(0, Ordering::SeqCst);
    fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await?;
    assert_eq!(token_posts().load(Ordering::SeqCst), 1);
    let saved = db(&fixture.path, &fixture.vault)?.credential(id)?.1;
    let parsed: Value = serde_json::from_slice(saved.expose())?;
    assert_eq!(parsed["access"], "access-2");
    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(401, "{}"),
    );
    fixture.replace(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(401, "{}"),
    );
    token_posts().store(0, Ordering::SeqCst);
    let second = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("a second refusal does not renew again");
    assert!(matches!(second, LoginError::Provider(_)));
    assert_eq!(token_posts().load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn quota_denial_reports_the_provider_reason_and_keeps_the_grant() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let id = seeded(
        &fixture,
        EMAIL,
        "old-access",
        fixture.clock.now().0 + 10_000,
    )?;
    let account = db(&fixture.path, &fixture.vault)?.list()?.remove(0);
    let denied = r#"{"error":{"code":403,"status":"PERMISSION_DENIED","details":[{"reason":"SERVICE_DISABLED"}]}}"#;
    fixture.serve(
        Method::Post,
        "/production/v1internal:loadCodeAssist",
        Route::json(403, denied).requires_header("user-agent", agent()),
    );
    fixture.serve(
        Method::Post,
        "/daily/v1internal:loadCodeAssist",
        Route::json(403, denied),
    );
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(403, denied).requires_header("user-agent", agent()),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(403, denied),
    );
    let error = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("quota denied");
    let message = error.to_string();
    assert_eq!(message, "HTTP 403 SERVICE_DISABLED");
    assert_clean(&message);
    let registry = db(&fixture.path, &fixture.vault)?;
    let saved: Value = serde_json::from_slice(registry.credential(id)?.1.expose())?;
    assert_eq!(saved["access"], "old-access");
    let (_, _, snapshot) = registry
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .ok_or("missing snapshot")?;
    assert_eq!(snapshot.ok_or("empty snapshot")?["plan"], "Google AI Ultra");
    let failure = registry.failure(id)?.ok_or("missing failure")?;
    assert_eq!(failure.kind, FailureKind::Limitation);
    assert_eq!(failure.retry_at, fixture.clock.now().0 + 3600);
    Ok(())
}

#[tokio::test]
async fn sign_in_keeps_the_grant_when_google_denies_the_quota() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let id = seeded(
        &fixture,
        EMAIL,
        "old-access",
        fixture.clock.now().0 + 10_000,
    )?;
    fixture.prepare_login();
    let denied = r#"{"error":{"code":403,"status":"PERMISSION_DENIED","details":[{"reason":"SERVICE_DISABLED"}]}}"#;
    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(403, denied)
            .requires_header("authorization", "Bearer access-1")
            .requires_header("user-agent", agent())
            .requires_json_body(json!({"project": "proj-1"})),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(403, denied),
    );
    let (reconnect, url, paste) = browser_prompt(&fixture.service, Some(id)).await?;
    *challenge_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = parameter(&url, "code_challenge")?;
    let state = parameter(&url, "state")?;
    paste
        .send(Secret::new(format!("again#{state}")))
        .map_err(|_| "paste closed")?;
    let error = reconnect.await?.expect_err("quota denied");
    let message = error.to_string();
    assert_eq!(message, "HTTP 403 SERVICE_DISABLED");
    assert_clean(&message);
    let registry = db(&fixture.path, &fixture.vault)?;
    let saved: Value = serde_json::from_slice(registry.credential(id)?.1.expose())?;
    assert_eq!(saved["access"], "access-1");
    let account = registry
        .list()?
        .into_iter()
        .find(|row| row.id == id)
        .ok_or("missing account")?;
    assert_eq!(account.label, "Personal");
    let (_, _, snapshot) = registry
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .ok_or("missing snapshot")?;
    assert_eq!(snapshot.ok_or("empty snapshot")?["plan"], "Google AI Ultra");
    Ok(())
}

#[tokio::test]
async fn sign_in_converts_a_status_line_row_even_when_the_first_quota_read_is_denied() -> Result<()>
{
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let now = fixture.clock.now().0;
    let id = super::antigravity::statusline_account(&fixture.service, EMAIL, now).await?;
    assert!(db(&fixture.path, &fixture.vault)?.is_statusline(id)?);
    fixture.prepare_login();
    let denied = r#"{"error":{"code":403,"status":"PERMISSION_DENIED","details":[{"reason":"SERVICE_DISABLED"}]}}"#;
    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(403, denied),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(403, denied),
    );
    let (reconnect, url, paste) = browser_prompt(&fixture.service, Some(id)).await?;
    *challenge_slot()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = parameter(&url, "code_challenge")?;
    let state = parameter(&url, "state")?;
    paste
        .send(Secret::new(format!("again#{state}")))
        .map_err(|_| "paste closed")?;
    let error = reconnect.await?.expect_err("quota denied");
    let message = error.to_string();
    assert_eq!(message, "HTTP 403 SERVICE_DISABLED");
    assert_clean(&message);
    let registry = db(&fixture.path, &fixture.vault)?;
    assert!(!registry.is_statusline(id)?);
    let saved: Value = serde_json::from_slice(registry.credential(id)?.1.expose())?;
    assert_eq!(saved["access"], "access-1");
    let failure = registry.failure(id)?.ok_or("missing failure")?;
    assert_eq!(failure.kind, FailureKind::Limitation);
    assert_eq!(failure.message, "HTTP 403 SERVICE_DISABLED");
    Ok(())
}

#[tokio::test]
async fn an_unsafe_first_quota_reason_uses_the_provider_status() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let id = seeded(
        &fixture,
        EMAIL,
        "old-access",
        fixture.clock.now().0 + 10_000,
    )?;
    let account = db(&fixture.path, &fixture.vault)?.list()?.remove(0);
    let denied = r#"{"error":{"code":403,"status":"PERMISSION_DENIED","details":[{"reason":"https://console.cloud.google.com/project/proj-1"},{"reason":"OTHER_CODE"}]}}"#;
    fixture.serve(
        Method::Post,
        "/production/v1internal:loadCodeAssist",
        Route::json(403, denied),
    );
    fixture.serve(
        Method::Post,
        "/daily/v1internal:loadCodeAssist",
        Route::json(403, denied),
    );
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(403, denied),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(403, denied),
    );
    let error = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("unsafe reason");
    let message = error.to_string();
    assert_eq!(message, "HTTP 403 PERMISSION_DENIED");
    assert!(!message.contains("OTHER_CODE"), "{message}");
    assert!(!message.contains("console.cloud.google.com"), "{message}");
    assert_clean(&message);
    let saved: Value = serde_json::from_slice(
        db(&fixture.path, &fixture.vault)?
            .credential(id)?
            .1
            .expose(),
    )?;
    assert_eq!(saved["access"], "old-access");
    Ok(())
}

#[tokio::test]
async fn retry_after_holds_at_most_an_hour_and_a_forced_refresh_keeps_it() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let now = fixture.clock.now().0;
    let id = seeded(&fixture, EMAIL, "old-access", now + 10_000)?;
    let account = db(&fixture.path, &fixture.vault)?.list()?.remove(0);
    fixture.serve(
        Method::Post,
        "/production/v1internal:loadCodeAssist",
        Route::json(200, &load_body(r#"{"id":"g1-ultra-tier","name":"x"}"#)),
    );
    quota_hits().store(0, Ordering::SeqCst);
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(429, "")
            .reply_header("retry-after", "90")
            .responding_with(count_hit),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(429, "")
            .reply_header("retry-after", "90")
            .responding_with(count_hit),
    );
    let error = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("rate limit");
    assert!(matches!(error, LoginError::Provider(_)));
    let failure = db(&fixture.path, &fixture.vault)?
        .failure(id)?
        .ok_or("missing hold")?;
    assert_eq!(failure.kind, FailureKind::RateLimited);
    assert_eq!(failure.retry_at, now + 90);
    let hits = quota_hits().load(Ordering::SeqCst);
    let again = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("hold remains");
    assert!(matches!(again, LoginError::Wait(_)));
    assert_eq!(quota_hits().load(Ordering::SeqCst), hits);
    fixture.clock.advance(std::time::Duration::from_secs(89));
    let during = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("89 seconds of a 90 second hold");
    assert!(matches!(during, LoginError::Wait(_)));
    assert_eq!(quota_hits().load(Ordering::SeqCst), hits);
    fixture.clock.advance(std::time::Duration::from_secs(2));
    let resumed = quota_hits().load(Ordering::SeqCst);
    let _after = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("the hold has elapsed");
    assert!(quota_hits().load(Ordering::SeqCst) > resumed);

    let other = seeded(&fixture, "other@example.com", "old-access", now + 100_000)?;
    let other_account = db(&fixture.path, &fixture.vault)?
        .list()?
        .into_iter()
        .find(|row| row.id == other)
        .ok_or("missing")?;
    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(429, "")
            .reply_header("retry-after", "7200")
            .responding_with(count_hit),
    );
    fixture.replace(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(429, "")
            .reply_header("retry-after", "7200")
            .responding_with(count_hit),
    );
    let capped = fixture
        .service
        .refresh(&other_account, &CancellationToken::new())
        .await
        .expect_err("capped");
    assert!(matches!(capped, LoginError::Provider(_)));
    let capped_at = fixture.clock.now().0;
    assert_eq!(
        db(&fixture.path, &fixture.vault)?
            .failure(other)?
            .ok_or("missing")?
            .retry_at,
        capped_at + 3600
    );
    fixture.clock.advance(std::time::Duration::from_secs(3599));
    let blocked = fixture
        .service
        .refresh(&other_account, &CancellationToken::new())
        .await
        .expect_err("the hour hold remains");
    assert!(matches!(blocked, LoginError::Wait(_)));
    fixture.clock.advance(std::time::Duration::from_secs(2));
    let before_resume = quota_hits().load(Ordering::SeqCst);
    let _resumed = fixture
        .service
        .refresh(&other_account, &CancellationToken::new())
        .await
        .expect_err("the hour hold elapsed");
    assert!(quota_hits().load(Ordering::SeqCst) > before_resume);
    let dated = seeded(&fixture, "dated@example.com", "old-access", now + 100_000)?;
    let dated_account = db(&fixture.path, &fixture.vault)?
        .list()?
        .into_iter()
        .find(|row| row.id == dated)
        .ok_or("missing dated account")?;
    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(429, "").reply_header("retry-after", "Wed, 01 Jan 2031 00:00:00 GMT"),
    );
    fixture.replace(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(429, "").reply_header("retry-after", "Wed, 01 Jan 2031 00:00:00 GMT"),
    );
    let dated_error = fixture
        .service
        .refresh(&dated_account, &CancellationToken::new())
        .await
        .expect_err("http date hold");
    assert!(matches!(dated_error, LoginError::Provider(_)));
    let dated_at = fixture.clock.now().0;
    assert_eq!(
        db(&fixture.path, &fixture.vault)?
            .failure(dated)?
            .ok_or("missing date hold")?
            .retry_at,
        dated_at + 3600
    );
    Ok(())
}

#[tokio::test]
async fn a_failed_early_renewal_keeps_the_usable_token() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let now = fixture.clock.now().0;
    let id = seeded(&fixture, EMAIL, "old-access", now + 60)?;
    let account = db(&fixture.path, &fixture.vault)?.list()?.remove(0);
    token_posts().store(0, Ordering::SeqCst);
    fixture.serve(
        Method::Post,
        "/token",
        Route::status(500).responding_with(counted_refresh),
    );
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(200, &complete_body()).requires_header("authorization", "Bearer old-access"),
    );
    fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await?;
    let saved = db(&fixture.path, &fixture.vault)?.credential(id)?.1;
    let parsed: Value = serde_json::from_slice(saved.expose())?;
    assert_eq!(parsed["access"], "old-access");
    assert_eq!(token_posts().load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn a_late_write_after_removal_is_rejected() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let now = fixture.clock.now().0;
    let id = seeded(&fixture, EMAIL, "old-access", now + 10_000)?;
    let account = db(&fixture.path, &fixture.vault)?.list()?.remove(0);
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(200, &complete_body()),
    );
    let held = fixture.server.hold(
        Provider::Antigravity,
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
    );
    let service = fixture.service.clone();
    let refresh =
        tokio::spawn(async move { service.refresh(&account, &CancellationToken::new()).await });
    held.arrived().await;
    db(&fixture.path, &fixture.vault)?.remove(id)?;
    held.release();
    let error = refresh
        .await?
        .expect_err("removed account rejects the late snapshot");
    assert!(matches!(error, LoginError::Store(ConnectionError::Changed)));
    assert!(db(&fixture.path, &fixture.vault)?.list()?.is_empty());
    Ok(())
}

#[tokio::test]
async fn connect_without_registration_stops_before_browser_consent() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("usage/accounts.sqlite3");
    let server = UsageServer::start().await?;
    let vault = Arc::new(Vault::default());
    let shared: Arc<dyn jackin_usage::accounts::CredentialVault> = Arc::<Vault>::clone(&vault);
    let service = OwnedLogin::new(
        path.clone(),
        server.endpoints(),
        Arc::new(FakeClock::new()),
        shared,
    );
    let cancel = CancellationToken::new();
    let (send, mut prompts) = tokio::sync::mpsc::channel(2);
    let pending = tokio::spawn(async move {
        service
            .connect(UsageProvider::Antigravity, None, None, send, &cancel)
            .await
    });
    let error = pending
        .await?
        .expect_err("an unauthorized registration must stop sign-in");
    assert_eq!(
        error.to_string(),
        "Google has not authorized Jackin to use the Antigravity installed-app OAuth client; use jackin account usage connect antigravity --status-line"
    );
    assert!(prompts.try_recv().is_err(), "sign-in opened a prompt");
    assert!(
        server.mismatches().is_empty(),
        "sign-in sent a request: {:?}",
        server.mismatches()
    );
    assert!(db(&path, &vault).is_err() || db(&path, &vault)?.list()?.is_empty());
    Ok(())
}

fn escaped_slash_grant(expires: i64) -> SecretBytes {
    let body = format!(
        r#"{{"access":"old-access","client_id":"{CLIENT_ID}","client_secret":"{CLIENT_SECRET}","expires_at":{expires},"product":"antigravity","refresh":"refresh\/1"}}"#,
    );
    SecretBytes::new(body.into_bytes())
}

fn accept_unescaped_slash(body: &[u8]) -> Value {
    let text = std::str::from_utf8(body).unwrap_or("");
    if text.contains("grant_type=refresh_token")
        && (text.contains("refresh%2F1") || text.contains("refresh/1"))
        && !text.contains("%5C")
        && !text.contains("\\/")
    {
        json!({
            "access_token": "access-2",
            "refresh_token": "refresh/2",
            "expires_in": 3600
        })
    } else {
        json!({"error": "invalid_grant"})
    }
}

fn saved_refresh(path: &std::path::Path, vault: &Arc<Vault>, id: Uuid) -> Result<String> {
    let saved = db(path, vault)?.credential(id)?.1;
    let parsed: Value = serde_json::from_slice(saved.expose())?;
    parsed["refresh"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "stored grant has no refresh token".into())
}

#[tokio::test]
async fn a_grant_with_an_escaped_slash_refreshes_and_the_stored_grant_reopens() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let now = fixture.clock.now().0;
    let mut registry = db(&fixture.path, &fixture.vault)?;
    let id = registry.add_with_snapshot(
        &jackin_core::CriticalWrite::begin(),
        UsageProvider::Antigravity,
        "Personal",
        &CandidateIdentity::OAuth(Some(EMAIL.into())),
        &escaped_slash_grant(now + 60),
        (now - 60, &json!({"plan":"Google AI Ultra","resources":{"gemini-5h":{"kind":"quota","unit":"percent","remaining_fraction":0.2,"window_seconds":18000}}})),
    )?;
    drop(registry);
    let account = db(&fixture.path, &fixture.vault)?.list()?.remove(0);
    fixture.serve(
        Method::Post,
        "/token",
        Route::json(200, "{}").responding_with(accept_unescaped_slash),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:loadCodeAssist",
        Route::json(200, &load_body(r#"{"id":"g1-ultra-tier","name":"x"}"#)),
    );
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(200, &complete_body()).requires_header("authorization", "Bearer access-2"),
    );
    fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await?;
    assert_eq!(
        saved_refresh(&fixture.path, &fixture.vault, id)?,
        "refresh/2"
    );
    let path = fixture.path.clone();
    let endpoints = fixture.server.endpoints();
    let vault = Arc::clone(&fixture.vault);
    let clock = Arc::clone(&fixture.clock);
    let vault: Arc<dyn jackin_usage::accounts::CredentialVault> = vault;
    let clock: Arc<dyn Clock> = clock;
    drop(fixture.service);
    let restarted = OwnedLogin::new(path, endpoints, clock, vault);
    let row = restarted
        .with_registry(|db| db.list())
        .await?
        .into_iter()
        .next()
        .ok_or("missing restarted account")?;
    restarted.refresh(&row, &CancellationToken::new()).await?;
    Ok(())
}

#[tokio::test]
async fn a_renewed_grant_is_stored_before_a_failed_quota_read() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let now = fixture.clock.now().0;
    let id = seeded(&fixture, EMAIL, "old-access", now + 60)?;
    let account = db(&fixture.path, &fixture.vault)?.list()?.remove(0);
    let seeded_at = db(&fixture.path, &fixture.vault)?
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .and_then(|(_, at, _)| at)
        .ok_or("missing seeded time")?;
    token_posts().store(0, Ordering::SeqCst);
    fixture.serve(
        Method::Post,
        "/token",
        Route::json(200, "{}").responding_with(rotated_grant),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:loadCodeAssist",
        Route::json(200, &load_body(r#"{"id":"g1-ultra-tier","name":"x"}"#)),
    );
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::status(500),
    );
    fixture.serve(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::status(500),
    );
    let held = fixture.server.hold(
        Provider::Antigravity,
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
    );
    let service = fixture.service.clone();
    let refresh =
        tokio::spawn(async move { service.refresh(&account, &CancellationToken::new()).await });
    held.arrived().await;
    assert_eq!(
        saved_refresh(&fixture.path, &fixture.vault, id)?,
        "refresh-2"
    );
    assert_eq!(token_posts().load(Ordering::SeqCst), 1);
    held.release();
    let error = refresh
        .await?
        .expect_err("a quota failure keeps the renewed grant");
    assert!(
        error.to_string().contains("temporarily unavailable"),
        "quota failure was {error}"
    );
    assert_clean(&error.to_string());
    assert_eq!(
        saved_refresh(&fixture.path, &fixture.vault, id)?,
        "refresh-2"
    );
    let (_, at, kept) = db(&fixture.path, &fixture.vault)?
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .ok_or("missing snapshot")?;
    assert_eq!(at, Some(seeded_at));
    assert_eq!(
        kept.ok_or("empty snapshot")?["resources"]["gemini-5h"]["remaining_fraction"],
        0.2
    );

    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(401, "{}"),
    );
    fixture.replace(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(401, "{}"),
    );
    let account = db(&fixture.path, &fixture.vault)?.list()?.remove(0);
    let refused = fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await
        .expect_err("a second refusal keeps the rotated grant");
    assert!(matches!(refused, LoginError::Provider(_)));
    assert_clean(&refused.to_string());
    assert_eq!(token_posts().load(Ordering::SeqCst), 2);
    assert_eq!(
        saved_refresh(&fixture.path, &fixture.vault, id)?,
        "refresh-3"
    );
    let (_, at, kept) = db(&fixture.path, &fixture.vault)?
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .ok_or("missing snapshot")?;
    assert_eq!(at, Some(seeded_at));
    assert_eq!(
        kept.ok_or("empty snapshot")?["resources"]["gemini-5h"]["remaining_fraction"],
        0.2
    );
    Ok(())
}

#[tokio::test]
async fn disabled_shared_windows_keep_the_daily_fraction() -> Result<()> {
    let turn = gate().lock().await;
    let _pin = std::ptr::from_ref(&turn);
    let fixture = Fixture::open().await?;
    let id = seeded(
        &fixture,
        EMAIL,
        "old-access",
        fixture.clock.now().0 + 10_000,
    )?;
    let account = db(&fixture.path, &fixture.vault)?.list()?.remove(0);
    fixture.serve(
        Method::Post,
        "/production/v1internal:loadCodeAssist",
        Route::json(200, &load_body(r#"{"id":"g1-ultra-tier","name":"x"}"#)),
    );
    let daily = summary(
        &[
            marked("gemini-5h", "1", "true"),
            marked("gemini-weekly", "1", "\"no\""),
            window("3p-5h", "0.5", "2027-03-02T15:00:00Z"),
            window("3p-weekly", "0.5", "2027-03-08T09:00:00Z"),
        ]
        .join(","),
    );
    fixture.serve(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(200, &daily),
    );
    production_quota_hits().store(0, Ordering::SeqCst);
    fixture.serve(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(200, "{}").responding_with(counted_production),
    );
    fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await?;
    let stored = db(&fixture.path, &fixture.vault)?
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .ok_or("missing snapshot")?;
    let observed = stored.1.ok_or("missing time")?;
    let body = stored.2.ok_or("empty snapshot")?;
    assert_eq!(body["resources"]["gemini-5h"]["remaining_fraction"], 1.0);
    assert_eq!(
        body["resources"]["gemini-weekly"]["remaining_fraction"],
        1.0
    );
    assert_eq!(body["resources"]["3p-weekly"]["remaining_fraction"], 0.5);
    assert!(body["resources"].get("gemini-argon-wtu-weekly").is_none());
    assert_eq!(production_quota_hits().load(Ordering::SeqCst), 0);

    fixture.clock.advance(std::time::Duration::from_secs(5));
    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(200, &all_disabled()),
    );
    fixture.replace(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(200, &all_disabled()),
    );
    fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await?;
    let (_, at, published) = db(&fixture.path, &fixture.vault)?
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .ok_or("missing snapshot")?;
    assert_eq!(at, Some(observed + 5));
    let published = published.ok_or("empty snapshot")?;
    assert_eq!(
        published["resources"]["gemini-5h"]["remaining_fraction"],
        0.25
    );
    assert_eq!(
        published["resources"]["gemini-weekly"]["remaining_fraction"],
        0.75
    );
    assert_eq!(published["resources"]["3p-5h"]["remaining_fraction"], 0.125);
    assert_eq!(
        published["resources"]["3p-weekly"]["remaining_fraction"],
        0.875
    );

    fixture.clock.advance(std::time::Duration::from_secs(5));
    fixture.replace(
        Method::Post,
        "/daily/v1internal:retrieveUserQuotaSummary",
        Route::json(200, r#"{"groups":[]}"#),
    );
    fixture.replace(
        Method::Post,
        "/production/v1internal:retrieveUserQuotaSummary",
        Route::json(200, r#"{"groups":[]}"#),
    );
    fixture
        .service
        .refresh(&account, &CancellationToken::new())
        .await?;
    let (_, at, emptied) = db(&fixture.path, &fixture.vault)?
        .snapshots()?
        .into_iter()
        .find(|(row, _, _)| row.id == id)
        .ok_or("missing snapshot")?;
    assert_eq!(at, Some(observed + 10));
    let resources = emptied.ok_or("empty snapshot")?["resources"].clone();
    let map = resources.as_object().ok_or("resources missing")?;
    assert!(map.is_empty());
    Ok(())
}
