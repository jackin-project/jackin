// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use jackin_core::EnvValue;

pub trait OpRunner: Send + Sync {
    fn read(&self, reference: &str) -> anyhow::Result<String>;

    fn read_with_account(&self, reference: &str, _account: Option<&str>) -> anyhow::Result<String> {
        self.read(reference)
    }

    fn probe(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn resolve_env_value<R, H>(
    layer_label: &str,
    var_name: &str,
    value: &EnvValue,
    op_runner: &R,
    host_env: H,
) -> anyhow::Result<String>
where
    R: OpRunner + ?Sized,
    H: FnMut(&str) -> Result<String, std::env::VarError>,
{
    match value {
        EnvValue::Plain(s) => dispatch_plain(layer_label, var_name, s, host_env),
        EnvValue::OpRef(r) => op_runner
            .read_with_account(&r.op, r.account.as_deref())
            .map_err(|e| {
                anyhow::anyhow!(
                    "{layer_label} env var {var_name:?}: 1Password reference {:?} failed: {e}",
                    jackin_core::display_op_breadcrumb_path(&r.path)
                )
            }),
        EnvValue::Extended(e) => {
            if e.on_demand {
                Err(anyhow::anyhow!(
                    "{layer_label} env var {var_name:?}: on_demand value reached \
                     resolve_env_value — it should have been filtered before launch"
                ))
            } else {
                dispatch_plain(layer_label, var_name, &e.value, host_env)
            }
        }
    }
}

fn dispatch_plain<H>(
    layer_label: &str,
    var_name: &str,
    value: &str,
    mut host_env: H,
) -> anyhow::Result<String>
where
    H: FnMut(&str) -> Result<String, std::env::VarError>,
{
    if let Some(host_name) = parse_host_ref(value) {
        return host_env(host_name).map_err(|_| {
            anyhow::anyhow!(
                "{layer_label} env var {var_name:?}: host env var {host_name:?} is not set"
            )
        });
    }
    Ok(value.to_owned())
}

fn parse_host_ref(value: &str) -> Option<&str> {
    if let Some(rest) = value.strip_prefix("${")
        && let Some(name) = rest.strip_suffix('}')
        && is_valid_env_name(name)
    {
        return Some(name);
    }
    if let Some(name) = value.strip_prefix('$')
        && !name.is_empty()
        && is_valid_env_name(name)
    {
        return Some(name);
    }
    None
}

fn is_valid_env_name(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
