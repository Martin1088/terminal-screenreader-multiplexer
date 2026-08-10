use accesskit::{ActionHandler, TreeUpdate};
pub struct Adapter;

impl Adapter {
    pub fn new(_action_handler: impl ActionHandler + Send + 'static) -> Self {
        Self
    }

    pub fn update_if_active(&mut self, update_factory: impl FnOnce() -> TreeUpdate) {
        let _ = update_factory();
    }
}
