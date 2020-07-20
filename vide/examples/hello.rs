use vide::*;

struct Handler {}

impl UiEventHandler for Handler {
    fn handle_ui_event(&self, event: UiEvent) {
        if let UiEvent::Quit = event {
            std::process::exit(0x00);
        }
    }
}

fn main() {
    ui_loop(Handler { }, (64, 64));
}
