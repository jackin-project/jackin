// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use futures::stream;
use openshell_sdk::raw::proto::{
    CreateSshSessionRequest, CreateSshSessionResponse, DeletionOutcome, RevokeSshSessionRequest,
    RevokeSshSessionResponse, SandboxPhase, TcpForwardFrame, tcp_forward_frame,
};
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use super::sandbox_rpc::phase_of;
use super::server::{Fake, scope_name};
use super::service::Boxed;
use super::ssh_server::SshModel;

const BRIDGE_BYTES: usize = 64 * 1024;
const FRAME_BYTES: usize = 16 * 1024;

const FINGERPRINT: &str = "SHA256:in-process-host-key";

fn lookup(state: &super::server::State, key: &(String, String)) -> Result<(), Status> {
    let sandbox = state
        .sandboxes
        .get(key)
        .ok_or_else(|| Status::not_found("sandbox not found"))?;
    let phase = phase_of(sandbox);
    if [
        SandboxPhase::Ready as i32,
        SandboxPhase::Error as i32,
        SandboxPhase::Completed as i32,
    ]
    .contains(&phase)
    {
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
        let fingerprint = state.ssh.as_ref().map_or(FINGERPRINT, |model| {
            model.note_created(&token);
            model.advertised()
        });
        Ok(Response::new(CreateSshSessionResponse {
            sandbox_id: key.1,
            token,
            gateway_host: "127.0.0.1".to_owned(),
            gateway_port: 1,
            gateway_scheme: "https".to_owned(),
            host_key_fingerprint: fingerprint.to_owned(),
            ..CreateSshSessionResponse::default()
        }))
    }

    pub(crate) async fn revoke_ssh_session_impl(
        &self,
        request: Request<RevokeSshSessionRequest>,
    ) -> Result<Response<RevokeSshSessionResponse>, Status> {
        self.enter("revoke_ssh_session").await?;
        let model = self.shared.lock().ssh.clone();
        if let Some(model) = model {
            model.note_revoked(&request.into_inner().token);
        }
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
        let (planned, model) = {
            let state = self.shared.lock();
            lookup(&state, &key)?;
            (state.forward_error.clone(), state.ssh.clone())
        };
        match (planned, model) {
            (Some(status), _) => Ok(Response::new(Box::pin(stream::iter([Err(status)])))),
            (None, Some(model)) => Ok(Response::new(bridge(&model, inbound))),
            (None, None) => Err(Status::unimplemented("forward_tcp")),
        }
    }
}

fn bridge(
    model: &Arc<SshModel>,
    mut inbound: Streaming<TcpForwardFrame>,
) -> Boxed<TcpForwardFrame> {
    let (near, far) = tokio::io::duplex(BRIDGE_BYTES);
    model.serve(far);
    let (mut read_half, mut write_half) = tokio::io::split(near);
    let (frames, outbound) = mpsc::channel(16);
    tokio::spawn(async move {
        while let Ok(Some(frame)) = inbound.message().await {
            if let Some(tcp_forward_frame::Payload::Data(data)) = frame.payload
                && write_half.write_all(&data).await.is_err()
            {
                break;
            }
        }
        drop(write_half.shutdown().await);
    });
    tokio::spawn(async move {
        let mut buffer = vec![0_u8; FRAME_BYTES];
        while let Ok(count) = read_half.read(&mut buffer).await {
            if count == 0 {
                break;
            }
            let frame = TcpForwardFrame {
                payload: Some(tcp_forward_frame::Payload::Data(buffer[..count].to_vec())),
            };
            if frames.send(Ok(frame)).await.is_err() {
                break;
            }
        }
    });
    Box::pin(ReceiverStream::new(outbound))
}
