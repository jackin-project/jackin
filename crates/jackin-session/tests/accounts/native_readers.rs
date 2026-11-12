// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use jackin_core::{CREDENTIAL_FILE_MAX_BYTES, HostPath, StdFileSystem};
use jackin_session::{
    CredentialError, KimiRegion, MinimaxHostLogin, MinimaxLoginStatus, MinimaxRegion,
    NativeCredentialError, NativeRefreshUnsupportedReason, read_kimi_host_login,
    read_mimo_host_login, read_minimax_host_login,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

use super::support::{TestResult, guard};

const KIMI_ACCESS: &str = "FAKE-KIMI-ACCESS-9d831";
const KIMI_REFRESH: &str = "FAKE-KIMI-REFRESH-72c40";
const KIMI_CONFIG_SECRET: &str = "FAKE-KIMI-CONFIG-516d2";
const MINIMAX_ACCESS: &str = "FAKE-MINIMAX-ACCESS-38af1";
const MINIMAX_REFRESH: &str = "FAKE-MINIMAX-REFRESH-60b92";
const MINIMAX_SUBJECT: &str = "FAKE-MINIMAX-SUBJECT-177ce";
const MINIMAX_ACCOUNT: &str = "FAKE-MINIMAX-ACCOUNT-288df";
const MINIMAX_LOGIN: &str = "FAKE-MINIMAX-LOGIN-399e0";
const MINIMAX_EMAIL: &str = "fake-minimax-identity@example.invalid";
const MIMO_KEY: &str = "FAKE-MIMO-KEY-4a011";
const GLOBAL_SLOT: &str = "kimi-code-env-0e4f99c69cc27850";
const EXPIRES_AT_MS: u64 = 1_800_000_000_123;

struct Files {
    fs: StdFileSystem,
    root: HostPath,
    _dir: TempDir,
}

impl Files {
    fn new() -> Result<Files, Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let root = HostPath::parse(&std::fs::canonicalize(dir.path())?)?;
        Ok(Files {
            fs: StdFileSystem::default(),
            root,
            _dir: dir,
        })
    }

    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.root.as_path().join(relative)
    }

    async fn write(&self, relative: &str, bytes: impl AsRef<[u8]>) -> TestResult {
        let path = self.path(relative);
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path, bytes).await?;
        Ok(())
    }

    async fn write_json(&self, relative: &str, value: &Value) -> TestResult {
        self.write(relative, value.to_string()).await
    }

    async fn make_dir(&self, relative: &str) -> TestResult {
        tokio::fs::create_dir_all(self.path(relative)).await?;
        Ok(())
    }

    async fn remove(&self, relative: &str) -> TestResult {
        tokio::fs::remove_file(self.path(relative)).await?;
        Ok(())
    }

    fn oversized() -> Vec<u8> {
        vec![b'x'; CREDENTIAL_FILE_MAX_BYTES + 1]
    }
}

fn kimi_token() -> Value {
    json!({
        "access_token": KIMI_ACCESS,
        "refresh_token": KIMI_REFRESH,
        "expires_at": 1_800_000_000_u64,
        "expires_in": 3600,
        "scope": "membership.read membership.write",
        "token_type": "Bearer"
    })
}

fn kimi_config(region: KimiRegion) -> String {
    match region {
        KimiRegion::MainlandCn => r#"[providers."managed:kimi-code"]
type = "kimi"
base_url = "https://api.kimi.com/coding/v1"
api_key = ""

[providers."managed:kimi-code".oauth]
storage = "file"
key = "oauth/kimi-code"
"#
        .to_owned(),
        KimiRegion::Global => format!(
            r#"[providers."managed:kimi-code"]
type = "kimi"
base_url = "https://api.kimi.ai/coding/v1"
api_key = ""

[providers."managed:kimi-code".oauth]
storage = "file"
key = "oauth/{GLOBAL_SLOT}"
oauth_host = "https://auth.kimi.ai"
"#
        ),
    }
}

fn merge(value: &mut Value, patch: &Value) {
    for (key, patch) in patch.as_object().into_iter().flatten() {
        value[key] = patch.clone();
    }
}

async fn seed_kimi(files: &Files, region: KimiRegion, config: &str, token: &Value) -> TestResult {
    files.write("config.toml", config).await?;
    let slot = match region {
        KimiRegion::MainlandCn => "kimi-code",
        KimiRegion::Global => GLOBAL_SLOT,
    };
    files
        .write_json(&format!("credentials/{slot}.json"), token)
        .await
}

fn assert_kimi_secret_free(error: &NativeCredentialError) {
    let text = format!("{error} {error:?}");
    for secret in [KIMI_ACCESS, KIMI_REFRESH, KIMI_CONFIG_SECRET] {
        assert!(!text.contains(secret), "{text}");
    }
}

#[tokio::test]
async fn kimi_reads_regional_grants_and_preserves_scope() -> TestResult {
    guard(async {
        for (region, scope) in [
            (KimiRegion::MainlandCn, "membership.read membership.write"),
            (KimiRegion::Global, ""),
        ] {
            let files = Files::new()?;
            let mut token = kimi_token();
            token["scope"] = json!(scope);
            seed_kimi(&files, region, &kimi_config(region), &token).await?;
            let login = read_kimi_host_login(&files.fs, &files.root).await?;
            assert_eq!(login.grant.access_token.expose(), KIMI_ACCESS);
            assert_eq!(login.grant.refresh_token.expose(), KIMI_REFRESH);
            assert_eq!(login.grant.expires_at, 1_800_000_000);
            assert_eq!(login.grant.expires_in, 3600);
            assert_eq!(login.metadata.region, region);
            assert_eq!(login.metadata.scope, scope);
            let debug = format!("{login:?}");
            assert!(!debug.contains(KIMI_ACCESS), "{debug}");
            assert!(!debug.contains(KIMI_REFRESH), "{debug}");
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn kimi_rejects_invalid_token_types_and_expiry_numbers() -> TestResult {
    guard(async {
        let overflowing: Value = serde_json::from_str("18446744073709551616")?;
        let invalid_numbers = [
            json!(0),
            json!(-1),
            json!("1800000000"),
            json!(1.5),
            overflowing,
        ];
        for field in ["expires_at", "expires_in"] {
            for number in &invalid_numbers {
                let files = Files::new()?;
                let mut token = kimi_token();
                token[field] = number.clone();
                seed_kimi(
                    &files,
                    KimiRegion::MainlandCn,
                    &kimi_config(KimiRegion::MainlandCn),
                    &token,
                )
                .await?;
                let error = read_kimi_host_login(&files.fs, &files.root)
                    .await
                    .unwrap_err();
                assert_kimi_secret_free(&error);
            }
        }
        let cases = [
            json!({"access_token": ""}),
            json!({"refresh_token": ""}),
            json!({"access_token": 7}),
            json!({"refresh_token": null}),
            json!({"token_type": "bearer"}),
            json!({"scope": ["membership.read"]}),
            json!({"scope": null}),
        ];
        for patch in cases {
            let files = Files::new()?;
            let mut token = kimi_token();
            merge(&mut token, &patch);
            seed_kimi(
                &files,
                KimiRegion::MainlandCn,
                &kimi_config(KimiRegion::MainlandCn),
                &token,
            )
            .await?;
            let error = read_kimi_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_kimi_secret_free(&error);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn kimi_rejects_host_slot_storage_and_static_key_mismatches() -> TestResult {
    guard(async {
        let valid = kimi_config(KimiRegion::Global);
        let cases = [
            valid.replace(
                "https://api.kimi.ai/coding/v1",
                "https://api.kimi.example/coding/v1",
            ),
            valid.replace("https://auth.kimi.ai", "https://auth.kimi.com"),
            valid.replace(GLOBAL_SLOT, "kimi-code"),
            valid.replace("type = \"kimi\"", "type = \"anthropic\""),
            valid.replace("storage = \"file\"", "storage = \"keyring\""),
            valid.replace("storage = \"file\"", "storage = \"database\""),
            valid.replace(
                "api_key = \"\"",
                &format!("api_key = \"{KIMI_CONFIG_SECRET}\""),
            ),
            valid.replace("api_key = \"\"", "api_key = 7"),
            valid.replace(
                "api_key = \"\"",
                "api_key = \"\"\napi_key_env = \"KIMI_API_KEY\"",
            ),
            valid.replace("base_url =", "base_url = 7\nignored ="),
        ];
        for config in cases {
            let files = Files::new()?;
            seed_kimi(&files, KimiRegion::Global, &config, &kimi_token()).await?;
            let error = read_kimi_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_kimi_secret_free(&error);
        }
        let files = Files::new()?;
        seed_kimi(&files, KimiRegion::Global, &valid, &kimi_token()).await?;
        files
            .write_json("credentials/kimi-code.json", &kimi_token())
            .await?;
        files
            .remove(&format!("credentials/{GLOBAL_SLOT}.json"))
            .await?;
        let error = read_kimi_host_login(&files.fs, &files.root)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            NativeCredentialError::Credential(CredentialError::LoginUnreadable { .. })
        ));
        assert_kimi_secret_free(&error);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn kimi_config_and_selected_token_reads_are_bounded_and_regular() -> TestResult {
    guard(async {
        let files = Files::new()?;
        let error = read_kimi_host_login(&files.fs, &files.root)
            .await
            .unwrap_err();
        assert_kimi_secret_free(&error);

        let files = Files::new()?;
        files
            .write("config.toml", format!("api_key = \"{KIMI_CONFIG_SECRET}"))
            .await?;
        let error = read_kimi_host_login(&files.fs, &files.root)
            .await
            .unwrap_err();
        assert_kimi_secret_free(&error);

        let files = Files::new()?;
        files.make_dir("config.toml").await?;
        let error = read_kimi_host_login(&files.fs, &files.root)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            NativeCredentialError::Credential(CredentialError::LoginNotRegular { .. })
        ));

        let files = Files::new()?;
        files.write("config.toml", Files::oversized()).await?;
        let error = read_kimi_host_login(&files.fs, &files.root)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            NativeCredentialError::Credential(CredentialError::LoginTooLarge { .. })
        ));

        for case in ["missing", "malformed", "nonregular", "oversized"] {
            let files = Files::new()?;
            files
                .write("config.toml", kimi_config(KimiRegion::MainlandCn))
                .await?;
            let token_path = "credentials/kimi-code.json";
            match case {
                "missing" => {}
                "malformed" => {
                    files
                        .write(token_path, format!("{{\"access_token\":\"{KIMI_ACCESS}\""))
                        .await?;
                }
                "nonregular" => files.make_dir(token_path).await?,
                "oversized" => files.write(token_path, Files::oversized()).await?,
                _ => unreachable!(),
            }
            let error = read_kimi_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_kimi_secret_free(&error);
        }
        Ok(())
    })
    .await
}

fn minimax_record_key(root: &HostPath, region: &str) -> String {
    let auth_home = root.as_path().join("auth");
    let mut hasher = Sha256::new();
    hasher.update(auth_home.to_string_lossy().as_bytes());
    hasher.update([0]);
    hasher.update(b"mcode-public");
    let account = URL_SAFE_NO_PAD.encode(hasher.finalize());
    format!("com.minimax.mcode.oauth.prod.{region}\0{account}")
}

fn minimax_credential() -> Value {
    json!({
        "schemaVersion": 1,
        "accessToken": MINIMAX_ACCESS,
        "refreshToken": MINIMAX_REFRESH,
        "tokenType": "Bearer",
        "clientId": "mcode-public",
        "scopes": ["agent.default", "agent.extra"],
        "audience": "agent-backend",
        "expiresAtMs": EXPIRES_AT_MS,
        "generation": 7,
        "subject": MINIMAX_SUBJECT,
        "accountId": MINIMAX_ACCOUNT,
        "loginEpoch": MINIMAX_LOGIN,
        "email": MINIMAX_EMAIL
    })
}

fn minimax_state(region: &str, status: &str) -> Value {
    json!({
        "schemaVersion": 2,
        "status": status,
        "storeKind": "file",
        "clientId": "mcode-public",
        "scopes": ["agent.default", "agent.extra"],
        "audience": "agent-backend",
        "buildEnv": "prod",
        "region": region,
        "expiresAtMs": EXPIRES_AT_MS,
        "generation": 7
    })
}

async fn write_minimax(
    files: &Files,
    region: &str,
    credential: &Value,
    state: &Value,
) -> Result<String, Box<dyn std::error::Error>> {
    let key = minimax_record_key(&files.root, region);
    let namespace = format!("auth/prod/{region}/mcode-public");
    files
        .write_json(
            &format!("{namespace}/auth.json"),
            &json!({"schemaVersion": 1, "records": {key.clone(): credential.clone()}}),
        )
        .await?;
    files
        .write_json(&format!("{namespace}/auth-state.json"), state)
        .await?;
    Ok(key)
}

async fn seed_minimax(
    files: &Files,
    region: &str,
    status: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    write_minimax(
        files,
        region,
        &minimax_credential(),
        &minimax_state(region, status),
    )
    .await
}

fn assert_minimax_secret_free(error: &NativeCredentialError, record_key: &str) {
    let text = format!("{error} {error:?}");
    for sensitive in [
        MINIMAX_ACCESS,
        MINIMAX_REFRESH,
        MINIMAX_SUBJECT,
        MINIMAX_ACCOUNT,
        MINIMAX_LOGIN,
        MINIMAX_EMAIL,
        record_key,
    ] {
        assert!(!text.contains(sensitive), "{text}");
    }
}

fn assert_minimax_login_secret_free(login: &MinimaxHostLogin, record_key: &str) {
    let debug = format!("{login:?}");
    for sensitive in [
        MINIMAX_ACCESS,
        MINIMAX_REFRESH,
        MINIMAX_SUBJECT,
        MINIMAX_ACCOUNT,
        MINIMAX_LOGIN,
        MINIMAX_EMAIL,
        record_key,
    ] {
        assert!(!debug.contains(sensitive), "{debug}");
    }
}

#[tokio::test]
async fn minimax_reads_cn_authenticated_and_en_expired_native_material() -> TestResult {
    guard(async {
        for (region, status, expected_region, expected_status) in [
            (
                "cn",
                "authenticated",
                MinimaxRegion::Cn,
                MinimaxLoginStatus::Authenticated,
            ),
            (
                "en",
                "expired",
                MinimaxRegion::En,
                MinimaxLoginStatus::Expired,
            ),
        ] {
            let files = Files::new()?;
            let key = seed_minimax(&files, region, status).await?;
            let login = read_minimax_host_login(&files.fs, &files.root).await?;
            assert_eq!(login.access_token.expose(), MINIMAX_ACCESS);
            assert_eq!(login.refresh_token.expose(), MINIMAX_REFRESH);
            assert_eq!(login.region, expected_region);
            assert_eq!(login.status, expected_status);
            assert_eq!(login.generation, 7);
            assert_eq!(login.scopes, ["agent.default", "agent.extra"]);
            assert_eq!(login.expires_at_ms, EXPIRES_AT_MS as f64);
            assert_minimax_login_secret_free(&login, &key);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn minimax_rejects_invalid_credential_and_state_contracts() -> TestResult {
    guard(async {
        let credential_cases = [
            json!({"schemaVersion": 2}),
            json!({"accessToken": ""}),
            json!({"refreshToken": ""}),
            json!({"tokenType": "Basic"}),
            json!({"clientId": "other-client"}),
            json!({"audience": "other-audience"}),
            json!({"scopes": ["agent.extra"]}),
            json!({"scopes": "agent.default"}),
            json!({"expiresAtMs": "1800000000123"}),
            json!({"generation": -1}),
            json!({"generation": 1.5}),
            json!({"generation": 9_007_199_254_740_992_u64}),
        ];
        for patch in credential_cases {
            let files = Files::new()?;
            let mut credential = minimax_credential();
            merge(&mut credential, &patch);
            let key = write_minimax(
                &files,
                "cn",
                &credential,
                &minimax_state("cn", "authenticated"),
            )
            .await?;
            let error = read_minimax_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_minimax_secret_free(&error, &key);
        }
        let state_cases = [
            json!({"schemaVersion": 1}),
            json!({"storeKind": "os-keyring"}),
            json!({"storeKind": "database"}),
            json!({"clientId": "other-client"}),
            json!({"audience": "other-audience"}),
            json!({"scopes": ["agent.extra"]}),
            json!({"expiresAtMs": "1800000000123"}),
            json!({"buildEnv": "dev"}),
            json!({"region": "en"}),
            json!({"generation": -1}),
            json!({"generation": 1.5}),
            json!({"generation": 9_007_199_254_740_992_u64}),
        ];
        for patch in state_cases {
            let files = Files::new()?;
            let mut state = minimax_state("cn", "authenticated");
            merge(&mut state, &patch);
            let key = write_minimax(&files, "cn", &minimax_credential(), &state).await?;
            let error = read_minimax_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_minimax_secret_free(&error, &key);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn minimax_rejects_nonterminal_states_and_all_record_mismatches() -> TestResult {
    guard(async {
        for status in [
            "ready",
            "valid",
            "anonymous",
            "authorizing",
            "refreshing",
            "scope_upgrade_required",
            "logging_out",
            "logout_pending",
            "error",
        ] {
            let files = Files::new()?;
            let key = seed_minimax(&files, "cn", status).await?;
            let error = read_minimax_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_minimax_secret_free(&error, &key);
        }
        for (credential_patch, state_patch) in [
            (json!({"generation": 6}), json!({})),
            (json!({"generation": 8}), json!({})),
            (json!({}), json!({"generation": 8})),
            (json!({}), json!({"expiresAtMs": EXPIRES_AT_MS + 1})),
            (json!({}), json!({"scopes": ["agent.default"]})),
            (json!({"scopes": ["agent.default"]}), json!({})),
        ] {
            let files = Files::new()?;
            let mut credential = minimax_credential();
            let mut state = minimax_state("cn", "authenticated");
            merge(&mut credential, &credential_patch);
            merge(&mut state, &state_patch);
            let key = write_minimax(&files, "cn", &credential, &state).await?;
            let error = read_minimax_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_minimax_secret_free(&error, &key);
        }
        Ok(())
    })
    .await
}

#[tokio::test]
async fn minimax_rejects_sensitive_pending_and_local_runtime_state() -> TestResult {
    guard(async {
        let patches = [
            json!({"accessToken": MINIMAX_ACCESS}),
            json!({"nested": {"refresh_token": MINIMAX_REFRESH}}),
            json!({"deviceCode": "FAKE-DEVICE-CODE"}),
            json!({"code-verifier": "FAKE-CODE-VERIFIER"}),
            json!({"authorization": {"leaseId": "pending"}}),
            json!({"refresh": {"id": "pending"}}),
            json!({"localRuntime": {"accessToken": MINIMAX_ACCESS}}),
        ];
        for patch in patches {
            let files = Files::new()?;
            let mut state = minimax_state("cn", "authenticated");
            merge(&mut state, &patch);
            let key = write_minimax(&files, "cn", &minimax_credential(), &state).await?;
            let error = read_minimax_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_minimax_secret_free(&error, &key);
        }
        let files = Files::new()?;
        let mut state = minimax_state("cn", "authenticated");
        merge(
            &mut state,
            &json!({"localRuntime": {"accessToken": MINIMAX_ACCESS}}),
        );
        let key = minimax_record_key(&files.root, "cn");
        files
            .write_json("auth/prod/cn/mcode-public/auth-state.json", &state)
            .await?;
        let error = read_minimax_host_login(&files.fs, &files.root)
            .await
            .unwrap_err();
        assert_minimax_secret_free(&error, &key);
        Ok(())
    })
    .await
}

async fn damage_minimax_file(files: &Files, target: &str, case: &str) -> TestResult {
    let file = match target {
        "credential" => "auth/prod/cn/mcode-public/auth.json",
        "state" => "auth/prod/cn/mcode-public/auth-state.json",
        _ => unreachable!(),
    };
    match case {
        "missing" => files.remove(file).await?,
        "malformed" => {
            files
                .write(file, format!("{{\"accessToken\":\"{MINIMAX_ACCESS}\""))
                .await?;
        }
        "nonregular" => {
            files.remove(file).await?;
            files.make_dir(file).await?;
        }
        "oversized" => files.write(file, Files::oversized()).await?,
        _ => unreachable!(),
    }
    Ok(())
}

#[tokio::test]
async fn minimax_rejects_missing_ambiguous_malformed_and_unbounded_files() -> TestResult {
    guard(async {
        let files = Files::new()?;
        let key = minimax_record_key(&files.root, "cn");
        let error = read_minimax_host_login(&files.fs, &files.root)
            .await
            .unwrap_err();
        assert_minimax_secret_free(&error, &key);

        for records in [json!({}), json!({"wrong-record": minimax_credential()})] {
            let files = Files::new()?;
            let key = minimax_record_key(&files.root, "cn");
            files
                .write_json(
                    "auth/prod/cn/mcode-public/auth.json",
                    &json!({"schemaVersion": 1, "records": records}),
                )
                .await?;
            files
                .write_json(
                    "auth/prod/cn/mcode-public/auth-state.json",
                    &minimax_state("cn", "authenticated"),
                )
                .await?;
            let error = read_minimax_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_minimax_secret_free(&error, &key);
        }

        let files = Files::new()?;
        let cn = seed_minimax(&files, "cn", "authenticated").await?;
        seed_minimax(&files, "en", "authenticated").await?;
        let error = read_minimax_host_login(&files.fs, &files.root)
            .await
            .unwrap_err();
        assert_minimax_secret_free(&error, &cn);

        for target in ["credential", "state"] {
            for case in ["missing", "malformed", "nonregular", "oversized"] {
                let files = Files::new()?;
                let key = seed_minimax(&files, "cn", "authenticated").await?;
                damage_minimax_file(&files, target, case).await?;
                let error = read_minimax_host_login(&files.fs, &files.root)
                    .await
                    .unwrap_err();
                assert_minimax_secret_free(&error, &key);
            }
        }
        Ok(())
    })
    .await
}

fn assert_mimo_secret_free(error: &NativeCredentialError) {
    let text = format!("{error} {error:?}");
    assert!(!text.contains(MIMO_KEY), "{text}");
}

#[tokio::test]
async fn mimo_api_key_returns_no_refresh_grant_instead_of_oauth() -> TestResult {
    guard(async {
        let files = Files::new()?;
        files
            .write_json(
                "auth.json",
                &json!({"xiaomi": {"type": "api", "key": MIMO_KEY}}),
            )
            .await?;
        let error = read_mimo_host_login(&files.fs, &files.root)
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            NativeCredentialError::NativeRefreshUnsupported(ref unsupported)
                if unsupported.reason == NativeRefreshUnsupportedReason::NoRefreshGrant
        ));
        assert_mimo_secret_free(&error);
        Ok(())
    })
    .await
}

#[tokio::test]
async fn mimo_rejects_missing_malformed_wrong_and_unbounded_records() -> TestResult {
    guard(async {
        for case in [
            json!({}),
            json!({"other": {"type": "api", "key": MIMO_KEY}}),
            json!({"xiaomi": {"type": "bearer", "key": MIMO_KEY}}),
            json!({"xiaomi": {"type": "api", "key": ""}}),
            json!({"xiaomi": {"type": "api", "key": 7}}),
            json!({"xiaomi": {"type": 7, "key": MIMO_KEY}}),
        ] {
            let files = Files::new()?;
            files.write_json("auth.json", &case).await?;
            let error = read_mimo_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_mimo_secret_free(&error);
        }

        let files = Files::new()?;
        let error = read_mimo_host_login(&files.fs, &files.root)
            .await
            .unwrap_err();
        assert_mimo_secret_free(&error);

        for case in ["malformed", "nonregular", "oversized"] {
            let files = Files::new()?;
            match case {
                "malformed" => {
                    files
                        .write("auth.json", format!("{{\"key\":\"{MIMO_KEY}\""))
                        .await?;
                }
                "nonregular" => files.make_dir("auth.json").await?,
                "oversized" => files.write("auth.json", Files::oversized()).await?,
                _ => unreachable!(),
            }
            let error = read_mimo_host_login(&files.fs, &files.root)
                .await
                .unwrap_err();
            assert_mimo_secret_free(&error);
        }
        Ok(())
    })
    .await
}
