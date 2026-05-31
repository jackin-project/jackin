pub use jackin_console::widgets::role_picker::{
    RoleChoice, RolePickerState as GenericRolePickerState, render,
};

impl RoleChoice for crate::selector::RoleSelector {
    fn key(&self) -> String {
        self.to_string()
    }
}

pub type RolePickerState = GenericRolePickerState<crate::selector::RoleSelector>;
