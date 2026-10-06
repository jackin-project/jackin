// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) fn graph() -> WorkspaceGraph {
    WorkspaceGraph {
        names: BTreeMap::from([
            ("core-id".into(), "core".into()),
            ("runtime-id".into(), "runtime".into()),
            ("app-id".into(), "jackin".into()),
            ("tool-id".into(), "tool".into()),
        ]),
        package_names: BTreeMap::from([
            ("core-id".into(), "core".into()),
            ("runtime-id".into(), "runtime".into()),
            ("app-id".into(), "jackin".into()),
            ("tool-id".into(), "tool".into()),
            ("serde 1.0.0".into(), "serde".into()),
        ]),
        roots: BTreeMap::from([
            ("core-id".into(), PathBuf::from("crates/core")),
            ("runtime-id".into(), PathBuf::from("crates/runtime")),
            ("app-id".into(), PathBuf::from("crates/app")),
            ("tool-id".into(), PathBuf::from("crates/tool")),
        ]),
        dependents: BTreeMap::from([
            ("core-id".into(), BTreeSet::from(["runtime-id".into()])),
            ("runtime-id".into(), BTreeSet::from(["app-id".into()])),
        ]),
        resolved_dependencies: BTreeMap::from([
            ("core-id".into(), BTreeSet::from(["serde 1.0.0".into()])),
            ("runtime-id".into(), BTreeSet::from(["core-id".into()])),
            ("app-id".into(), BTreeSet::from(["runtime-id".into()])),
        ]),
        resolved_features: BTreeMap::from([(
            "serde 1.0.0".into(),
            BTreeSet::from(["derive".into()]),
        )]),
    }
}
