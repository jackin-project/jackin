// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use crate::ConfigError;
use std::num::NonZeroU32;
use std::path::Path;

use anyhow::Context;
use toml_edit::DocumentMut;

use crate::persist::{acquire_config_write_lock, atomic_write, config_file_for_workspace_path};
use crate::versions::{CURRENT_CONFIG_VERSION, CURRENT_WORKSPACE_VERSION, LEGACY_VERSION};

pub type Migration = fn(&mut DocumentMut) -> crate::ConfigResult<()>;

#[expect(
    missing_debug_implementations,
    reason = "MigrationStep stores a function pointer; debug output would not add useful migration evidence."
)]
#[derive(Clone, Copy)]
pub struct MigrationStep {
    pub from: &'static str,

    pub to: &'static str,

    pub migrate: Migration,
}

pub const CONFIG_MIGRATIONS: &[MigrationStep] = &[
    MigrationStep {
        from: LEGACY_VERSION,
        to: "v1alpha1",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha1",
        to: "v1alpha2",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha2",
        to: "v1alpha3",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha3",
        to: "v1alpha4",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha4",
        to: "v1alpha5",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha5",
        to: "v1alpha6",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha6",
        to: "v1alpha7",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha7",
        to: "v1alpha8",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha8",
        to: "v1alpha9",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha9",
        to: "v1alpha10",
        migrate: strip_legacy_agent_tables,
    },
    MigrationStep {
        from: "v1alpha10",
        to: "v1alpha11",
        migrate: stamp_bootstrap_initialized,
    },
    MigrationStep {
        from: "v1alpha11",
        to: "v1alpha12",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha12",
        to: CURRENT_CONFIG_VERSION,
        migrate: migrate_config_op_breadcrumbs,
    },
];

pub const WORKSPACE_MIGRATIONS: &[MigrationStep] = &[
    MigrationStep {
        from: LEGACY_VERSION,
        to: "v1alpha1",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha1",
        to: "v1alpha2",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha2",
        to: "v1alpha3",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha3",
        to: "v1alpha4",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha4",
        to: "v1alpha5",
        migrate: migrate_workspace_op_account_to_refs,
    },
    MigrationStep {
        from: "v1alpha5",
        to: "v1alpha6",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha6",
        to: "v1alpha7",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha7",
        to: "v1alpha8",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha8",
        to: "v1alpha9",
        migrate: strip_legacy_agent_tables,
    },
    MigrationStep {
        from: "v1alpha9",
        to: "v1alpha10",
        migrate: noop_migration,
    },
    MigrationStep {
        from: "v1alpha10",
        to: CURRENT_WORKSPACE_VERSION,
        migrate: migrate_workspace_op_breadcrumbs,
    },
];

pub(crate) fn stamp_bootstrap_initialized(doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    use toml_edit::Item;
    let has_marker = doc
        .get("bootstrap")
        .and_then(Item::as_table_like)
        .and_then(|table| table.get("fresh_install"))
        .and_then(Item::as_bool)
        .unwrap_or(false);
    if has_marker {
        return Ok(());
    }
    let mut table = toml_edit::Table::new();
    table.insert(
        "version",
        toml_edit::value(i64::from(crate::BOOTSTRAP_VERSION)),
    );
    table.insert("fresh_install", toml_edit::value(false));
    doc.insert("bootstrap", Item::Table(table));
    Ok(())
}

pub(crate) fn strip_legacy_agent_tables(doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    for agent in jackin_core::Agent::ALL {
        doc.remove(agent.slug());
    }
    if let Some(roles) = doc
        .get_mut("roles")
        .and_then(toml_edit::Item::as_table_like_mut)
    {
        for (_, role) in roles.iter_mut() {
            if let Some(role_tbl) = role.as_table_like_mut() {
                for agent in jackin_core::Agent::ALL {
                    role_tbl.remove(agent.slug());
                }
            }
        }
    }
    Ok(())
}

pub fn migrate_workspace_op_account_to_refs(doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    let acct = match doc.get("op_account") {
        None => return Ok(()),
        Some(item) => match item.as_str() {
            Some(s) => s.to_owned(),
            None => {
                return Err(ConfigError::msg(format_args!(
                    "workspace migration v1alpha4 → v1alpha5: `op_account` must be a string, \
                     found {item:?}"
                )));
            }
        },
    };

    stamp_account_in_env(doc.as_table_mut(), &acct);
    if let Some(roles) = doc.get_mut("roles").and_then(toml_edit::Item::as_table_mut) {
        for (_, role) in roles.iter_mut() {
            if let Some(role_tbl) = role.as_table_mut() {
                stamp_account_in_env(role_tbl, &acct);
            }
        }
    }

    doc.remove("op_account");
    Ok(())
}

fn migrate_config_op_breadcrumbs(doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    let root = doc.as_table_mut();
    migrate_env_map(root.get_mut("env"))?;
    migrate_github_settings(root.get_mut("github"))?;
    migrate_role_sources(root.get_mut("roles"))?;
    migrate_workspace_map(root.get_mut("workspaces"))?;
    migrate_account_credentials(root.get_mut("accounts"))
}

fn migrate_workspace_op_breadcrumbs(doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    migrate_workspace_settings(doc.as_table_mut())
}

fn migrate_workspace_settings(table: &mut dyn toml_edit::TableLike) -> crate::ConfigResult<()> {
    migrate_env_map(table.get_mut("env"))?;
    migrate_github_settings(table.get_mut("github"))?;
    migrate_workspace_role_overrides(table.get_mut("roles"))
}

fn migrate_env_map(item: Option<&mut toml_edit::Item>) -> crate::ConfigResult<()> {
    let Some(item) = item else {
        return Ok(());
    };
    for_each_table_like_entry(item, &mut migrate_env_value_table)
}

fn migrate_github_settings(item: Option<&mut toml_edit::Item>) -> crate::ConfigResult<()> {
    let Some(item) = item else {
        return Ok(());
    };
    visit_table_like_item(item, &mut |github| migrate_env_map(github.get_mut("env")))
}

fn migrate_role_sources(item: Option<&mut toml_edit::Item>) -> crate::ConfigResult<()> {
    let Some(item) = item else {
        return Ok(());
    };
    for_each_table_like_entry(item, &mut |role| migrate_env_map(role.get_mut("env")))
}

fn migrate_workspace_map(item: Option<&mut toml_edit::Item>) -> crate::ConfigResult<()> {
    let Some(item) = item else {
        return Ok(());
    };
    for_each_table_like_entry(item, &mut migrate_workspace_settings)
}

fn migrate_workspace_role_overrides(item: Option<&mut toml_edit::Item>) -> crate::ConfigResult<()> {
    let Some(item) = item else {
        return Ok(());
    };
    for_each_table_like_entry(item, &mut |role| {
        migrate_env_map(role.get_mut("env"))?;
        migrate_github_settings(role.get_mut("github"))
    })
}

fn migrate_account_credentials(item: Option<&mut toml_edit::Item>) -> crate::ConfigResult<()> {
    let Some(item) = item else {
        return Ok(());
    };
    for_each_table_like_entry(item, &mut |account| {
        let Some(credential) = account.get_mut("credential") else {
            return Ok(());
        };
        visit_table_like_item(credential, &mut |credential| {
            let kind = credential.get("type").and_then(toml_edit::Item::as_str);
            if matches!(kind, Some("api_key" | "oauth_token")) {
                migrate_env_value(credential.get_mut("value"))?;
            }
            Ok(())
        })
    })
}

fn migrate_env_value_table(table: &mut dyn toml_edit::TableLike) -> crate::ConfigResult<()> {
    migrate_op_ref_value(table)
}

fn migrate_env_value(item: Option<&mut toml_edit::Item>) -> crate::ConfigResult<()> {
    let Some(item) = item else {
        return Ok(());
    };
    visit_table_like_item(item, &mut migrate_op_ref_value)
}

fn migrate_op_ref_value(table: &mut dyn toml_edit::TableLike) -> crate::ConfigResult<()> {
    let has_op = table.contains_key("op");
    let has_path = table.contains_key("path");
    let has_breadcrumb = table.contains_key("breadcrumb");
    if !has_op && !has_path && !has_breadcrumb {
        return Ok(());
    }

    let old_path = table.get("path");
    match (old_path, has_breadcrumb) {
        (Some(_), true) => Err(ConfigError::msg(format_args!(
            "OpRef contains both legacy `path` and versioned `breadcrumb` fields"
        ))),
        (Some(path_item), false) => {
            let Some(path) = path_item.as_str() else {
                return Err(ConfigError::msg(format_args!(
                    "OpRef legacy `path` must be a string to migrate"
                )));
            };
            let Some(reference) = table.get("op").and_then(toml_edit::Item::as_str) else {
                return Err(ConfigError::msg(format_args!(
                    "OpRef `op` must be a string to migrate its legacy `path`"
                )));
            };
            let Some(value) = jackin_core::encode_legacy_op_breadcrumb(path, reference) else {
                return Err(ConfigError::msg(format_args!(
                    "OpRef legacy `path` is ambiguous or does not match the 3/4-segment path or query suffix in its `op` URI"
                )));
            };
            let mut breadcrumb = toml_edit::InlineTable::new();
            breadcrumb.insert("version", toml_edit::Value::from(1));
            breadcrumb.insert("value", toml_edit::Value::from(value));
            table.remove("path");
            table.insert(
                "breadcrumb",
                toml_edit::Item::Value(toml_edit::Value::InlineTable(breadcrumb)),
            );
            Ok(())
        }
        (None, true) => Ok(()),
        (None, false) => Err(ConfigError::msg(format_args!(
            "OpRef is missing both legacy `path` and versioned `breadcrumb` fields"
        ))),
    }?;
    validate_versioned_op_breadcrumb(table)
}

fn validate_versioned_op_breadcrumb(table: &dyn toml_edit::TableLike) -> crate::ConfigResult<()> {
    let mut document = DocumentMut::new();
    for (key, item) in table.iter() {
        document.as_table_mut().insert(key, item.clone());
    }
    let decoded: jackin_core::EnvValue = toml::from_str(&document.to_string()).map_err(|_| {
        ConfigError::msg(format_args!(
            "versioned OpRef does not match the strict environment-value schema"
        ))
    })?;
    let jackin_core::EnvValue::OpRef(op_ref) = decoded else {
        return Err(ConfigError::msg(format_args!(
            "versioned OpRef does not match the strict environment-value schema"
        )));
    };

    let Some(reference_parts) = jackin_core::parse_op_reference(&op_ref.op) else {
        return Err(ConfigError::msg(format_args!(
            "versioned OpRef `op` must be a valid 3/4-segment op:// URI"
        )));
    };
    let Some(path_parts) = jackin_core::parse_op_breadcrumb_path(&op_ref.path) else {
        return Err(ConfigError::msg(format_args!(
            "versioned OpRef breadcrumb value is not a valid v1 escaped path"
        )));
    };
    if path_parts.section.is_some() != reference_parts.section.is_some() {
        return Err(ConfigError::msg(format_args!(
            "OpRef breadcrumb segment count does not match its op:// URI"
        )));
    }
    if let Some(path_query) = path_parts.attribute_query.as_deref() {
        let reference_query = op_ref.op.split_once('?').map(|(_, query)| query);
        if reference_query != path_query.strip_prefix('?') {
            return Err(ConfigError::msg(format_args!(
                "OpRef breadcrumb query suffix does not match its op:// URI"
            )));
        }
    }
    Ok(())
}

fn for_each_table_like_entry<F>(
    item: &mut toml_edit::Item,
    visitor: &mut F,
) -> crate::ConfigResult<()>
where
    F: FnMut(&mut dyn toml_edit::TableLike) -> crate::ConfigResult<()>,
{
    if let Some(tables) = item.as_array_of_tables_mut() {
        for table in tables.iter_mut() {
            visitor(table)?;
        }
    } else if let Some(table) = item.as_table_like_mut() {
        for (_, child) in table.iter_mut() {
            visit_table_like_item(child, visitor)?;
        }
    } else if let Some(value) = item.as_value_mut() {
        for_each_table_like_entry_value(value, visitor)?;
    }
    Ok(())
}

fn for_each_table_like_entry_value<F>(
    value: &mut toml_edit::Value,
    visitor: &mut F,
) -> crate::ConfigResult<()>
where
    F: FnMut(&mut dyn toml_edit::TableLike) -> crate::ConfigResult<()>,
{
    if let Some(table) = value.as_inline_table_mut() {
        for (_, child) in table.iter_mut() {
            visit_table_like_item_value(child, visitor)?;
        }
    } else if let Some(array) = value.as_array_mut() {
        for child in array.iter_mut() {
            visit_table_like_item_value(child, visitor)?;
        }
    }
    Ok(())
}

fn visit_table_like_item<F>(item: &mut toml_edit::Item, visitor: &mut F) -> crate::ConfigResult<()>
where
    F: FnMut(&mut dyn toml_edit::TableLike) -> crate::ConfigResult<()>,
{
    if let Some(tables) = item.as_array_of_tables_mut() {
        for table in tables.iter_mut() {
            visitor(table)?;
        }
    } else if let Some(table) = item.as_table_like_mut() {
        visitor(table)?;
    } else if let Some(value) = item.as_value_mut() {
        visit_table_like_item_value(value, visitor)?;
    }
    Ok(())
}

fn visit_table_like_item_value<F>(
    value: &mut toml_edit::Value,
    visitor: &mut F,
) -> crate::ConfigResult<()>
where
    F: FnMut(&mut dyn toml_edit::TableLike) -> crate::ConfigResult<()>,
{
    if let Some(table) = value.as_inline_table_mut() {
        visitor(table)?;
    } else if let Some(array) = value.as_array_mut() {
        for child in array.iter_mut() {
            visit_table_like_item_value(child, visitor)?;
        }
    }
    Ok(())
}

fn stamp_account_in_env(table: &mut toml_edit::Table, acct: &str) {
    stamp_account_in_env_table(table.get_mut("env"), acct);
    if let Some(github) = table
        .get_mut("github")
        .and_then(toml_edit::Item::as_table_mut)
    {
        stamp_account_in_env_table(github.get_mut("env"), acct);
    }
}

fn stamp_account_in_env_table(env: Option<&mut toml_edit::Item>, acct: &str) {
    let Some(env) = env.and_then(toml_edit::Item::as_table_like_mut) else {
        return;
    };
    for (_, value) in env.iter_mut() {
        if let Some(tbl) = value.as_table_like_mut()
            && tbl.contains_key("op")
            && !tbl.contains_key("account")
        {
            tbl.insert("account", toml_edit::value(acct));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SchemaVersion {
    Legacy,

    Kubernetes(KubernetesVersion),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct KubernetesVersion {
    pub major: NonZeroU32,

    pub channel: Channel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Channel {
    Alpha(NonZeroU32),

    Beta(NonZeroU32),

    Stable,
}

impl std::fmt::Display for SchemaVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Legacy => write!(f, "{LEGACY_VERSION}"),
            Self::Kubernetes(k) => write!(f, "{k}"),
        }
    }
}

impl std::fmt::Display for KubernetesVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.channel {
            Channel::Stable => write!(f, "v{}", self.major),
            Channel::Alpha(seq) => write!(f, "v{}alpha{seq}", self.major),
            Channel::Beta(seq) => write!(f, "v{}beta{seq}", self.major),
        }
    }
}

pub fn migrate_config_file_if_needed(path: &Path) -> crate::ConfigResult<bool> {
    let _lock = acquire_config_write_lock(path)?;
    migrate_config_file_if_needed_locked(path)
}

pub(crate) fn migrate_config_file_if_needed_locked(path: &Path) -> crate::ConfigResult<bool> {
    let result = migrate_file_if_needed(path, "config", CURRENT_CONFIG_VERSION, CONFIG_MIGRATIONS);
    emit_migration_result("global", CURRENT_CONFIG_VERSION, CONFIG_MIGRATIONS, &result);
    result
        .map(|version| version.is_some())
        .map_err(ConfigError::telemetry_owned)
}

pub fn migrate_workspace_file_if_needed(path: &Path) -> crate::ConfigResult<bool> {
    let config_file = config_file_for_workspace_path(path);
    let _lock = acquire_config_write_lock(&config_file)?;
    migrate_workspace_file_if_needed_locked(path)
}

pub(crate) fn migrate_workspace_file_if_needed_locked(path: &Path) -> crate::ConfigResult<bool> {
    let result = migrate_file_if_needed(
        path,
        "workspace config",
        CURRENT_WORKSPACE_VERSION,
        WORKSPACE_MIGRATIONS,
    );
    emit_migration_result(
        "workspace",
        CURRENT_WORKSPACE_VERSION,
        WORKSPACE_MIGRATIONS,
        &result,
    );
    result
        .map(|version| version.is_some())
        .map_err(ConfigError::telemetry_owned)
}

pub(crate) fn emit_migration_result(
    scope: &'static str,
    current: &'static str,
    migrations: &[MigrationStep],
    result: &crate::ConfigResult<Option<SchemaVersion>>,
) {
    let from = result
        .as_ref()
        .ok()
        .and_then(Option::as_ref)
        .map(ToString::to_string);
    let outcome = match result {
        Ok(Some(_)) => "success",
        Ok(None) => "skip",
        Err(_) => "failure",
    };
    let mut attrs = vec![
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CONFIG_SCOPE,
            value: jackin_telemetry::Value::Str(scope),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::CONFIG_OPERATION,
            value: jackin_telemetry::Value::Str("migrate"),
        },
        jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::OUTCOME,
            value: jackin_telemetry::Value::Str(outcome),
        },
    ];
    if let Some(from) = from.as_deref() {
        let step_count = u64::try_from(
            migrations
                .iter()
                .skip_while(|step| step.from != from)
                .take_while(|step| step.from != current)
                .count(),
        )
        .unwrap_or(u64::MAX);
        attrs.extend([
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCHEMA_VERSION_FROM,
                value: jackin_telemetry::Value::Str(from),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_SCHEMA_VERSION_TO,
                value: jackin_telemetry::Value::Str(current),
            },
            jackin_telemetry::Attr {
                key: jackin_telemetry::schema::attrs::CONFIG_MIGRATION_STEP_COUNT,
                value: jackin_telemetry::Value::U64(step_count),
            },
        ]);
    }
    if result.is_err() {
        attrs.push(jackin_telemetry::Attr {
            key: jackin_telemetry::schema::attrs::std_attrs::ERROR_TYPE,
            value: jackin_telemetry::Value::Str("config_error"),
        });
    }
    let _result = jackin_telemetry::emit_event(
        &jackin_telemetry::event::CONFIG_OPERATION,
        jackin_telemetry::FieldSet::new(&attrs, None),
    );
}

pub fn migrate_file_if_needed(
    path: &Path,
    label: &str,
    current_raw: &str,
    migrations: &[MigrationStep],
) -> crate::ConfigResult<Option<SchemaVersion>> {
    migrate_file_contents_if_needed(path, label, current_raw, migrations).and_then(
        |(contents, old_version)| {
            if old_version.is_some() {
                atomic_write(path, &contents)
                    .with_context(|| format!("writing migrated {label} to {}", path.display()))?;
            }
            Ok(old_version)
        },
    )
}

pub(crate) fn migrate_file_contents_if_needed(
    path: &Path,
    label: &str,
    current_raw: &str,
    migrations: &[MigrationStep],
) -> crate::ConfigResult<(String, Option<SchemaVersion>)> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let mut doc: DocumentMut = raw
        .parse()
        .with_context(|| format!("parsing {}", path.display()))?;
    let migrated_from = migrate_document_if_needed(&mut doc, label, current_raw, migrations)?;
    if migrated_from.is_none() {
        return Ok((raw, None));
    }
    Ok((doc.to_string(), migrated_from))
}

pub(crate) fn migrate_document_if_needed(
    doc: &mut DocumentMut,
    label: &str,
    current_raw: &str,
    migrations: &[MigrationStep],
) -> crate::ConfigResult<Option<SchemaVersion>> {
    let old_version = doc_version(doc, label)?;
    let current = parse_version(current_raw)?;

    if old_version > current {
        return Err(ConfigError::msg(format_args!(
            "{label} is at {old_version}, this binary only understands up to {current_raw}; upgrade jackin"
        )));
    }
    if old_version == current {
        return Ok(None);
    }

    apply_migrations(doc, &old_version, &current, migrations, label)?;
    Ok(Some(old_version))
}

pub fn apply_migrations(
    doc: &mut DocumentMut,
    old_version: &SchemaVersion,
    current_version: &SchemaVersion,
    migrations: &[MigrationStep],
    label: &str,
) -> crate::ConfigResult<()> {
    let mut cursor = old_version.clone();
    while &cursor < current_version {
        let Some(step) = find_step(&cursor, migrations)? else {
            return Err(ConfigError::msg(format_args!(
                "{label} is at {old_version}, but this binary no longer includes a migration path to {current_version}; upgrade through an older jackin first"
            )));
        };
        let next = parse_registry_version(step.to)?;
        if next <= cursor {
            return Err(ConfigError::msg(format_args!(
                "{label} migration registry is invalid: step {} -> {} does not move forward",
                step.from, step.to
            )));
        }
        (step.migrate)(doc)
            .with_context(|| format!("running {label} migration {} -> {}", step.from, step.to))?;
        set_doc_version(doc, step.to);
        cursor = next;
    }
    if &cursor != current_version {
        return Err(ConfigError::msg(format_args!(
            "{label} migration registry stopped at {cursor}, expected {current_version}"
        )));
    }
    Ok(())
}

fn find_step<'a>(
    from: &SchemaVersion,
    migrations: &'a [MigrationStep],
) -> anyhow::Result<Option<&'a MigrationStep>> {
    for step in migrations {
        if parse_registry_version(step.from)? == *from {
            return Ok(Some(step));
        }
    }
    Ok(None)
}

pub fn parse_registry_version(version: &str) -> crate::ConfigResult<SchemaVersion> {
    if version == LEGACY_VERSION {
        return Ok(SchemaVersion::Legacy);
    }
    parse_version(version)
}

pub fn doc_version(doc: &DocumentMut, label: &str) -> crate::ConfigResult<SchemaVersion> {
    let Some(item) = doc.get("version") else {
        return Ok(SchemaVersion::Legacy);
    };
    let Some(version) = item.as_str() else {
        return Err(ConfigError::msg(format_args!(
            "{label} version must be a string"
        )));
    };
    Ok(parse_version(version).with_context(|| format!("{label} version is invalid"))?)
}

pub fn parse_version(version: &str) -> crate::ConfigResult<SchemaVersion> {
    let rest = version.strip_prefix('v').ok_or_else(|| {
        anyhow::Error::from(ConfigError::msg(format_args!(
            "version must start with `v`"
        )))
    })?;
    let (major_raw, suffix) = split_first_nondigit(rest).ok_or_else(|| {
        anyhow::Error::from(ConfigError::msg(format_args!("missing major version")))
    })?;
    let major = parse_canonical_u32(major_raw, "major version")?;
    let major = NonZeroU32::new(major).ok_or_else(|| {
        anyhow::Error::from(ConfigError::msg(format_args!(
            "major version must be greater than zero"
        )))
    })?;

    let channel = if suffix.is_empty() {
        Channel::Stable
    } else if let Some(seq_raw) = suffix.strip_prefix("alpha") {
        Channel::Alpha(parse_sequence("alpha", seq_raw)?)
    } else if let Some(seq_raw) = suffix.strip_prefix("beta") {
        Channel::Beta(parse_sequence("beta", seq_raw)?)
    } else {
        return Err(ConfigError::msg(format_args!(
            "version must look like v1, v1beta1, or v1alpha1"
        )));
    };

    Ok(SchemaVersion::Kubernetes(KubernetesVersion {
        major,
        channel,
    }))
}

fn parse_sequence(prefix: &str, raw: &str) -> anyhow::Result<NonZeroU32> {
    if raw.is_empty() {
        return Err(ConfigError::msg(format_args!(
            "{prefix} version must include a sequence number"
        ))
        .into());
    }
    let value = parse_canonical_u32(raw, &format!("{prefix} sequence"))?;
    NonZeroU32::new(value).ok_or_else(|| {
        anyhow::Error::from(ConfigError::msg(format_args!(
            "{prefix} sequence must be greater than zero"
        )))
    })
}

fn parse_canonical_u32(raw: &str, label: &str) -> anyhow::Result<u32> {
    if raw.len() > 1 && raw.starts_with('0') {
        return Err(ConfigError::msg(format_args!("{label} must not have leading zeros")).into());
    }
    raw.parse::<u32>()
        .with_context(|| format!("invalid {label} {raw:?}"))
}

fn split_first_nondigit(s: &str) -> Option<(&str, &str)> {
    let split = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if split == 0 {
        return None;
    }
    Some(s.split_at(split))
}

pub fn set_doc_version(doc: &mut DocumentMut, version: &str) {
    doc["version"] = toml_edit::value(version);
    doc.as_table_mut().sort_values_by(|left, _, right, _| {
        match (left.get() == "version", right.get() == "version") {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        }
    });
}

pub const fn noop_migration(_doc: &mut DocumentMut) -> crate::ConfigResult<()> {
    Ok(())
}

#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "registry verifier is test-only assertion logic; invariant failures must fail the test with exact context"
)]
pub fn assert_registry_chain(migrations: &[MigrationStep], current_raw: &str) {
    let mut seen_froms = std::collections::BTreeSet::new();
    for step in migrations {
        let from = parse_registry_version(step.from)
            .unwrap_or_else(|_| panic!("step.from {:?} does not parse", step.from));
        assert!(
            seen_froms.insert(from),
            "duplicate `from` {} in registry",
            step.from
        );
    }

    let current = parse_version(current_raw).expect("current version parses");
    let mut cursor = SchemaVersion::Legacy;
    let mut steps_taken = 0;
    while cursor < current {
        let step = migrations
            .iter()
            .find(|s| parse_registry_version(s.from).is_ok_and(|v| v == cursor))
            .unwrap_or_else(|| panic!("no step from {cursor} in registry"));
        let next = parse_registry_version(step.to)
            .unwrap_or_else(|_| panic!("step.to {:?} does not parse", step.to));
        assert!(
            next > cursor,
            "step {} -> {} is not strictly forward",
            step.from,
            step.to
        );
        cursor = next;
        steps_taken += 1;
        assert!(steps_taken <= migrations.len(), "registry has a cycle");
    }
    assert_eq!(cursor, current, "registry does not reach {current_raw}");

    assert_eq!(
        steps_taken,
        migrations.len(),
        "registry has {} unreachable step(s) past {current_raw}",
        migrations.len() - steps_taken
    );
}

#[cfg(test)]
mod tests;
