// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use futures::stream;
use openshell_sdk::raw::proto::{
    CreateSshSessionRequest, CreateSshSessionResponse, DeletionOutcome, RevokeSshSessionRequest,
    RevokeSshSessionResponse, SandboxPhase, TcpForwardFrame, tcp_forward_frame,
};
use tonic::{Request, Response, Status, Streaming};

use super::sandbox_rpc::phase_of;
use super::server::{Fake, scope_name};
use super::service::Boxed;

const FINGERPRINT: &str = "SHA256:in-process-host-key";

fn lookup(state: &super::server::State, key: &(String, String)) -> Result<(), Status> {
    let sandbox = state
        .sandboxes
        .get(key)
        .ok_or_else(|| Status::not_found("sandbox not found"))?;
    if phase_of(sandbox) == SandboxPhase::Ready as i32 {
        Ok(())
    } else {
        Err(Status::failed_precondition("sandbox is not ready"))
    }
}

impl Fake {
    pub(crate) async fn create_ssh_session_impl(
        &self,
        request: Request<CreateSshSessionRequest>,
    ) -> Result<Response<CreateSshSessionResponse>, Status> {
        self.enter("create_ssh_session").await?;
        let request = request.into_inner();
        let key = (
            scope_name(request.workspace_scope.as_ref())?,
            request.sandbox,
        );
        let mut state = self.shared.lock();
        lookup(&state, &key)?;
        let token = format!("token-{}", Fake::next_id(&mut state));
        if let Some(mut hook) = state.after_ssh_session.take() {
            hook(&mut state);
        }
        Ok(Response::new(CreateSshSessionResponse {
            sandbox_id: key.1,
            token,
            gateway_host: "127.0.0.1".to_owned(),
            gateway_port: 1,
            gateway_scheme: "https".to_owned(),
            host_key_fingerprint: FINGERPRINT.to_owned(),
            ..CreateSshSessionResponse::default()
        }))
    }

    pub(crate) async fn revoke_ssh_session_impl(
        &self,
        request: Request<RevokeSshSessionRequest>,
    ) -> Result<Response<RevokeSshSessionResponse>, Status> {
        self.enter("revoke_ssh_session").await?;
        drop(request);
        Ok(Response::new(RevokeSshSessionResponse {
            outcome: DeletionOutcome::Completed as i32,
        }))
    }

    pub(crate) async fn forward_tcp_impl(
        &self,
        request: Request<Streaming<TcpForwardFrame>>,
    ) -> Result<Response<Boxed<TcpForwardFrame>>, Status> {
        self.enter("forward_tcp").await?;
        let mut inbound = request.into_inner();
        let Some(tcp_forward_frame::Payload::Init(init)) =
            inbound.message().await?.and_then(|frame| frame.payload)
        else {
            return Err(Status::invalid_argument("the first frame must be init"));
        };
        let key = (init.workspace, init.sandbox);
        let planned = {
            let state = self.shared.lock();
            lookup(&state, &key)?;
            state.forward_error.clone()
        };
        match planned {
            Some(status) => Ok(Response::new(Box::pin(stream::iter([Err(status)])))),
            None => Err(Status::unimplemented("forward_tcp")),
        }
    }
}
