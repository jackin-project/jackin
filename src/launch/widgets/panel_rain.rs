//! Area-bounded phosphor rain. Uses the same `RainState` engine as
//! src/tui/animation.rs, rendered into a sub-rect rather than fullscreen.

use ratatui::{Frame, layout::Rect};

use crate::tui::animation;

pub struct PanelRainState {
    rain: animation::RainState,
    tick_count: u64,
}

impl std::fmt::Debug for PanelRainState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PanelRainState")
            .field("tick_count", &self.tick_count)
            .finish_non_exhaustive()
    }
}

impl PanelRainState {
    pub fn new(cols: u16, rows: u16) -> Self {
        Self {
            rain: animation::RainState::new(cols as usize, rows as usize),
            tick_count: 0,
        }
    }

    /// Tick once per frame. Caller controls frame rate (target ~20fps).
    pub fn tick(&mut self) {
        animation::tick_rain(&mut self.rain);
        self.tick_count += 1;
    }
}

pub fn render(frame: &mut Frame, area: Rect, state: &mut PanelRainState) {
    // Resize state if the rect has changed shape.
    if state.rain.cols != area.width as usize || state.rain.rows != area.height as usize {
        *state = PanelRainState::new(area.width, area.height);
    }
    // Tick is not called here — leave frame-rate control to the caller
    // so a slow paint doesn't starve tick.
    animation::render_rain_frame(&state.rain, (area.x, area.y, area.width, area.height));
    let _ = frame; // Frame is accepted for symmetry with other render fns.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_rain_sized_to_area() {
        let s = PanelRainState::new(20, 10);
        assert_eq!(s.rain.cols, 20);
        assert_eq!(s.rain.rows, 10);
    }

    #[test]
    fn tick_advances_frame_count() {
        let mut s = PanelRainState::new(5, 5);
        let before = s.tick_count;
        s.tick();
        assert_eq!(s.tick_count, before + 1);
    }
}
