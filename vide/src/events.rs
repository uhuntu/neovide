pub enum Direction {
    Up, Right, Down, Left
}

pub enum UiEvent {
    Quit,
    KeyboardInput(String),
    MouseDragged(u32, u32),
    MousePressed(u32, u32),
    MouseReleased(u32, u32),
    Scroll(Direction, u32, u32),
    FocusLost,
    FocusGained
}

pub trait UiEventHandler {
    fn handle_ui_event(&self, event: UiEvent);
}
