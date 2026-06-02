#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderPickerState<C, A, P> {
    pub context: C,
    pub agent: A,
    // Private so the `selected < providers.len()` invariant holds: `selected`
    // is only ever moved by the clamping `move_up`/`move_down`, and `providers`
    // is set once at construction. External code reads them via the accessors.
    providers: Vec<P>,
    selected: usize,
}

impl<C, A, P> ProviderPickerState<C, A, P> {
    pub const fn new(context: C, agent: A, providers: Vec<P>) -> Self {
        Self {
            context,
            agent,
            providers,
            selected: 0,
        }
    }

    pub const fn move_up(&mut self) {
        self.selected = self.selected.saturating_sub(1);
    }

    pub const fn move_down(&mut self) {
        if self.selected + 1 < self.providers.len() {
            self.selected += 1;
        }
    }

    #[must_use]
    pub fn providers(&self) -> &[P] {
        &self.providers
    }

    #[must_use]
    pub const fn selected(&self) -> usize {
        self.selected
    }

    #[must_use]
    pub fn selected_provider(&self) -> Option<P>
    where
        P: Copy,
    {
        self.providers.get(self.selected).copied()
    }
}
