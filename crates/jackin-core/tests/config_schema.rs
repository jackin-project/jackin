// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use jackin_core::{
    Account, AccountName, Agent, CodexAuthMode, Config, ConfigEdit, ConfigError, CredentialSource,
    EnvName, GatewayEntry, GatewayName, HostPath, Platform, Provider, RoleName, RoleRev,
    RoleSource, RoleUrl, WorkspaceName,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const FULL: &str = r#"default_gateway = "jackin.local"

[gateways."lab.gw_2"]
openshell_workspace = "lab"
builder = "default"
platform = "linux/amd64"

[gateways."jackin.local"]
openshell_workspace = "default"
builder = "orbstack"
platform = "linux/arm64"

[gateways."jackin.local".docker_engine]
port = 18476
client_certs = "/Users/me/.config/jackin/docker/jackin-local"

[defaults.accounts]
codex = "work"
claude = "personal"
grok = "xai"

[accounts.work]
provider = "codex"
credential = { source = "codex-login", home = "/Users/me/.codex", chatgpt_account_id = "7a1f" }

[accounts.ci]
provider = "codex"
credential = { source = "op", reference = "op://Dev/OpenAI/credential" }

[accounts.personal]
provider = "claude"
credential = { source = "env", var = "ANTHROPIC_API_KEY" }

[accounts.xai]
provider = "grok"
credential = { source = "file", path = "/Users/me/.config/xai/key" }

[roles.sentinel]
source = { path = "/Users/me/Projects/jackin-sentinel" }
bindings = { codex = "work" }

[roles."smith.v2"]
source = { git = "ssh://git@example.com/smith-repo.git", rev = "feature/x" }

[env]
REGION = "eu"
GREETING = "hello world"

[workspaces.myproj]
path = "/Users/me/Projects/myproj"
docker = false
default_role = "sentinel"
roles = ["sentinel", "smith.v2"]
bindings = { codex = "work", claude = "personal" }

[workspaces.myproj.env]
REGION = "us"

[workspaces.myproj.role_env.sentinel]
REGION = "ap"
MODE = "fast"

[workspaces."my.project"]
path = "/Users/me/Projects/my \"quoted\" dir\\α"
docker = true
bindings = { grok = "xai" }
"#;

const GW: &str = "[gateways.g]\nopenshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n";
const ACC: &str =
    "[accounts.work]\nprovider = \"codex\"\ncredential.source = \"env\"\ncredential.var = \"K\"\n";
const CL: &str =
    "[accounts.cl]\nprovider = \"claude\"\ncredential.source = \"env\"\ncredential.var = \"K\"\n";
const ROLE: &str = "[roles.r]\nsource.path = \"/r\"\n";
const WS: &str = "[workspaces.w]\npath = \"/p\"\ndocker = false\n";

fn hp(text: &str) -> Result<HostPath, Box<dyn std::error::Error>> {
    Ok(HostPath::parse(Path::new(text))?)
}

fn model(text: &str) -> Result<toml::Table, Box<dyn std::error::Error>> {
    Ok(text.parse::<toml::Table>()?)
}

fn prune(table: toml::Table) -> toml::Table {
    table
        .into_iter()
        .filter_map(|(key, value)| match value {
            toml::Value::Table(inner) => {
                let inner = prune(inner);
                (!inner.is_empty()).then_some((key, toml::Value::Table(inner)))
            }
            other => Some((key, other)),
        })
        .collect()
}

fn label(error: &ConfigError) -> String {
    match error {
        ConfigError::Syntax { line, .. } => format!("syntax@{line}"),
        ConfigError::UnknownField { key } => format!("unknown:{key}"),
        ConfigError::MissingField { key } => format!("missing:{key}"),
        ConfigError::InvalidValue { key, .. } => format!("invalid:{key}"),
        ConfigError::DuplicateName { kind, name } => format!("duplicate:{kind}:{name}"),
        ConfigError::MissingReference { kind, name } => format!("reference:{kind}:{name}"),
        ConfigError::ProviderMismatch {
            account,
            expected,
            found,
        } => format!(
            "mismatch:{account}:{}:{}",
            expected.as_str(),
            found.as_str()
        ),
        ConfigError::CredentialNotAllowed { provider } => {
            format!("credential:{}", provider.as_str())
        }
        ConfigError::AccountInUse { account, .. } => format!("account-in-use:{account}"),
        ConfigError::RoleInUse { role, .. } => format!("role-in-use:{role}"),
        ConfigError::NoGateway => "no-gateway".to_owned(),
        ConfigError::AmbiguousGateway { names } => format!(
            "ambiguous:{}",
            names
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
        ConfigError::UnknownGateway(name) => format!("unknown-gateway:{name}"),
        ConfigError::NoWorkspaceForPath(path) => {
            format!("no-workspace:{}", path.as_path().display())
        }
        ConfigError::DuplicatePath { path, workspaces } => format!(
            "duplicate-path:{}:{}",
            path.as_path().display(),
            workspaces
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
        ConfigError::Fs(_) => "fs".to_owned(),
    }
}

fn failure(text: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
    match Config::parse(text) {
        Ok(_) => Err(format!("expected an error for {text:?}").into()),
        Err(error) => Ok((label(&error), error.to_string())),
    }
}

#[test]
fn config_parses_the_full_documented_file_and_exposes_every_value() -> TestResult {
    let config = Config::parse(FULL)?;
    assert_eq!(
        config.default_gateway().map(ToString::to_string),
        Some("jackin.local".to_owned())
    );
    let gateways: Vec<_> = config.gateways().collect();
    let names: Vec<String> = gateways.iter().map(|(name, _)| name.to_string()).collect();
    assert_eq!(names, ["jackin.local", "lab.gw_2"]);
    let local = gateways[0].1;
    assert_eq!(local.openshell_workspace.as_str(), "default");
    assert_eq!(local.builder.as_str(), "orbstack");
    assert_eq!(local.platform, Platform::LinuxArm64);
    let engine = local.docker_engine.as_ref().ok_or("engine")?;
    assert_eq!(engine.port.get(), 18476);
    assert_eq!(
        engine.client_certs,
        hp("/Users/me/.config/jackin/docker/jackin-local")?
    );
    assert_eq!(gateways[1].1.platform, Platform::LinuxAmd64);
    assert!(gateways[1].1.docker_engine.is_none());
    let accounts: Vec<String> = config.accounts().map(|a| a.name().to_string()).collect();
    assert_eq!(accounts, ["ci", "personal", "work", "xai"]);
    let by_name = |name: &str| -> Result<&Account, Box<dyn std::error::Error>> {
        config
            .account(&AccountName::parse(name)?)
            .ok_or_else(|| format!("account {name}").into())
    };
    let work = by_name("work")?;
    assert_eq!(work.provider(), Provider::Codex);
    assert_eq!(
        *work.credential(),
        CredentialSource::CodexLogin {
            home: hp("/Users/me/.codex")?,
            chatgpt_account_id: "7a1f".parse()?,
        }
    );
    assert_eq!(work.codex_auth_mode(), Some(CodexAuthMode::ChatgptLogin));
    let ci = by_name("ci")?;
    assert!(matches!(
        ci.credential(),
        CredentialSource::OnePassword { reference } if reference.to_string() == "op://Dev/OpenAI/credential"
    ));
    assert_eq!(ci.codex_auth_mode(), Some(CodexAuthMode::ApiKey));
    let personal = by_name("personal")?;
    assert_eq!(personal.provider(), Provider::Claude);
    assert_eq!(
        *personal.credential(),
        CredentialSource::Env {
            var: EnvName::parse("ANTHROPIC_API_KEY")?
        }
    );
    assert_eq!(personal.codex_auth_mode(), None);
    assert_eq!(
        *by_name("xai")?.credential(),
        CredentialSource::File {
            path: hp("/Users/me/.config/xai/key")?
        }
    );
    for (provider, expected) in [
        (Provider::Codex, Some("work")),
        (Provider::Claude, Some("personal")),
        (Provider::Grok, Some("xai")),
        (Provider::Cursor, None),
    ] {
        assert_eq!(
            config.default_account(provider).map(ToString::to_string),
            expected.map(str::to_owned),
            "{provider:?}"
        );
    }
    Ok(())
}

#[test]
fn env_at_three_scopes_and_the_allowed_roles_parse_into_the_config() -> TestResult {
    let config = Config::parse(FULL)?;
    let pairs = |table: &jackin_core::EnvTable| -> Vec<(String, String)> {
        table
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect()
    };
    assert_eq!(
        pairs(config.env()),
        [
            ("GREETING".to_owned(), "hello world".to_owned()),
            ("REGION".to_owned(), "eu".to_owned())
        ]
    );
    let myproj = config
        .workspace(&WorkspaceName::parse("myproj")?)
        .ok_or("myproj")?;
    assert_eq!(pairs(&myproj.env), [("REGION".to_owned(), "us".to_owned())]);
    let sentinel = RoleName::parse("sentinel")?;
    assert_eq!(
        myproj.role_env.get(&sentinel).map(pairs),
        Some(vec![
            ("MODE".to_owned(), "fast".to_owned()),
            ("REGION".to_owned(), "ap".to_owned())
        ])
    );
    assert!(myproj.roles.allows(&sentinel));
    assert!(myproj.roles.allows(&RoleName::parse("smith.v2")?));
    assert!(!myproj.roles.allows(&RoleName::parse("other")?));
    let quoted = config
        .workspace(&WorkspaceName::parse("my.project")?)
        .ok_or("my.project")?;
    assert_eq!(quoted.roles, jackin_core::AllowedRoles::All);
    assert!(quoted.env.is_empty() && quoted.role_env.is_empty());
    Ok(())
}

#[test]
fn config_roles_and_workspaces_keep_their_sources_and_bindings() -> TestResult {
    let config = Config::parse(FULL)?;
    let sentinel = config
        .role(&RoleName::parse("sentinel")?)
        .ok_or("sentinel")?;
    assert_eq!(
        sentinel.source,
        RoleSource::Path(hp("/Users/me/Projects/jackin-sentinel")?)
    );
    assert_eq!(
        sentinel
            .bindings
            .get(&Agent::Codex)
            .map(ToString::to_string),
        Some("work".to_owned())
    );
    let smith = config.role(&RoleName::parse("smith.v2")?).ok_or("smith")?;
    assert_eq!(
        smith.source,
        RoleSource::Git {
            url: RoleUrl::parse("ssh://git@example.com/smith-repo.git")?,
            rev: Some(RoleRev::parse("feature/x")?),
        }
    );
    assert!(smith.bindings.is_empty());
    let myproj = config
        .workspace(&WorkspaceName::parse("myproj")?)
        .ok_or("myproj")?;
    assert!(!myproj.docker);
    assert_eq!(
        myproj.default_role.as_ref().map(ToString::to_string),
        Some("sentinel".to_owned())
    );
    assert_eq!(myproj.bindings.len(), 2);
    let quoted = config
        .workspace(&WorkspaceName::parse("my.project")?)
        .ok_or("my.project")?;
    assert_eq!(quoted.path, hp("/Users/me/Projects/my \"quoted\" dir\\α")?);
    assert!(quoted.docker);
    assert!(quoted.default_role.is_none());
    assert_eq!(
        quoted
            .bindings
            .iter()
            .map(|(p, a)| (p.as_str(), a.to_string()))
            .collect::<Vec<_>>(),
        [("grok", "xai".to_owned())]
    );
    Ok(())
}

fn reversed_tables(text: &str) -> String {
    let mut blocks = text.trim_end().split("\n\n");
    let head = blocks.next().unwrap_or_default().to_owned();
    let mut rest: Vec<&str> = blocks.collect();
    rest.reverse();
    format!("{head}\n\n{}\n", rest.join("\n\n"))
}

fn gateway(
    workspace: &str,
    platform: Platform,
) -> Result<GatewayEntry, Box<dyn std::error::Error>> {
    Ok(GatewayEntry {
        openshell_workspace: workspace.parse()?,
        builder: "default".parse()?,
        platform,
        docker_engine: None,
    })
}

fn env_account(name: &str, provider: Provider) -> Result<Account, Box<dyn std::error::Error>> {
    Ok(Account::new(
        AccountName::parse(name)?,
        provider,
        CredentialSource::Env {
            var: EnvName::parse("K")?,
        },
    )?)
}

fn built(forward: bool) -> Result<Config, Box<dyn std::error::Error>> {
    let mut independent = vec![
        ConfigEdit::AddGateway {
            name: GatewayName::parse("g1")?,
            entry: gateway("one", Platform::LinuxArm64)?,
        },
        ConfigEdit::AddGateway {
            name: GatewayName::parse("g2")?,
            entry: gateway("two", Platform::LinuxAmd64)?,
        },
        ConfigEdit::AddAccount(env_account("a1", Provider::Codex)?),
        ConfigEdit::AddAccount(env_account("a2", Provider::Claude)?),
        ConfigEdit::AddRole {
            name: RoleName::parse("r1")?,
            source: RoleSource::Path(hp("/r1")?),
        },
    ];
    if !forward {
        independent.reverse();
    }
    independent.push(ConfigEdit::AddWorkspace {
        name: WorkspaceName::parse("w")?,
        workspace: jackin_core::Workspace {
            default_role: Some(RoleName::parse("r1")?),
            bindings: [(Provider::Claude, AccountName::parse("a2")?)].into(),
            ..jackin_core::Workspace::new(hp("/w")?, true)
        },
    });
    let mut config = Config::empty();
    for edit in independent {
        config.apply(edit)?;
    }
    Ok(config)
}

#[test]
fn config_render_round_trips_and_writes_the_documented_keys() -> TestResult {
    let config = Config::parse(FULL)?;
    let rendered = config.render();
    assert_eq!(Config::parse(&rendered)?, config);
    assert_eq!(Config::parse(&rendered)?.render(), rendered);
    assert_eq!(prune(model(&rendered)?), prune(model(FULL)?));
    let reordered = Config::parse(&reversed_tables(FULL))?;
    assert_eq!(reordered, config);
    assert_eq!(reordered.render(), rendered);
    assert_eq!(built(true)?.render(), built(false)?.render());
    Ok(())
}

fn unknown_cases() -> Vec<(&'static str, String, &'static str, &'static str)> {
    let engine = format!("{GW}[gateways.g.docker_engine]\nport = 1\nclient_certs = \"/c\"\n");
    vec![
        ("root", String::new(), "bogus = 1", "bogus"),
        (
            "gateway",
            GW.to_owned(),
            "builderr = \"x\"",
            "gateways.g.builderr",
        ),
        (
            "engine",
            engine,
            "prot = 2",
            "gateways.g.docker_engine.prot",
        ),
        (
            "defaults",
            "[defaults]\n".to_owned(),
            "bogus = 1",
            "defaults.bogus",
        ),
        (
            "default account provider",
            format!("{ACC}[defaults.accounts]\ncodex = \"work\"\n"),
            "fooai = \"work\"",
            "defaults.accounts.fooai",
        ),
        (
            "account",
            ACC.to_owned(),
            "note = \"x\"",
            "accounts.work.note",
        ),
        (
            "credential",
            ACC.to_owned(),
            "credential.tpyo = 1",
            "accounts.work.credential.tpyo",
        ),
        (
            "credential of another source",
            ACC.to_owned(),
            "credential.home = \"/h\"",
            "accounts.work.credential.home",
        ),
        ("role", ROLE.to_owned(), "color = \"red\"", "roles.r.color"),
        (
            "role source",
            ROLE.to_owned(),
            "source.tpyo = 1",
            "roles.r.source.tpyo",
        ),
        (
            "role binding with a provider key",
            format!("{CL}{ROLE}"),
            "bindings.claude = \"cl\"",
            "roles.r.bindings.claude",
        ),
        (
            "role binding with an unknown key",
            format!("{ACC}{ROLE}"),
            "bindings.fooai = \"work\"",
            "roles.r.bindings.fooai",
        ),
        (
            "workspace",
            WS.to_owned(),
            "extra = 1",
            "workspaces.w.extra",
        ),
        (
            "workspace binding with an unknown key",
            format!("{ACC}{WS}"),
            "bindings.fooai = \"work\"",
            "workspaces.w.bindings.fooai",
        ),
    ]
}

#[test]
fn config_rejects_an_unknown_key_at_every_level_naming_its_path() -> TestResult {
    for (name, context, injected, key) in unknown_cases() {
        Config::parse(&context).map_err(|e| format!("control {name}: {e}"))?;
        let (found, shown) = failure(&format!("{context}{injected}\n"))?;
        assert_eq!(found, format!("unknown:{key}"), "{name}");
        assert!(shown.contains(key), "{name}: {shown}");
    }
    Ok(())
}

fn gateway_invalid_cases() -> Vec<(&'static str, String, String)> {
    let gw = |lines: &str| format!("[gateways.g]\n{lines}");
    let engine = |lines: &str| format!("{GW}[gateways.g.docker_engine]\n{lines}");
    let full_gw =
        "openshell_workspace = \"default\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n";
    let case = |name, text: String, found: &str| (name, text, found.to_owned());
    vec![
        case(
            "no workspace",
            gw("builder = \"b\"\nplatform = \"linux/arm64\"\n"),
            "missing:gateways.g.openshell_workspace",
        ),
        case(
            "no builder",
            gw("openshell_workspace = \"default\"\nplatform = \"linux/arm64\"\n"),
            "missing:gateways.g.builder",
        ),
        case(
            "no platform",
            gw("openshell_workspace = \"default\"\nbuilder = \"b\"\n"),
            "missing:gateways.g.platform",
        ),
        case(
            "bad platform",
            gw(&full_gw.replace("arm64", "arm32")),
            "invalid:gateways.g.platform",
        ),
        case(
            "bad workspace",
            gw(&full_gw.replace("\"default\"", "\"Default\"")),
            "invalid:gateways.g.openshell_workspace",
        ),
        case(
            "port zero",
            engine("port = 0\nclient_certs = \"/c\"\n"),
            "invalid:gateways.g.docker_engine.port",
        ),
        case(
            "port too large",
            engine("port = 70000\nclient_certs = \"/c\"\n"),
            "invalid:gateways.g.docker_engine.port",
        ),
        case(
            "plaintext docker port",
            engine("port = 2375\nclient_certs = \"/c\"\n"),
            "invalid:gateways.g.docker_engine.port",
        ),
        case(
            "no certs",
            engine("port = 1\n"),
            "missing:gateways.g.docker_engine.client_certs",
        ),
        case(
            "relative certs",
            engine("port = 1\nclient_certs = \"rel/dir\"\n"),
            "invalid:gateways.g.docker_engine.client_certs",
        ),
    ]
}

fn other_invalid_cases() -> Vec<(&'static str, String, String)> {
    let acc = |lines: &str| format!("[accounts.work]\n{lines}");
    let cred = |credential: &str| format!("[accounts.work]\nprovider = \"codex\"\n{credential}");
    let ws = |lines: &str| format!("[workspaces.w]\n{lines}");
    let env = "credential.source = \"env\"\ncredential.var = \"K\"\n";
    let case = |name, text: String, found: &str| (name, text, found.to_owned());
    vec![
        case("no provider", acc(env), "missing:accounts.work.provider"),
        case(
            "bad provider",
            acc(&format!("provider = \"nope\"\n{env}")),
            "invalid:accounts.work.provider",
        ),
        case(
            "no credential",
            acc("provider = \"codex\"\n"),
            "missing:accounts.work.credential",
        ),
        case(
            "bad source",
            cred("credential.source = \"vault\"\n"),
            "invalid:accounts.work.credential.source",
        ),
        case(
            "no var",
            cred("credential.source = \"env\"\n"),
            "missing:accounts.work.credential.var",
        ),
        case(
            "bad var",
            cred("credential.source = \"env\"\ncredential.var = \"1BAD\"\n"),
            "invalid:accounts.work.credential.var",
        ),
        case(
            "bad reference",
            cred("credential.source = \"op\"\ncredential.reference = \"http://x/y\"\n"),
            "invalid:accounts.work.credential.reference",
        ),
        case(
            "no chatgpt id",
            cred("credential = { source = \"codex-login\", home = \"/h\" }\n"),
            "missing:accounts.work.credential.chatgpt_account_id",
        ),
        case(
            "relative home",
            cred(
                "credential = { source = \"codex-login\", home = \"rel\", chatgpt_account_id = \"1\" }\n",
            ),
            "invalid:accounts.work.credential.home",
        ),
        case(
            "bad account name",
            format!("[accounts.Bad_Name]\nprovider = \"codex\"\n{env}"),
            "invalid:accounts.Bad_Name",
        ),
        case(
            "role without source",
            "[roles.r]\n".to_owned(),
            "missing:roles.r.source",
        ),
        case(
            "relative role path",
            "[roles.r]\nsource.path = \"rel\"\n".to_owned(),
            "invalid:roles.r.source.path",
        ),
        case(
            "workspace path with a parent segment",
            ws("path = \"/p/../q\"\n"),
            "invalid:workspaces.w.path",
        ),
        case(
            "role path with a parent segment",
            "[roles.r]\nsource.path = \"/r/../s\"\n".to_owned(),
            "invalid:roles.r.source.path",
        ),
        case(
            "workspace without path",
            ws("docker = false\n"),
            "missing:workspaces.w.path",
        ),
        case(
            "relative workspace path",
            ws("path = \"rel\"\n"),
            "invalid:workspaces.w.path",
        ),
        case(
            "bad docker",
            ws("path = \"/p\"\ndocker = \"yes\"\n"),
            "invalid:workspaces.w.docker",
        ),
        case(
            "bad default role",
            ws("path = \"/p\"\ndefault_role = \"Bad Role\"\n"),
            "invalid:workspaces.w.default_role",
        ),
        case(
            "bad binding account",
            format!(
                "{ACC}{}",
                ws("path = \"/p\"\nbindings.codex = \"Bad Name\"\n")
            ),
            "invalid:workspaces.w.bindings.codex",
        ),
        case(
            "default gateway type",
            "default_gateway = 5\n".to_owned(),
            "invalid:default_gateway",
        ),
        case(
            "unclosed table",
            "default_gateway = \"g\"\n\n[gateways.g\n".to_owned(),
            "syntax@3",
        ),
        case(
            "duplicate table",
            "[accounts.work]\nprovider = \"codex\"\n[accounts.work]\n".to_owned(),
            "syntax@3",
        ),
    ]
}

fn env_invalid_cases() -> Vec<(&'static str, String, String)> {
    let ws = |lines: &str| format!("[workspaces.w]\n{lines}");
    let case = |name, text: String, found: &str| (name, text, found.to_owned());
    vec![
        case(
            "global env value that is a 1Password reference",
            "[env]\nREGION = \"op://Dev/Region/value\"\n".to_owned(),
            "invalid:env.REGION",
        ),
        case(
            "global env value that is not text",
            "[env]\nREGION = 5\n".to_owned(),
            "invalid:env.REGION",
        ),
        case(
            "global env value with a newline",
            "[env]\nREGION = \"a\\nb\"\n".to_owned(),
            "invalid:env.REGION",
        ),
        case(
            "global env name outside the alphabet",
            "[env]\n\"1BAD\" = \"x\"\n".to_owned(),
            "invalid:env.1BAD",
        ),
        case(
            "global env name that jackin reserves",
            "[env]\nDOCKER_HOST = \"x\"\n".to_owned(),
            "invalid:env.DOCKER_HOST",
        ),
        case(
            "workspace env value that is a 1Password reference",
            format!("{WS}[workspaces.w.env]\nA = \"op://x/y/z\"\n"),
            "invalid:workspaces.w.env.A",
        ),
        case(
            "workspace env name that jackin reserves",
            format!("{WS}[workspaces.w.env]\nJACKIN_LAUNCH_ID = \"x\"\n"),
            "invalid:workspaces.w.env.JACKIN_LAUNCH_ID",
        ),
        case(
            "role env value that is a 1Password reference",
            format!("{ROLE}{WS}[workspaces.w.role_env.r]\nA = \"op://x/y/z\"\n"),
            "invalid:workspaces.w.role_env.r.A",
        ),
        case(
            "role env that is not a table",
            format!("{ROLE}{WS}[workspaces.w.role_env]\nr = \"x\"\n"),
            "invalid:workspaces.w.role_env.r",
        ),
        case(
            "allowed roles that are empty",
            ws("path = \"/p\"\nroles = []\n"),
            "invalid:workspaces.w.roles",
        ),
        case(
            "allowed roles that are not a list",
            ws("path = \"/p\"\nroles = \"r\"\n"),
            "invalid:workspaces.w.roles",
        ),
        case(
            "allowed roles with a bad name",
            ws("path = \"/p\"\nroles = [\"Bad Role\"]\n"),
            "invalid:workspaces.w.roles",
        ),
        case(
            "allowed roles with a non-text item",
            ws("path = \"/p\"\nroles = [3]\n"),
            "invalid:workspaces.w.roles",
        ),
    ]
}

fn invalid_cases() -> Vec<(&'static str, String, String)> {
    let mut cases = gateway_invalid_cases();
    cases.extend(other_invalid_cases());
    cases.extend(env_invalid_cases());
    cases
}

#[test]
fn config_accepts_every_engine_port_except_the_plaintext_docker_port() -> TestResult {
    for port in [1, 2374, 2376, 12376, 65535] {
        let text =
            format!("{GW}[gateways.g.docker_engine]\nport = {port}\nclient_certs = \"/c\"\n");
        let config = Config::parse(&text)?;
        let engine = config
            .gateways()
            .find_map(|(_, entry)| entry.docker_engine.as_ref())
            .ok_or("no engine")?;
        assert_eq!(engine.port.get(), port);
    }
    let (_, shown) = failure(&format!(
        "{GW}[gateways.g.docker_engine]\nport = 2375\nclient_certs = \"/c\"\n"
    ))?;
    assert!(shown.contains("2375"), "{shown}");
    Ok(())
}

#[test]
fn config_rejects_invalid_values_missing_keys_and_syntax_naming_the_key() -> TestResult {
    for (name, text, expected) in invalid_cases() {
        let (found, shown) = failure(&text)?;
        assert_eq!(found, expected, "{name}");
        if let Some((_, key)) = expected.split_once(':') {
            assert!(shown.contains(key), "{name}: {shown}");
        }
    }
    Ok(())
}

#[test]
fn config_parse_enforces_the_cross_reference_invariants() -> TestResult {
    let context = format!("{GW}{ACC}{CL}{WS}");
    let ws_line = |line: &str| format!("{context}{line}\n");
    let defaults = |line: &str| format!("{context}[defaults.accounts]\n{line}\n");
    let role = |line: &str| format!("{context}[roles.r]\nsource.path = \"/r\"\n{line}\n");
    let cases = [
        ("default gateway", format!("default_gateway = \"ghost\"\n{context}"), "reference:gateway:ghost"),
        ("default account", defaults("codex = \"ghost\""), "reference:account:ghost"),
        ("default codex with claude", defaults("codex = \"cl\""), "mismatch:cl:codex:claude"),
        ("default claude with codex", defaults("claude = \"work\""), "mismatch:work:claude:codex"),
        ("workspace account", ws_line("bindings.codex = \"ghost\""), "reference:account:ghost"),
        ("workspace mismatch", ws_line("bindings.codex = \"cl\""), "mismatch:cl:codex:claude"),
        ("role account", role("bindings.codex = \"ghost\""), "reference:account:ghost"),
        ("role mismatch", role("bindings.codex = \"cl\""), "mismatch:cl:codex:claude"),
        ("default role", ws_line("default_role = \"ghost\""), "reference:role:ghost"),
        (
            "allowed role that is not configured",
            format!("{context}[roles.r]\nsource.path = \"/r\"\n[workspaces.w2]\npath = \"/q\"\nroles = [\"r\", \"ghost\"]\n"),
            "reference:role:ghost",
        ),
        (
            "default role outside the allowed roles",
            format!("{GW}[roles.r]\nsource.path = \"/r\"\n[roles.s]\nsource.path = \"/s\"\n[workspaces.w]\npath = \"/p\"\ndefault_role = \"s\"\nroles = [\"r\"]\n"),
            "invalid:workspaces.w.default_role",
        ),
        (
            "role env for a role that is not configured",
            format!("{context}[workspaces.w.role_env.ghost]\nA = \"x\"\n"),
            "reference:role:ghost",
        ),
        (
            "codex login on claude",
            "[accounts.x]\nprovider = \"claude\"\ncredential = { source = \"codex-login\", home = \"/h\", chatgpt_account_id = \"1\" }\n".to_owned(),
            "credential:claude",
        ),
    ];
    Config::parse(&context)?;
    for (name, text, expected) in cases {
        assert_eq!(failure(&text)?.0, expected, "{name}");
    }
    Ok(())
}

fn workspaces_at(paths: &[(&str, &str)]) -> String {
    paths
        .iter()
        .map(|(name, path)| format!("[workspaces.{name}]\npath = \"{path}\"\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn config_parse_rejects_workspaces_that_share_a_normalized_path() -> TestResult {
    let rejected = [
        (
            vec![("a", "/p/x"), ("b", "/p/x/")],
            "duplicate-path:/p/x:a,b",
        ),
        (
            vec![("a", "/p/x"), ("b", "/p/./x")],
            "duplicate-path:/p/x:a,b",
        ),
        (
            vec![("a", "/p/x"), ("b", "/p//x")],
            "duplicate-path:/p/x:a,b",
        ),
        (
            vec![("a", "/p/x"), ("b", "/p/x"), ("c", "/p/x")],
            "duplicate-path:/p/x:a,b,c",
        ),
    ];
    for (paths, expected) in rejected {
        let (found, shown) = failure(&workspaces_at(&paths))?;
        assert_eq!(found, expected, "{paths:?}");
        assert!(shown.contains("/p/x"), "{shown}");
        for (name, _) in &paths {
            assert!(shown.contains(name), "{shown}");
        }
    }
    for paths in [
        vec![("a", "/p/x"), ("b", "/p/y")],
        vec![("a", "/p/x"), ("b", "/p/x/sub")],
    ] {
        let config = Config::parse(&workspaces_at(&paths))?;
        assert_eq!(config.workspaces().count(), 2, "{paths:?}");
        let (name, _) = config.workspace_for(&hp(paths[0].1)?)?;
        assert_eq!(name.as_str(), "a");
    }
    Ok(())
}

#[test]
fn config_git_role_sources_are_typed_and_bad_ones_name_their_key() -> TestResult {
    let git = |source: &str| format!("[roles.r]\nsource = {{ {source} }}\n");
    let password = "ghp_Zq7Xv9Lm2";
    let rejected = [
        (
            git("git = \"-oProxyCommand=x\""),
            "invalid:roles.r.source.git",
        ),
        (
            git("git = \"https://example.com/r.git\", rev = \"--output=/tmp/x\""),
            "invalid:roles.r.source.rev",
        ),
        (
            git(&format!(
                "git = \"https://user:{password}@example.com/r.git\""
            )),
            "invalid:roles.r.source.git",
        ),
        (git("git = \"\""), "invalid:roles.r.source.git"),
    ];
    for (text, expected) in rejected {
        let (found, shown) = failure(&text)?;
        assert_eq!(found, expected, "{text}");
        for window in password.as_bytes().windows(3) {
            let window = std::str::from_utf8(window)?;
            assert!(!shown.contains(window), "{window} in {shown}");
        }
    }
    let text = git("git = \"https://example.com/r.git\", rev = \"v1.2\"");
    let config = Config::parse(&text)?;
    let role = config.role(&RoleName::parse("r")?).ok_or("r")?;
    assert_eq!(
        role.source,
        RoleSource::Git {
            url: RoleUrl::parse("https://example.com/r.git")?,
            rev: Some(RoleRev::parse("v1.2")?),
        }
    );
    assert_eq!(Config::parse(&config.render())?, config);
    Ok(())
}

#[test]
fn account_new_allows_codex_login_only_for_codex_and_derives_the_codex_auth_mode() -> TestResult {
    let login = || -> Result<CredentialSource, Box<dyn std::error::Error>> {
        Ok(CredentialSource::CodexLogin {
            home: hp("/h")?,
            chatgpt_account_id: "1".parse()?,
        })
    };
    let others = || -> Result<Vec<CredentialSource>, Box<dyn std::error::Error>> {
        Ok(vec![
            CredentialSource::Env {
                var: EnvName::parse("K")?,
            },
            CredentialSource::OnePassword {
                reference: "op://a/b".parse()?,
            },
            CredentialSource::File { path: hp("/k")? },
        ])
    };
    for provider in Provider::ALL {
        let name = AccountName::parse("a")?;
        let result = Account::new(name.clone(), provider, login()?);
        if provider == Provider::Codex {
            let account = result?;
            assert_eq!(account.codex_auth_mode(), Some(CodexAuthMode::ChatgptLogin));
            assert_eq!(*account.name(), name);
            assert_eq!(account.provider(), provider);
        } else {
            assert!(
                matches!(result, Err(ConfigError::CredentialNotAllowed { provider: p }) if p == provider),
                "{provider:?}"
            );
        }
        for credential in others()? {
            let account = Account::new(name.clone(), provider, credential.clone())?;
            assert_eq!(*account.credential(), credential);
            let mode = (provider == Provider::Codex).then_some(CodexAuthMode::ApiKey);
            assert_eq!(account.codex_auth_mode(), mode, "{provider:?}");
        }
    }
    Ok(())
}

#[test]
fn every_provider_can_hold_an_account_a_default_and_a_binding() -> TestResult {
    let mut text = String::from("[workspaces.w]\npath = \"/p\"\n");
    for provider in Provider::ALL {
        text.push_str(&format!(
            "bindings.{} = \"acct-{}\"\n",
            provider.as_str(),
            provider.as_str()
        ));
    }
    text.push_str("[defaults.accounts]\n");
    for provider in Provider::ALL {
        text.push_str(&format!(
            "{} = \"acct-{}\"\n",
            provider.as_str(),
            provider.as_str()
        ));
    }
    for provider in Provider::ALL {
        text.push_str(&format!(
            "[accounts.acct-{p}]\nprovider = \"{p}\"\ncredential = {{ source = \"env\", var = \"K\" }}\n",
            p = provider.as_str()
        ));
    }
    let config = Config::parse(&text)?;
    for provider in Provider::ALL {
        let expected = format!("acct-{}", provider.as_str());
        assert_eq!(
            config.default_account(provider).map(ToString::to_string),
            Some(expected.clone())
        );
        let account = config
            .account(&AccountName::parse(&expected)?)
            .ok_or("account")?;
        assert_eq!(account.provider(), provider);
    }
    assert_eq!(
        config.workspace_accounts(&WorkspaceName::parse("w")?).len(),
        14
    );
    assert_eq!(Config::parse(&config.render())?, config);
    Ok(())
}

fn gateway_text(names: &[&str], default: Option<&str>) -> String {
    let mut text = default.map_or(String::new(), |d| format!("default_gateway = \"{d}\"\n"));
    for name in names {
        text.push_str(&format!(
            "[gateways.{name}]\nopenshell_workspace = \"w\"\nbuilder = \"b\"\nplatform = \"linux/arm64\"\n"
        ));
    }
    text
}

fn resolved(text: &str, requested: Option<&str>) -> Result<String, Box<dyn std::error::Error>> {
    let config = Config::parse(text)?;
    let requested = requested.map(GatewayName::parse).transpose()?;
    Ok(match config.resolve_gateway(requested.as_ref()) {
        Ok((name, _)) => name.to_string(),
        Err(error) => label(&error),
    })
}

#[test]
fn resolve_gateway_follows_request_default_then_the_only_gateway() -> TestResult {
    assert_eq!(resolved("", None)?, "no-gateway");
    assert_eq!(resolved(&gateway_text(&["a"], None), None)?, "a");
    assert_eq!(resolved(&gateway_text(&["a", "b"], Some("a")), None)?, "a");
    assert_eq!(
        resolved(&gateway_text(&["b", "a"], None), None)?,
        "ambiguous:a,b"
    );
    let ambiguous = Config::parse(&gateway_text(&["a", "b"], None))?;
    let shown = ambiguous
        .resolve_gateway(None)
        .err()
        .ok_or("ambiguous")?
        .to_string();
    assert!(shown.contains('a') && shown.contains('b'), "{shown}");
    let two = gateway_text(&["a", "b"], Some("a"));
    assert_eq!(resolved(&two, Some("b"))?, "b");
    assert_eq!(resolved(&two, Some("ghost"))?, "unknown-gateway:ghost");
    Ok(())
}

#[test]
fn workspace_lookup_matches_by_exact_path_and_lists_bound_accounts() -> TestResult {
    let config = Config::parse(FULL)?;
    let (name, workspace) = config.workspace_for(&hp("/Users/me/Projects/myproj")?)?;
    assert_eq!((name.as_str(), workspace.docker), ("myproj", false));
    let (name, workspace) =
        config.workspace_for(&hp("/Users/me/Projects/my \"quoted\" dir\\α")?)?;
    assert_eq!((name.as_str(), workspace.docker), ("my.project", true));
    for path in ["/Users/me/Projects/myproj/src", "/Users/me/Projects"] {
        let error = config.workspace_for(&hp(path)?).err().ok_or(path)?;
        assert_eq!(label(&error), format!("no-workspace:{path}"));
        assert!(error.to_string().contains("jackin workspace add"));
    }
    let names = |workspace: &str| -> Result<Vec<String>, Box<dyn std::error::Error>> {
        Ok(config
            .workspace_accounts(&WorkspaceName::parse(workspace)?)
            .iter()
            .map(|account| account.name().to_string())
            .collect())
    };
    assert_eq!(names("myproj")?, ["personal", "work"]);
    assert_eq!(names("my.project")?, ["xai"]);
    assert!(names("ghost")?.is_empty());
    Ok(())
}

#[test]
fn config_errors_never_echo_a_pasted_secret() -> TestResult {
    let sentinel = "Zq7#Xv9!Lm2@Hk4wP";
    let chars: Vec<char> = sentinel.chars().collect();
    let windows: Vec<String> = chars.windows(3).map(|w| w.iter().collect()).collect();
    let cases = [
        format!(
            "[accounts.a]\nprovider = \"codex\"\ncredential = {{ source = \"env\", var = \"{sentinel}\" }}\n"
        ),
        format!(
            "[accounts.a]\nprovider = \"codex\"\ncredential = {{ source = \"op\", reference = \"{sentinel}\" }}\n"
        ),
        format!(
            "[accounts.a]\nprovider = \"codex\"\ncredential = {{ source = \"codex-login\", home = \"/h\", chatgpt_account_id = \"{sentinel}\" }}\n"
        ),
        format!("[accounts.a]\nprovider = \"{sentinel}\"\n"),
        format!("[accounts.a]\nprovider = \"codex\"\ncredential.var = {sentinel}\n"),
        format!("[roles.r]\nsource = {{ git = \"https://user:{sentinel}@host/r.git\" }}\n"),
        format!("[roles.r]\nsource = {{ git = \"https://h/r.git\", rev = \"--{sentinel}\" }}\n"),
    ];
    for text in cases {
        let error = Config::parse(&text).err().ok_or("expected an error")?;
        for shown in [error.to_string(), format!("{error:?}")] {
            for window in &windows {
                assert!(!shown.contains(window.as_str()), "{window} in {shown}");
            }
        }
    }
    Ok(())
}

#[test]
fn optional_config_keys_default_and_an_empty_file_is_the_empty_config() -> TestResult {
    assert_eq!(Config::parse("")?, Config::empty());
    assert_eq!(Config::parse("\n\n")?, Config::empty());
    assert_eq!(Config::parse(&Config::empty().render())?, Config::empty());
    let config = Config::parse("[workspaces.w]\npath = \"/p\"\n[roles.r]\nsource.path = \"/r\"\n")?;
    let workspace = config.workspace(&WorkspaceName::parse("w")?).ok_or("w")?;
    assert!(!workspace.docker);
    assert!(workspace.default_role.is_none());
    assert!(workspace.bindings.is_empty());
    let role = config.role(&RoleName::parse("r")?).ok_or("r")?;
    assert!(role.bindings.is_empty());
    let config = Config::parse(GW)?;
    let (_, entry) = config.resolve_gateway(None)?;
    assert!(entry.docker_engine.is_none());
    Ok(())
}
