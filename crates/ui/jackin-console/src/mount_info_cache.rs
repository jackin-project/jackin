// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

#[cfg(test)]
use crate::mount_info::inspect;
use crate::mount_info::{GitOrigin, MountKind};

pub trait MountSource {
    fn mount_src(&self) -> &str;
}

#[derive(Clone, Debug, Default)]
pub struct MountInfoCache {
    entries: Rc<RefCell<BTreeMap<String, MountKind>>>,
}

impl MountInfoCache {
    #[cfg(test)]
    pub fn refresh_src(&self, src: &str) {
        let kind = inspect(src);
        self.entries.borrow_mut().insert(src.to_owned(), kind);
    }

    pub fn store_entries(&self, entries: impl IntoIterator<Item = (String, MountKind)>) {
        self.entries.borrow_mut().extend(entries);
    }

    #[cfg(test)]
    pub fn refresh_mounts(&self, mounts: &[impl MountSource]) {
        for mount in mounts {
            self.refresh_src(mount.mount_src());
        }
    }

    pub fn inspect_cached(&self, src: &str) -> Option<MountKind> {
        self.entries.borrow().get(src).cloned()
    }

    pub fn label(&self, src: &str) -> String {
        self.inspect_cached(src)
            .map_or_else(|| "unknown".to_owned(), |kind| kind.label())
    }

    pub fn github_web_url(&self, src: &str) -> Option<String> {
        match self.inspect_cached(src)? {
            MountKind::Git {
                origin: Some(GitOrigin::Github { web_url, .. }),
                ..
            } => Some(web_url),
            MountKind::Missing | MountKind::Folder | MountKind::Git { .. } => None,
        }
    }

    pub fn clear(&self) {
        self.entries.borrow_mut().clear();
    }
}

impl MountSource for jackin_config::MountConfig {
    fn mount_src(&self) -> &str {
        &self.src
    }
}

impl MountSource for jackin_config::GlobalMountRow {
    fn mount_src(&self) -> &str {
        &self.mount.src
    }
}
