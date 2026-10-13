// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

//! Tests for `universe`.

use super::super::coordination;
use super::*;

use jackin_docker::docker_client::{
    ContainerHandle, ContainerInspection, ContainerRow, ContainerSpec, ContainerState, DockerApi,
    NetworkRow, RemoveImageOutcome,
};

use jackin_test_support::FakeDockerClient;

use std::collections::{HashMap, VecDeque};

#[cfg(unix)]
macro_rules! forward_docker_api_methods {
    ($($method:ident($($argument:ident: $argument_type:ty),*) -> $output:ty),* $(,)?) => {
        $(
            async fn $method(&self $(, $argument: $argument_type)*) -> $output {
                self.fake.$method($($argument),*).await
            }
        )*
    };
}

mod support;
use support::*;
mod case_01;
mod case_02;
mod case_03;
mod case_04;
