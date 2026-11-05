// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use openshell_sdk::raw::proto::open_shell_server::OpenShell;
use std::pin::Pin;
use tonic::codegen::tokio_stream::Stream;
use tonic::{Request, Response, Status, Streaming};

use super::server::Fake;

pub(crate) type Boxed<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

#[tonic::async_trait]
impl OpenShell for Fake {
    type ExecSandboxStream = Boxed<openshell_sdk::raw::proto::ExecSandboxEvent>;
    type ForwardTcpStream = Boxed<openshell_sdk::raw::proto::TcpForwardFrame>;
    type ExecSandboxInteractiveStream = Boxed<openshell_sdk::raw::proto::ExecSandboxEvent>;
    type ConnectSupervisorStream = Boxed<openshell_sdk::raw::proto::GatewayMessage>;
    type RelayStreamStream = Boxed<openshell_sdk::raw::proto::RelayFrame>;
    type PeerRelayStream = Boxed<openshell_sdk::raw::proto::PeerRelayFrame>;
    type WatchSandboxStream = Boxed<openshell_sdk::raw::proto::SandboxStreamEvent>;

    async fn health(
        &self,
        request: Request<openshell_sdk::raw::proto::HealthRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::HealthResponse>, Status> {
        self.health_impl(request).await
    }

    async fn get_current_user(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetCurrentUserRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetCurrentUserResponse>, Status> {
        Err(Status::unimplemented("get_current_user"))
    }

    async fn get_gateway_info(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetGatewayInfoRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetGatewayInfoResponse>, Status> {
        Err(Status::unimplemented("get_gateway_info"))
    }

    async fn create_sandbox(
        &self,
        request: Request<openshell_sdk::raw::proto::CreateSandboxRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::SandboxResponse>, Status> {
        self.create_sandbox_impl(request).await
    }

    async fn begin_rootfs_tar_staging(
        &self,
        _request: Request<openshell_sdk::raw::proto::BeginRootfsTarStagingRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::BeginRootfsTarStagingResponse>, Status> {
        Err(Status::unimplemented("begin_rootfs_tar_staging"))
    }

    async fn get_sandbox(
        &self,
        request: Request<openshell_sdk::raw::proto::GetSandboxRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::SandboxResponse>, Status> {
        self.get_sandbox_impl(request).await
    }

    async fn list_sandboxes(
        &self,
        request: Request<openshell_sdk::raw::proto::ListSandboxesRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ListSandboxesResponse>, Status> {
        self.list_sandboxes_impl(request).await
    }

    async fn create_sandbox_template(
        &self,
        _request: Request<openshell_sdk::raw::proto::CreateSandboxTemplateRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::SandboxTemplateResponse>, Status> {
        Err(Status::unimplemented("create_sandbox_template"))
    }

    async fn get_sandbox_template(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetSandboxTemplateRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::SandboxTemplateResponse>, Status> {
        Err(Status::unimplemented("get_sandbox_template"))
    }

    async fn list_sandbox_templates(
        &self,
        _request: Request<openshell_sdk::raw::proto::ListSandboxTemplatesRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ListSandboxTemplatesResponse>, Status> {
        Err(Status::unimplemented("list_sandbox_templates"))
    }

    async fn delete_sandbox_template(
        &self,
        _request: Request<openshell_sdk::raw::proto::DeleteSandboxTemplateRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::DeleteSandboxTemplateResponse>, Status> {
        Err(Status::unimplemented("delete_sandbox_template"))
    }

    async fn list_sandbox_providers(
        &self,
        request: Request<openshell_sdk::raw::proto::ListSandboxProvidersRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ListSandboxProvidersResponse>, Status> {
        self.list_sandbox_providers_impl(request).await
    }

    async fn attach_sandbox_provider(
        &self,
        _request: Request<openshell_sdk::raw::proto::AttachSandboxProviderRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::AttachSandboxProviderResponse>, Status> {
        Err(Status::unimplemented("attach_sandbox_provider"))
    }

    async fn detach_sandbox_provider(
        &self,
        _request: Request<openshell_sdk::raw::proto::DetachSandboxProviderRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::DetachSandboxProviderResponse>, Status> {
        Err(Status::unimplemented("detach_sandbox_provider"))
    }

    async fn get_sandbox_provider_status(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetSandboxProviderStatusRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetSandboxProviderStatusResponse>, Status> {
        Err(Status::unimplemented("get_sandbox_provider_status"))
    }

    async fn delete_sandbox(
        &self,
        request: Request<openshell_sdk::raw::proto::DeleteSandboxRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::DeleteSandboxResponse>, Status> {
        self.delete_sandbox_impl(request).await
    }

    async fn stop_sandbox(
        &self,
        request: Request<openshell_sdk::raw::proto::StopSandboxRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::SandboxResponse>, Status> {
        self.stop_sandbox_impl(request).await
    }

    async fn start_sandbox(
        &self,
        request: Request<openshell_sdk::raw::proto::StartSandboxRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::SandboxResponse>, Status> {
        self.start_sandbox_impl(request).await
    }

    async fn suspend_sandbox(
        &self,
        _request: Request<openshell_sdk::raw::proto::SuspendSandboxRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::SandboxResponse>, Status> {
        Err(Status::unimplemented("suspend_sandbox"))
    }

    async fn resume_sandbox(
        &self,
        _request: Request<openshell_sdk::raw::proto::ResumeSandboxRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::SandboxResponse>, Status> {
        Err(Status::unimplemented("resume_sandbox"))
    }

    async fn create_ssh_session(
        &self,
        request: Request<openshell_sdk::raw::proto::CreateSshSessionRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::CreateSshSessionResponse>, Status> {
        self.create_ssh_session_impl(request).await
    }

    async fn expose_service(
        &self,
        _request: Request<openshell_sdk::raw::proto::ExposeServiceRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ServiceEndpointResponse>, Status> {
        Err(Status::unimplemented("expose_service"))
    }

    async fn get_service(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetServiceRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ServiceEndpointResponse>, Status> {
        Err(Status::unimplemented("get_service"))
    }

    async fn list_services(
        &self,
        _request: Request<openshell_sdk::raw::proto::ListServicesRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ListServicesResponse>, Status> {
        Err(Status::unimplemented("list_services"))
    }

    async fn delete_service(
        &self,
        _request: Request<openshell_sdk::raw::proto::DeleteServiceRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::DeleteServiceResponse>, Status> {
        Err(Status::unimplemented("delete_service"))
    }

    async fn revoke_ssh_session(
        &self,
        request: Request<openshell_sdk::raw::proto::RevokeSshSessionRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::RevokeSshSessionResponse>, Status> {
        self.revoke_ssh_session_impl(request).await
    }

    async fn exec_sandbox(
        &self,
        _request: Request<openshell_sdk::raw::proto::ExecSandboxRequest>,
    ) -> Result<Response<Self::ExecSandboxStream>, Status> {
        Err(Status::unimplemented("exec_sandbox"))
    }

    async fn forward_tcp(
        &self,
        request: Request<Streaming<openshell_sdk::raw::proto::TcpForwardFrame>>,
    ) -> Result<Response<Self::ForwardTcpStream>, Status> {
        self.forward_tcp_impl(request).await
    }

    async fn exec_sandbox_interactive(
        &self,
        request: Request<Streaming<openshell_sdk::raw::proto::ExecSandboxInput>>,
    ) -> Result<Response<Self::ExecSandboxInteractiveStream>, Status> {
        self.exec_sandbox_interactive_impl(request).await
    }

    async fn create_provider(
        &self,
        request: Request<openshell_sdk::raw::proto::CreateProviderRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ProviderResponse>, Status> {
        self.create_provider_impl(request).await
    }

    async fn get_provider(
        &self,
        request: Request<openshell_sdk::raw::proto::GetProviderRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ProviderResponse>, Status> {
        self.get_provider_impl(request).await
    }

    async fn get_provider_credentials(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetProviderCredentialsRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetProviderCredentialsResponse>, Status> {
        Err(Status::unimplemented("get_provider_credentials"))
    }

    async fn list_providers(
        &self,
        _request: Request<openshell_sdk::raw::proto::ListProvidersRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ListProvidersResponse>, Status> {
        Err(Status::unimplemented("list_providers"))
    }

    async fn list_provider_profiles(
        &self,
        _request: Request<openshell_sdk::raw::proto::ListProviderProfilesRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ListProviderProfilesResponse>, Status> {
        Err(Status::unimplemented("list_provider_profiles"))
    }

    async fn get_provider_profile(
        &self,
        request: Request<openshell_sdk::raw::proto::GetProviderProfileRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ProviderProfileResponse>, Status> {
        self.get_provider_profile_impl(request).await
    }

    async fn import_provider_profiles(
        &self,
        request: Request<openshell_sdk::raw::proto::ImportProviderProfilesRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ImportProviderProfilesResponse>, Status> {
        self.import_provider_profiles_impl(request).await
    }

    async fn update_provider_profiles(
        &self,
        request: Request<openshell_sdk::raw::proto::UpdateProviderProfilesRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::UpdateProviderProfilesResponse>, Status> {
        self.update_provider_profiles_impl(request).await
    }

    async fn lint_provider_profiles(
        &self,
        _request: Request<openshell_sdk::raw::proto::LintProviderProfilesRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::LintProviderProfilesResponse>, Status> {
        Err(Status::unimplemented("lint_provider_profiles"))
    }

    async fn update_provider(
        &self,
        request: Request<openshell_sdk::raw::proto::UpdateProviderRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ProviderResponse>, Status> {
        self.update_provider_impl(request).await
    }

    async fn get_provider_refresh_status(
        &self,
        request: Request<openshell_sdk::raw::proto::GetProviderRefreshStatusRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetProviderRefreshStatusResponse>, Status> {
        self.get_provider_refresh_status_impl(request).await
    }

    async fn configure_provider_refresh(
        &self,
        request: Request<openshell_sdk::raw::proto::ConfigureProviderRefreshRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ConfigureProviderRefreshResponse>, Status> {
        self.configure_provider_refresh_impl(request).await
    }

    async fn rotate_provider_credential(
        &self,
        _request: Request<openshell_sdk::raw::proto::RotateProviderCredentialRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::RotateProviderCredentialResponse>, Status> {
        Err(Status::unimplemented("rotate_provider_credential"))
    }

    async fn delete_provider_refresh(
        &self,
        request: Request<openshell_sdk::raw::proto::DeleteProviderRefreshRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::DeleteProviderRefreshResponse>, Status> {
        self.delete_provider_refresh_impl(request).await
    }

    async fn delete_provider(
        &self,
        request: Request<openshell_sdk::raw::proto::DeleteProviderRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::DeleteProviderResponse>, Status> {
        self.delete_provider_impl(request).await
    }

    async fn delete_provider_profile(
        &self,
        request: Request<openshell_sdk::raw::proto::DeleteProviderProfileRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::DeleteProviderProfileResponse>, Status> {
        self.delete_provider_profile_impl(request).await
    }

    async fn get_sandbox_config(
        &self,
        request: Request<openshell_sdk::raw::proto::sandbox::v1::GetSandboxConfigRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::sandbox::v1::GetSandboxConfigResponse>, Status>
    {
        self.get_sandbox_config_impl(request).await
    }

    async fn get_gateway_config(
        &self,
        _request: Request<openshell_sdk::raw::proto::sandbox::v1::GetGatewayConfigRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::sandbox::v1::GetGatewayConfigResponse>, Status>
    {
        Err(Status::unimplemented("get_gateway_config"))
    }

    async fn update_config(
        &self,
        _request: Request<openshell_sdk::raw::proto::UpdateConfigRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::UpdateConfigResponse>, Status> {
        Err(Status::unimplemented("update_config"))
    }

    async fn get_sandbox_policy_status(
        &self,
        request: Request<openshell_sdk::raw::proto::GetSandboxPolicyStatusRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetSandboxPolicyStatusResponse>, Status> {
        self.get_sandbox_policy_status_impl(request).await
    }

    async fn list_sandbox_policies(
        &self,
        _request: Request<openshell_sdk::raw::proto::ListSandboxPoliciesRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ListSandboxPoliciesResponse>, Status> {
        Err(Status::unimplemented("list_sandbox_policies"))
    }

    async fn report_policy_status(
        &self,
        _request: Request<openshell_sdk::raw::proto::ReportPolicyStatusRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ReportPolicyStatusResponse>, Status> {
        Err(Status::unimplemented("report_policy_status"))
    }

    async fn report_endpoint_status(
        &self,
        _request: Request<openshell_sdk::raw::proto::ReportEndpointStatusRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ReportEndpointStatusResponse>, Status> {
        Err(Status::unimplemented("report_endpoint_status"))
    }

    async fn report_provider_readiness(
        &self,
        _request: Request<openshell_sdk::raw::proto::ReportProviderReadinessRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ReportProviderReadinessResponse>, Status> {
        Err(Status::unimplemented("report_provider_readiness"))
    }

    async fn report_sandbox_configuration(
        &self,
        _request: Request<openshell_sdk::raw::proto::ReportSandboxConfigurationRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ReportSandboxConfigurationResponse>, Status>
    {
        Err(Status::unimplemented("report_sandbox_configuration"))
    }

    async fn get_sandbox_provider_environment(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetSandboxProviderEnvironmentRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetSandboxProviderEnvironmentResponse>, Status>
    {
        Err(Status::unimplemented("get_sandbox_provider_environment"))
    }

    async fn exchange_provider_subject_token(
        &self,
        _request: Request<openshell_sdk::raw::proto::ExchangeProviderSubjectTokenRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ExchangeProviderSubjectTokenResponse>, Status>
    {
        Err(Status::unimplemented("exchange_provider_subject_token"))
    }

    async fn get_sandbox_logs(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetSandboxLogsRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetSandboxLogsResponse>, Status> {
        Err(Status::unimplemented("get_sandbox_logs"))
    }

    async fn push_sandbox_logs(
        &self,
        _request: Request<Streaming<openshell_sdk::raw::proto::PushSandboxLogsRequest>>,
    ) -> Result<Response<openshell_sdk::raw::proto::PushSandboxLogsResponse>, Status> {
        Err(Status::unimplemented("push_sandbox_logs"))
    }

    async fn connect_supervisor(
        &self,
        _request: Request<Streaming<openshell_sdk::raw::proto::SupervisorMessage>>,
    ) -> Result<Response<Self::ConnectSupervisorStream>, Status> {
        Err(Status::unimplemented("connect_supervisor"))
    }

    async fn report_main_process_exit(
        &self,
        _request: Request<openshell_sdk::raw::proto::ReportMainProcessExitRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ReportMainProcessExitResponse>, Status> {
        Err(Status::unimplemented("report_main_process_exit"))
    }

    async fn finalize_main_process_exit(
        &self,
        _request: Request<openshell_sdk::raw::proto::FinalizeMainProcessExitRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::FinalizeMainProcessExitResponse>, Status> {
        Err(Status::unimplemented("finalize_main_process_exit"))
    }

    async fn relay_stream(
        &self,
        _request: Request<Streaming<openshell_sdk::raw::proto::RelayFrame>>,
    ) -> Result<Response<Self::RelayStreamStream>, Status> {
        Err(Status::unimplemented("relay_stream"))
    }

    async fn peer_relay(
        &self,
        _request: Request<Streaming<openshell_sdk::raw::proto::PeerRelayFrame>>,
    ) -> Result<Response<Self::PeerRelayStream>, Status> {
        Err(Status::unimplemented("peer_relay"))
    }

    async fn peer_report_provider_readiness(
        &self,
        _request: Request<openshell_sdk::raw::proto::ReportProviderReadinessRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ReportProviderReadinessResponse>, Status> {
        Err(Status::unimplemented("peer_report_provider_readiness"))
    }

    async fn peer_report_endpoint_status(
        &self,
        _request: Request<openshell_sdk::raw::proto::ReportEndpointStatusRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ReportEndpointStatusResponse>, Status> {
        Err(Status::unimplemented("peer_report_endpoint_status"))
    }

    async fn peer_get_sandbox_provider_status(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetSandboxProviderStatusRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetSandboxProviderStatusResponse>, Status> {
        Err(Status::unimplemented("peer_get_sandbox_provider_status"))
    }

    async fn peer_notify_config_update(
        &self,
        _request: Request<openshell_sdk::raw::proto::PeerNotifyConfigUpdateRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::PeerNotifyConfigUpdateResponse>, Status> {
        Err(Status::unimplemented("peer_notify_config_update"))
    }

    async fn watch_sandbox(
        &self,
        _request: Request<openshell_sdk::raw::proto::WatchSandboxRequest>,
    ) -> Result<Response<Self::WatchSandboxStream>, Status> {
        Err(Status::unimplemented("watch_sandbox"))
    }

    async fn submit_policy_analysis(
        &self,
        _request: Request<openshell_sdk::raw::proto::SubmitPolicyAnalysisRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::SubmitPolicyAnalysisResponse>, Status> {
        Err(Status::unimplemented("submit_policy_analysis"))
    }

    async fn get_draft_policy(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetDraftPolicyRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetDraftPolicyResponse>, Status> {
        Err(Status::unimplemented("get_draft_policy"))
    }

    async fn approve_draft_chunk(
        &self,
        _request: Request<openshell_sdk::raw::proto::ApproveDraftChunkRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ApproveDraftChunkResponse>, Status> {
        Err(Status::unimplemented("approve_draft_chunk"))
    }

    async fn reject_draft_chunk(
        &self,
        _request: Request<openshell_sdk::raw::proto::RejectDraftChunkRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::RejectDraftChunkResponse>, Status> {
        Err(Status::unimplemented("reject_draft_chunk"))
    }

    async fn approve_all_draft_chunks(
        &self,
        _request: Request<openshell_sdk::raw::proto::ApproveAllDraftChunksRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ApproveAllDraftChunksResponse>, Status> {
        Err(Status::unimplemented("approve_all_draft_chunks"))
    }

    async fn edit_draft_chunk(
        &self,
        _request: Request<openshell_sdk::raw::proto::EditDraftChunkRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::EditDraftChunkResponse>, Status> {
        Err(Status::unimplemented("edit_draft_chunk"))
    }

    async fn undo_draft_chunk(
        &self,
        _request: Request<openshell_sdk::raw::proto::UndoDraftChunkRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::UndoDraftChunkResponse>, Status> {
        Err(Status::unimplemented("undo_draft_chunk"))
    }

    async fn clear_draft_chunks(
        &self,
        _request: Request<openshell_sdk::raw::proto::ClearDraftChunksRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ClearDraftChunksResponse>, Status> {
        Err(Status::unimplemented("clear_draft_chunks"))
    }

    async fn get_draft_history(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetDraftHistoryRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetDraftHistoryResponse>, Status> {
        Err(Status::unimplemented("get_draft_history"))
    }

    async fn issue_sandbox_token(
        &self,
        _request: Request<openshell_sdk::raw::proto::IssueSandboxTokenRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::IssueSandboxTokenResponse>, Status> {
        Err(Status::unimplemented("issue_sandbox_token"))
    }

    async fn refresh_sandbox_token(
        &self,
        _request: Request<openshell_sdk::raw::proto::RefreshSandboxTokenRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::RefreshSandboxTokenResponse>, Status> {
        Err(Status::unimplemented("refresh_sandbox_token"))
    }

    async fn create_workspace(
        &self,
        _request: Request<openshell_sdk::raw::proto::CreateWorkspaceRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::CreateWorkspaceResponse>, Status> {
        Err(Status::unimplemented("create_workspace"))
    }

    async fn get_workspace(
        &self,
        _request: Request<openshell_sdk::raw::proto::GetWorkspaceRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::GetWorkspaceResponse>, Status> {
        Err(Status::unimplemented("get_workspace"))
    }

    async fn list_workspaces(
        &self,
        _request: Request<openshell_sdk::raw::proto::ListWorkspacesRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ListWorkspacesResponse>, Status> {
        Err(Status::unimplemented("list_workspaces"))
    }

    async fn delete_workspace(
        &self,
        _request: Request<openshell_sdk::raw::proto::DeleteWorkspaceRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::DeleteWorkspaceResponse>, Status> {
        Err(Status::unimplemented("delete_workspace"))
    }

    async fn add_workspace_member(
        &self,
        _request: Request<openshell_sdk::raw::proto::AddWorkspaceMemberRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::AddWorkspaceMemberResponse>, Status> {
        Err(Status::unimplemented("add_workspace_member"))
    }

    async fn remove_workspace_member(
        &self,
        _request: Request<openshell_sdk::raw::proto::RemoveWorkspaceMemberRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::RemoveWorkspaceMemberResponse>, Status> {
        Err(Status::unimplemented("remove_workspace_member"))
    }

    async fn list_workspace_members(
        &self,
        _request: Request<openshell_sdk::raw::proto::ListWorkspaceMembersRequest>,
    ) -> Result<Response<openshell_sdk::raw::proto::ListWorkspaceMembersResponse>, Status> {
        Err(Status::unimplemented("list_workspace_members"))
    }
}
