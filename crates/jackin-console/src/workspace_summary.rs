#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSummary {
    pub name: String,
    pub workdir: String,
    pub mount_count: usize,
    pub readonly_mount_count: usize,
    pub allowed_role_count: usize,
    pub default_role: Option<String>,
    pub last_role: Option<String>,
}

pub trait WorkspaceSummarySource {
    fn workdir(&self) -> &str;
    fn mount_count(&self) -> usize;
    fn readonly_mount_count(&self) -> usize;
    fn allowed_role_count(&self) -> usize;
    fn default_role(&self) -> Option<&str>;
    fn last_role(&self) -> Option<&str>;
}

impl WorkspaceSummary {
    pub fn from_source(name: &str, source: &impl WorkspaceSummarySource) -> Self {
        Self {
            name: name.to_string(),
            workdir: source.workdir().to_string(),
            mount_count: source.mount_count(),
            readonly_mount_count: source.readonly_mount_count(),
            allowed_role_count: source.allowed_role_count(),
            default_role: source.default_role().map(str::to_string),
            last_role: source.last_role().map(str::to_string),
        }
    }
}
