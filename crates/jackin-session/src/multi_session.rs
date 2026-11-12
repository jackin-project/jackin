// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::time::Duration;

use jackin_core::{
    Agent, AgentInstance, AgentInstances, SandboxLabels, SandboxName, SandboxPath, TerminalText,
    plain,
};
use jackin_openshell::{
    GatewayError, GatewayOp, Missing, ProgramOutcome, ProgramRun, ProgramStdin,
};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::service::Session;

const KILL_POLL: Duration = Duration::from_millis(100);
const KILL_EXIT: Duration = Duration::from_secs(30);

const INSTANCE_WRAPPER: &str = r#"token=''
for candidate in /usr/bin/ps /bin/ps; do
  if [ -x "$candidate" ]; then
    token=$("$candidate" -o lstart= -p $$ 2>/dev/null) && break
  fi
done
if [ -z "$token" ]; then
  echo "jackin: the instance identity could not be recorded" >&2
  exit 1
fi
printf '%s %s\n' "$$" "$token" > "$1" || exit 1
shift
exec "$@""#;

const INSTANCE_INCARNATION: &str = r#"for candidate in /usr/bin/ps /bin/ps; do
  if [ -x "$candidate" ]; then
    "$candidate" -o lstart= -p "$1"
    exit $?
  fi
done
ps -o lstart= -p "$1""#;

const WORKTREE_PREPARE: &str = r#"parent=$1
leaf=$2
branch=$3
root='/sandbox'
project="$root/project"
case "$leaf" in ''|.|..|*/*) exit 3 ;; esac
case "$parent" in "$root"/*) ;; *) exit 3 ;; esac
cd -P "$root" 2>/dev/null || exit 9
base=$(pwd -P) || exit 3
expected=$base
rest=${parent#"$root"/}
old_ifs=$IFS
IFS=/
set -f
set -- $rest
set +f
IFS=$old_ifs
for comp in "$@"; do
  expected=$expected/$comp
  if ! cd -P "./$comp" 2>/dev/null; then
    mkdir "./$comp" 2>/dev/null || exit 9
    cd -P "./$comp" 2>/dev/null || exit 9
  fi
  base=$(pwd -P) || exit 3
  [ "$base" = "$expected" ] || exit 3
done
rebuild() {
  git -C "$project" rev-parse --verify --quiet "refs/heads/$branch" >/dev/null 2>&1 \
    || git -C "$project" branch "$branch" HEAD >/dev/null 2>&1 || exit 9
  rm -rf "./.jackin-rescue"
  mv "./$leaf" "./.jackin-rescue" || exit 9
  git -C "$project" worktree prune >/dev/null 2>&1 || exit 9
  git --git-dir="$project/.git" worktree add "./$leaf" "$branch" >/dev/null 2>&1 || exit 9
  rm -f "./.jackin-rescue/.git"
  cp -a "./.jackin-rescue/." "./$leaf/" || exit 9
  rm -rf "./.jackin-rescue"
  exit 8
}
if [ ! -L "./$leaf" ] && [ -f "./$leaf/.git" ]; then
  wt_head=$(git -C "./$leaf" symbolic-ref --quiet HEAD 2>/dev/null) || rebuild
  [ "$wt_head" = "refs/heads/$branch" ] || rebuild
  git -C "./$leaf" rev-parse --verify --quiet HEAD >/dev/null 2>&1 || rebuild
  wt_git=$(git -C "./$leaf" rev-parse --absolute-git-dir 2>/dev/null) || rebuild
  lead_git=$(git -C "$project" rev-parse --absolute-git-dir 2>/dev/null) || exit 9
  case "$wt_git" in "$lead_git"/worktrees/*) ;; *) rebuild ;; esac
  admin_head=$(cat "$wt_git/HEAD" 2>/dev/null) || admin_head=''
  [ "$admin_head" = "ref: refs/heads/$branch" ] || rebuild
  backlink="$wt_git/gitdir"
  recorded=$(cat "$backlink" 2>/dev/null) || recorded=''
  wanted="$expected/$leaf/.git"
  if [ "$recorded" != "$wanted" ]; then
    printf '%s\n' "$wanted" > "$backlink.jackin" 2>/dev/null || exit 9
    mv -f "$backlink.jackin" "$backlink" 2>/dev/null || exit 9
  fi
  exit 4
fi
if [ -e "./$leaf" ] || [ -L "./$leaf" ]; then
  rm -rf "./$leaf" || exit 5
fi
git -C "$project" worktree prune >/dev/null 2>&1 || exit 9
if git -C "$project" rev-parse --verify --quiet "refs/heads/$branch" >/dev/null 2>&1; then
  git --git-dir="$project/.git" worktree add "./$leaf" "$branch" >/dev/null 2>&1 || exit 9
else
  git --git-dir="$project/.git" worktree add -b "$branch" "./$leaf" HEAD >/dev/null 2>&1 || exit 9
fi
exit 8
"#;

#[derive(Debug, Error)]
pub enum IntegrateError {
    #[error(transparent)]
    Gateway(#[from] GatewayError),
    #[error("merge of branch {} stopped with a conflict", plain(.branch))]
    Conflict { branch: String },
    #[error("agent {} is not in this sandbox", plain(.agent))]
    UnknownAgent { agent: String },
}

fn layout_worktree_lead(instances: &AgentInstances) -> Option<Agent> {
    instances.lead_opt().map(|item| item.agent)
}

fn invalid_instance_identity() -> GatewayError {
    GatewayError::Protocol {
        message: "sandbox instance identity is invalid".to_owned(),
    }
}

fn instance_record(agent: Agent) -> SandboxPath {
    SandboxPath::parse(&format!(
        "/sandbox/.jackin/run/{}/instance.pid",
        agent.as_str()
    ))
    .unwrap_or_else(|_| SandboxPath::from_static("/sandbox/.jackin/run/instance.pid"))
}

pub(crate) fn instance_argv(agent: Agent, argv: &[String]) -> Vec<String> {
    let mut wrapped = vec![
        "/bin/sh".to_owned(),
        "-c".to_owned(),
        INSTANCE_WRAPPER.to_owned(),
        "jackin-instance".to_owned(),
        instance_record(agent).as_str().to_owned(),
    ];
    wrapped.extend(argv.iter().cloned());
    wrapped
}

struct InstanceIdentity {
    pid: u32,
    token: String,
}

fn normalize_token(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .split_whitespace()
        .collect::<Vec<_>>()
        .concat()
}

fn parse_instance_identity(held: &str) -> Result<InstanceIdentity, GatewayError> {
    let unreadable = || GatewayError::Protocol {
        message: "the retained instance identity is unreadable".to_owned(),
    };
    let not_a_process = || GatewayError::Protocol {
        message: "the retained instance identity is not a process".to_owned(),
    };
    let mut parts = held.split_whitespace();
    let pid = parts.next().unwrap_or_default();
    if pid.is_empty() || !pid.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(unreadable());
    }
    let pid = pid.parse::<u32>().map_err(|_| not_a_process())?;
    if pid == 0 {
        return Err(not_a_process());
    }
    let token = parts.collect::<Vec<_>>().concat();
    if token.is_empty() {
        return Err(unreadable());
    }
    Ok(InstanceIdentity { pid, token })
}

fn instance_exit(program: &str, code: i32) -> GatewayError {
    GatewayError::Protocol {
        message: format!("{program} for the detached instance exited {code}"),
    }
}

impl Session {
    pub async fn setup_worktrees(
        &self,
        sandbox: &SandboxName,
        instances: &AgentInstances,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        if instances.len() <= 1 {
            return Ok(());
        }
        let git = SandboxPath::from_static("/usr/bin/git");
        let project = SandboxPath::from_static("/sandbox/project");
        let probe = self
            .deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: git.clone(),
                    args: vec![
                        "rev-parse".to_owned(),
                        "--verify".to_owned(),
                        "HEAD".to_owned(),
                    ],
                    workdir: Some(project.clone()),
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: Duration::from_secs(30),
                },
                cancel,
            )
            .await?;
        if probe.exit_code != 0 {
            return Err(GatewayError::Protocol {
                message: "the project has no initial commit to branch the secondary worktrees from"
                    .to_owned(),
            });
        }
        let layout = crate::multi::plan_multi(instances, None, crate::multi::HeadState::Born)
            .map_err(|error| GatewayError::Protocol {
                message: error.to_string(),
            })?;
        let mkdir = SandboxPath::from_static("/usr/bin/mkdir");
        for directory in layout.directories.iter().filter(|path| {
            path.as_str().starts_with("/sandbox/.jackin/task")
                || path.as_str().starts_with("/sandbox/.jackin/run/")
        }) {
            self.mkdir(sandbox, &mkdir, directory, cancel).await?;
        }
        for home in &layout.homes {
            self.mkdir(sandbox, &mkdir, home, cancel).await?;
        }
        for worktree in layout
            .worktrees
            .iter()
            .filter(|item| Some(item.agent) != layout_worktree_lead(instances))
        {
            if worktree.worktree {
                let branch = worktree.branch.clone().unwrap_or_default();
                self.worktree_prepare(sandbox, &worktree.path, &branch, cancel)
                    .await?;
            } else {
                self.mkdir(sandbox, &mkdir, &worktree.path, cancel).await?;
            }
        }
        Ok(())
    }

    async fn mkdir(
        &self,
        sandbox: &SandboxName,
        mkdir: &SandboxPath,
        path: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let outcome = self
            .deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: mkdir.clone(),
                    args: vec!["-p".to_owned(), path.as_str().to_owned()],
                    workdir: None,
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: Duration::from_secs(30),
                },
                cancel,
            )
            .await?;
        if outcome.exit_code != 0 {
            return Err(GatewayError::Protocol {
                message: format!("mkdir {} exited {}", path.as_str(), outcome.exit_code),
            });
        }
        Ok(())
    }

    async fn worktree_prepare(
        &self,
        sandbox: &SandboxName,
        path: &SandboxPath,
        branch: &str,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let (parent, leaf) =
            path.as_str()
                .rsplit_once('/')
                .ok_or_else(|| GatewayError::Protocol {
                    message: format!("worktree path {} has no parent", path.as_str()),
                })?;
        let outcome = self
            .deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: SandboxPath::from_static("/bin/sh"),
                    args: vec![
                        "-c".to_owned(),
                        WORKTREE_PREPARE.to_owned(),
                        "worktree-prepare".to_owned(),
                        parent.to_owned(),
                        leaf.to_owned(),
                        branch.to_owned(),
                    ],
                    workdir: None,
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: Duration::from_secs(120),
                },
                cancel,
            )
            .await?;
        match outcome.exit_code {
            4 | 8 => Ok(()),
            3 => Err(GatewayError::Protocol {
                message: format!(
                    "worktree cleanup of {} refused a substituted ancestor",
                    path.as_str()
                ),
            }),
            code => Err(GatewayError::Protocol {
                message: format!("worktree setup of {} exited {}", path.as_str(), code),
            }),
        }
    }

    pub(crate) async fn sandbox_instances(
        &self,
        sandbox: &SandboxName,
    ) -> Result<AgentInstances, GatewayError> {
        let view = self
            .find_managed(sandbox)
            .await?
            .ok_or_else(|| GatewayError::NotFound {
                what: Missing::Sandbox(sandbox.clone()),
            })?;
        let labels =
            SandboxLabels::from_labels(&view.labels).map_err(|_| invalid_instance_identity())?;
        let path = SandboxPath::from_static("/sandbox/.jackin/agents.json");
        let found = self.deps.gateway.read_files(sandbox, &[path]).await?;
        let bytes = found
            .into_iter()
            .next()
            .flatten()
            .ok_or_else(invalid_instance_identity)?;
        let items: Vec<AgentInstance> =
            serde_json::from_slice(&bytes).map_err(|_| invalid_instance_identity())?;
        let instances = AgentInstances::new(items).map_err(|_| invalid_instance_identity())?;
        for instance in instances.iter() {
            jackin_core::lookup_tuple(instance.agent, instance.provider)
                .and_then(|tuple| tuple.check_model(&instance.model))
                .map_err(|_| invalid_instance_identity())?;
        }
        if instances.agents_label() != labels.planned.agents.agents_label() {
            return Err(invalid_instance_identity());
        }
        Ok(instances)
    }

    pub async fn sandbox_agent_names(
        &self,
        sandbox: &SandboxName,
    ) -> Result<Vec<String>, GatewayError> {
        Ok(self
            .sandbox_instances(sandbox)
            .await?
            .canonical()
            .into_iter()
            .map(|item| item.agent.as_str().to_owned())
            .collect())
    }

    pub(crate) async fn kill_instance(
        &self,
        sandbox: &SandboxName,
        agent: Agent,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let record = instance_record(agent);
        let found = self
            .deps
            .gateway
            .read_files(sandbox, std::slice::from_ref(&record))
            .await?;
        let Some(bytes) = found.into_iter().next().flatten() else {
            return Ok(());
        };
        let held = String::from_utf8_lossy(&bytes);
        let held = held.trim();
        if held.is_empty() {
            return Ok(());
        }
        let identity = parse_instance_identity(held)?;
        let current = self
            .instance_incarnation(sandbox, identity.pid, cancel)
            .await?;
        if current.as_deref() != Some(identity.token.as_str()) {
            self.clear_instance_record(sandbox, &record, cancel).await?;
            return Ok(());
        }
        let pid = identity.pid.to_string();
        let outcome = self.signal_instance(sandbox, &pid, cancel).await?;
        match outcome.exit_code {
            0 | 1 => {}
            code => return Err(instance_exit("kill", code)),
        }
        let started = self.deps.host.clock.now();
        loop {
            let probe = self.probe_instance(sandbox, &pid, cancel).await?;
            match probe.exit_code {
                1 => break,
                0 => {}
                code => return Err(instance_exit("kill", code)),
            }
            if self.deps.host.clock.now().0 - started.0 >= KILL_EXIT.as_secs() as i64 {
                return Err(GatewayError::Timeout {
                    op: GatewayOp::Program,
                    after: KILL_EXIT,
                    group: None,
                });
            }
            self.deps.host.clock.sleep(KILL_POLL).await;
        }
        self.clear_instance_record(sandbox, &record, cancel).await?;
        Ok(())
    }

    async fn instance_incarnation(
        &self,
        sandbox: &SandboxName,
        pid: u32,
        cancel: &CancellationToken,
    ) -> Result<Option<String>, GatewayError> {
        let outcome = self
            .deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: SandboxPath::from_static("/bin/sh"),
                    args: vec![
                        "-c".to_owned(),
                        INSTANCE_INCARNATION.to_owned(),
                        "instance-incarnation".to_owned(),
                        pid.to_string(),
                    ],
                    workdir: None,
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: Duration::from_secs(30),
                },
                cancel,
            )
            .await?;
        match outcome.exit_code {
            0 => Ok(Some(normalize_token(&outcome.stdout))),
            1 => Ok(None),
            code => Err(GatewayError::Protocol {
                message: format!(
                    "the retained instance identity could not be read: the query exited {code}"
                ),
            }),
        }
    }

    async fn clear_instance_record(
        &self,
        sandbox: &SandboxName,
        record: &SandboxPath,
        cancel: &CancellationToken,
    ) -> Result<(), GatewayError> {
        let cleared = self
            .deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: SandboxPath::from_static("/usr/bin/rm"),
                    args: vec!["-f".to_owned(), "--".to_owned(), record.as_str().to_owned()],
                    workdir: None,
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: Duration::from_secs(30),
                },
                cancel,
            )
            .await?;
        if cleared.exit_code != 0 {
            return Err(instance_exit("rm", cleared.exit_code));
        }
        Ok(())
    }

    async fn signal_instance(
        &self,
        sandbox: &SandboxName,
        pid: &str,
        cancel: &CancellationToken,
    ) -> Result<ProgramOutcome, GatewayError> {
        self.deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: SandboxPath::from_static("/usr/bin/kill"),
                    args: vec!["-TERM".to_owned(), pid.to_owned()],
                    workdir: None,
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: Duration::from_secs(30),
                },
                cancel,
            )
            .await
    }

    async fn probe_instance(
        &self,
        sandbox: &SandboxName,
        pid: &str,
        cancel: &CancellationToken,
    ) -> Result<ProgramOutcome, GatewayError> {
        self.deps
            .gateway
            .run_program(
                &ProgramRun {
                    sandbox: sandbox.clone(),
                    program: SandboxPath::from_static("/usr/bin/ps"),
                    args: vec!["-p".to_owned(), pid.to_owned()],
                    workdir: None,
                    env: BTreeMap::new(),
                    stdin: ProgramStdin::Closed,
                    touch_on_success: None,
                    timeout: Duration::from_secs(30),
                },
                cancel,
            )
            .await
    }

    pub async fn stop_instance(
        &self,
        sandbox: &SandboxName,
        agent: Agent,
        cancel: &CancellationToken,
    ) -> Result<(), IntegrateError> {
        let names = self.sandbox_agent_names(sandbox).await?;
        if !names.is_empty() && !names.iter().any(|name| name == agent.as_str()) {
            return Err(IntegrateError::UnknownAgent {
                agent: agent.as_str().to_owned(),
            });
        }
        self.kill_instance(sandbox, agent, cancel).await?;
        Ok(())
    }

    pub async fn integrate(
        &self,
        sandbox: &SandboxName,
        cancel: &CancellationToken,
    ) -> Result<Vec<String>, IntegrateError> {
        let names = self.sandbox_agent_names(sandbox).await?;
        let mut merged = Vec::new();
        for (index, name) in names.iter().enumerate() {
            if index == 0 {
                continue;
            }
            let branch = format!("agent/{name}");
            let outcome = self
                .deps
                .gateway
                .run_program(
                    &ProgramRun {
                        sandbox: sandbox.clone(),
                        program: SandboxPath::from_static("/usr/bin/git"),
                        args: vec!["merge".to_owned(), "--no-edit".to_owned(), branch.clone()],
                        workdir: Some(SandboxPath::from_static("/sandbox/project")),
                        env: BTreeMap::new(),
                        stdin: ProgramStdin::Closed,
                        touch_on_success: None,
                        timeout: Duration::from_secs(120),
                    },
                    cancel,
                )
                .await?;
            if outcome.exit_code != 0 {
                let paths = self
                    .deps
                    .gateway
                    .run_program(
                        &ProgramRun {
                            sandbox: sandbox.clone(),
                            program: SandboxPath::from_static("/usr/bin/git"),
                            args: vec![
                                "diff".to_owned(),
                                "--name-only".to_owned(),
                                "-z".to_owned(),
                                "--diff-filter=U".to_owned(),
                            ],
                            workdir: Some(SandboxPath::from_static("/sandbox/project")),
                            env: BTreeMap::new(),
                            stdin: ProgramStdin::Closed,
                            touch_on_success: None,
                            timeout: Duration::from_secs(30),
                        },
                        cancel,
                    )
                    .await?;
                if paths.exit_code == 0 && !paths.stdout.is_empty() {
                    return Err(IntegrateError::Conflict { branch });
                }
                let output = if outcome.stderr.is_empty() {
                    &outcome.stdout
                } else {
                    &outcome.stderr
                };
                let message = if paths.exit_code == 0 {
                    format!(
                        "git merge {branch} exited {} without unmerged paths: {}",
                        outcome.exit_code,
                        TerminalText::sanitize(output)
                    )
                } else {
                    format!(
                        "git merge {branch} exited {}: {}; checking unmerged paths exited {}: {}",
                        outcome.exit_code,
                        TerminalText::sanitize(output),
                        paths.exit_code,
                        TerminalText::sanitize(&paths.stderr)
                    )
                };
                return Err(GatewayError::Protocol { message }.into());
            }
            merged.push(branch);
        }
        Ok(merged)
    }
}

impl Session {
    pub async fn restart_instance(
        &self,
        sandbox: &SandboxName,
        agent: Agent,
        cancel: &CancellationToken,
    ) -> Result<(), IntegrateError> {
        self.stop_instance(sandbox, agent, cancel).await
    }
}
