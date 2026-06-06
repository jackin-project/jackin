//! Generic list picker state helpers.

/// Wrap-around cursor move for any list-style picker. `delta` is `-1`
/// for Up, `+1` for Down. No-op when `count == 0`.
pub fn cycle_select(list_state: &mut tui_widget_list::ListState, count: usize, delta: i32) {
    if count == 0 {
        return;
    }
    let cur = list_state.selected.unwrap_or(0);
    let next = if delta < 0 {
        if cur == 0 { count - 1 } else { cur - 1 }
    } else if cur + 1 >= count {
        0
    } else {
        cur + 1
    };
    list_state.select(Some(next));
}

#[must_use]
pub fn list_state_for_count(count: usize) -> tui_widget_list::ListState {
    let mut list_state = tui_widget_list::ListState::default();
    list_state.select(first_selection(count));
    list_state
}

#[must_use]
pub fn selected_choice<T>(choices: &[T], selected: Option<usize>) -> Option<&T> {
    selected.and_then(|index| choices.get(index))
}

#[must_use]
pub fn matches_filter<S>(filter: &str, values: impl IntoIterator<Item = S>) -> bool
where
    S: AsRef<str>,
{
    if filter.is_empty() {
        return true;
    }
    let needle = filter.to_lowercase();
    values
        .into_iter()
        .any(|value| value.as_ref().to_lowercase().contains(&needle))
}

#[must_use]
pub const fn first_selection(count: usize) -> Option<usize> {
    if count == 0 { None } else { Some(0) }
}

#[must_use]
pub const fn clamp_selection(selected: Option<usize>, count: usize) -> Option<usize> {
    if count == 0 {
        None
    } else if let Some(selected) = selected {
        if selected >= count {
            Some(count - 1)
        } else {
            Some(selected)
        }
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
