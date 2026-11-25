use std::rc::Rc;

pub fn show(_on_input: Rc<dyn Fn()>, _alarm: bool, _flash: bool) {}
pub fn hide() {}
pub fn listen() {}
pub fn window() -> Option<gtk::Window> {
    None
}
