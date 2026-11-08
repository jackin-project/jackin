// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};

use futures::future::BoxFuture;
use openshell_sdk::raw::proto::open_shell_server::OpenShellServer;
use openshell_sdk::raw::proto::{
    ExecSandboxEvent, ExecSandboxRequest, Provider, ProviderProfile, Sandbox, WorkspaceSelector,
    workspace_selector,
};
use tokio::sync::{Notify, mpsc, oneshot};
use tonic::transport::server::TcpIncoming;
use tonic::transport::{Certificate, Identity, Server, ServerTlsConfig};
use tonic::{Code, Status};
use tonic_types::{ErrorDetails, StatusExt};

use super::pki::Pki;
use super::ssh_server::SshModel;

pub(crate) const MAX_ROUTABLE_NAME_LEN: usize = 19;

pub(crate) type Key = (String, String);

#[derive(Clone, Debug)]
pub(crate) struct RefreshRecord {
    pub(crate) strategy: i32,
    pub(crate) material: HashMap<String, String>,
    pub(crate) secret_keys: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct PolicyInfo {
    pub(crate) version: u32,
    pub(crate) hash: String,
    pub(crate) loaded: bool,
    pub(crate) extra_rules: Vec<(String, openshell_sdk::raw::proto::NetworkPolicyRule)>,
}

impl Default for PolicyInfo {
    fn default() -> PolicyInfo {
        PolicyInfo {
            version: 1,
            hash: "h1".to_owned(),
            loaded: true,
            extra_rules: Vec::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Fault {
    Status(Code, String),
    Detailed(Status),
    Hang,
}

pub(crate) struct ExecCtx {
    pub(crate) start: ExecSandboxRequest,
    pub(crate) inputs:
        mpsc::UnboundedReceiver<openshell_sdk::raw::proto::exec_sandbox_input::Payload>,
    pub(crate) events: mpsc::Sender<Result<ExecSandboxEvent, Status>>,
    pub(crate) shared: Arc<Shared>,
}

pub(crate) type ExecBackend = Arc<dyn Fn(ExecCtx) -> BoxFuture<'static, ()> + Send + Sync>;

#[derive(Debug, Default)]
pub(crate) struct Observed {
    pub(crate) child_started: bool,
    pub(crate) child_killed: bool,
    pub(crate) request_stream_ended: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Broken {
    StopOnStopped,
    DeleteAttachedProviders,
    DeleteAfterRefusal,
    DeleteAbsentRefused,
}

pub(crate) type ProfileHook = Box<dyn FnMut(&mut State) + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Paging {
    Finite,
    Cycling,
}

pub(crate) struct State {
    pub(crate) sandboxes: BTreeMap<Key, Sandbox>,
    pub(crate) providers: BTreeMap<Key, Provider>,
    pub(crate) profiles: BTreeMap<Key, ProviderProfile>,
    pub(crate) refresh: BTreeMap<(String, String, String), RefreshRecord>,
    pub(crate) policy_info: BTreeMap<Key, PolicyInfo>,
    pub(crate) faults: HashMap<&'static str, Fault>,
    pub(crate) rpc_count: usize,
    pub(crate) page_cap: usize,
    pub(crate) paging: Paging,
    pub(crate) store_labels: bool,
    pub(crate) store_environment: bool,
    pub(crate) known_images: Option<HashSet<String>>,
    pub(crate) exec_starts: Vec<ExecSandboxRequest>,
    pub(crate) exec_streams: usize,
    pub(crate) profile_updates: usize,
    pub(crate) next_id: u64,
    pub(crate) exec: Option<ExecBackend>,
    pub(crate) after_get_profile: Option<ProfileHook>,
    pub(crate) before_import: Option<ProfileHook>,
    pub(crate) before_create_provider: Option<ProfileHook>,
    pub(crate) create_provider_response_error: Option<Status>,
    pub(crate) after_ssh_session: Option<ProfileHook>,
    pub(crate) import_rejection: Option<(String, String)>,
    pub(crate) forward_error: Option<Status>,
    pub(crate) observed: Observed,
    pub(crate) auto_advance: bool,
    pub(crate) ssh: Option<Arc<SshModel>>,
    pub(crate) broken: Option<Broken>,
    pub(crate) deleted_providers: BTreeSet<Key>,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State").finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub(crate) struct Shared {
    pub(crate) state: Mutex<State>,
    pub(crate) arrived: Notify,
    pub(crate) child_started: Notify,
    pub(crate) child_killed: Notify,
    pub(crate) stream_ended: Notify,
}

impl Shared {
    pub(crate) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct Fake {
    pub(crate) shared: Arc<Shared>,
}

pub(crate) struct ServerHandle {
    pub(crate) addr: SocketAddr,
    pub(crate) shared: Arc<Shared>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl std::fmt::Debug for ServerHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerHandle")
            .field("addr", &self.addr)
            .finish_non_exhaustive()
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _sent = shutdown.send(());
        }
    }
}

impl ServerHandle {
    pub(crate) fn endpoint(&self) -> String {
        format!("https://127.0.0.1:{}", self.addr.port())
    }

    pub(crate) fn state(&self) -> MutexGuard<'_, State> {
        self.shared.lock()
    }
}

fn fresh_state() -> State {
    State {
        sandboxes: BTreeMap::new(),
        providers: BTreeMap::new(),
        profiles: BTreeMap::new(),
        refresh: BTreeMap::new(),
        policy_info: BTreeMap::new(),
        faults: HashMap::new(),
        rpc_count: 0,
        page_cap: 2,
        paging: Paging::Finite,
        store_labels: true,
        store_environment: true,
        known_images: None,
        exec_starts: Vec::new(),
        exec_streams: 0,
        profile_updates: 0,
        next_id: 1,
        exec: None,
        after_get_profile: None,
        before_import: None,
        before_create_provider: None,
        create_provider_response_error: None,
        after_ssh_session: None,
        import_rejection: None,
        forward_error: None,
        observed: Observed::default(),
        auto_advance: false,
        ssh: None,
        broken: None,
        deleted_providers: BTreeSet::new(),
    }
}

pub(crate) async fn start(server_pki: &Pki, client_ca_pem: &str) -> ServerHandle {
    let shared = Arc::new(Shared {
        state: Mutex::new(fresh_state()),
        arrived: Notify::new(),
        child_started: Notify::new(),
        child_killed: Notify::new(),
        stream_ended: Notify::new(),
    });
    let tls = ServerTlsConfig::new()
        .identity(Identity::from_pem(
            &server_pki.server_cert_pem,
            &server_pki.server_key_pem,
        ))
        .client_ca_root(Certificate::from_pem(client_ca_pem));
    let incoming = TcpIncoming::bind("127.0.0.1:0".parse().unwrap()).expect("bind");
    let addr = incoming.local_addr().expect("address");
    let (shutdown, signal) = oneshot::channel::<()>();
    let service = OpenShellServer::new(Fake {
        shared: Arc::clone(&shared),
    });
    tokio::spawn(async move {
        Server::builder()
            .tls_config(tls)
            .expect("tls")
            .add_service(service)
            .serve_with_incoming_shutdown(incoming, async {
                drop(signal.await);
            })
            .await
            .expect("serve");
    });
    ServerHandle {
        addr,
        shared,
        shutdown: Some(shutdown),
    }
}

pub(crate) fn scope_name(scope: Option<&WorkspaceSelector>) -> Result<String, Status> {
    match scope.and_then(|selector| selector.selection.as_ref()) {
        Some(workspace_selector::Selection::Workspace(name)) if !name.is_empty() => {
            Ok(name.clone())
        }
        _ => Err(Status::invalid_argument("a named workspace is required")),
    }
}

pub(crate) fn invalid_field(field: &str, message: &str) -> Status {
    Status::with_error_details(
        Code::InvalidArgument,
        message,
        ErrorDetails::with_bad_request_violation(field, message),
    )
}

pub(crate) fn validate_sandbox_name(name: &str) -> Result<(), Status> {
    if name.len() > MAX_ROUTABLE_NAME_LEN {
        return Err(invalid_field(
            "name",
            &format!(
                "name exceeds maximum length ({} > {MAX_ROUTABLE_NAME_LEN})",
                name.len()
            ),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(invalid_field("name", "name has an invalid character"));
    }
    Ok(())
}

pub(crate) fn validate_label_key(key: &str) -> Result<(), Status> {
    let (prefix, name) = match key.split_once('/') {
        Some((prefix, name)) => (Some(prefix), name),
        None => (None, key),
    };
    let alnum = |c: char| c.is_alphanumeric();
    let valid_name = !name.is_empty()
        && name.len() <= 63
        && name
            .chars()
            .all(|c| alnum(c) || matches!(c, '-' | '_' | '.'))
        && name.chars().next().is_some_and(alnum)
        && name.chars().next_back().is_some_and(alnum);
    let valid_prefix = prefix.is_none_or(|prefix| {
        !prefix.is_empty()
            && prefix.len() <= 253
            && prefix
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '.'))
            && !prefix.starts_with(['-', '.'])
            && !prefix.ends_with(['-', '.'])
            && !prefix.contains("..")
    });
    if valid_name && valid_prefix {
        Ok(())
    } else {
        Err(Status::invalid_argument(format!(
            "invalid label key: '{key}'"
        )))
    }
}

pub(crate) fn validate_label_value(value: &str) -> Result<(), Status> {
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > 63 {
        return Err(Status::invalid_argument(format!(
            "label value exceeds 63 characters: '{value}'"
        )));
    }
    let alnum = |c: char| c.is_alphanumeric();
    let valid = value
        .chars()
        .all(|c| alnum(c) || matches!(c, '-' | '_' | '.'))
        && value.chars().next().is_some_and(alnum)
        && value.chars().next_back().is_some_and(alnum);
    if valid {
        Ok(())
    } else {
        Err(Status::invalid_argument(format!(
            "invalid label value: '{value}'"
        )))
    }
}

pub(crate) fn parse_selector(selector: &str) -> Result<Vec<(String, String)>, Status> {
    let mut pairs = Vec::new();
    for pair in selector.split(',') {
        let pair = pair.trim();
        if pair.is_empty() {
            continue;
        }
        let Some((key, value)) = pair.split_once('=') else {
            return Err(Status::invalid_argument(format!(
                "invalid label selector: expected 'key=value', got '{pair}'"
            )));
        };
        validate_label_key(key.trim())?;
        validate_label_value(value.trim())?;
        pairs.push((key.trim().to_owned(), value.trim().to_owned()));
    }
    Ok(pairs)
}

impl Fake {
    pub(crate) async fn enter(&self, method: &'static str) -> Result<(), Status> {
        let fault = {
            let mut state = self.shared.lock();
            state.rpc_count += 1;
            state.faults.get(method).cloned()
        };
        self.shared.arrived.notify_one();
        match fault {
            None => Ok(()),
            Some(Fault::Status(code, message)) => Err(Status::new(code, message)),
            Some(Fault::Detailed(status)) => Err(status),
            Some(Fault::Hang) => {
                std::future::pending::<()>().await;
                Ok(())
            }
        }
    }

    pub(crate) fn next_id(state: &mut State) -> String {
        state.next_id += 1;
        format!("id-{}", state.next_id)
    }
}
