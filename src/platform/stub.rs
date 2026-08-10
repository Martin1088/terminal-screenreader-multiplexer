use crate::AppEvent;
use accesskit::{ActionHandler, TreeUpdate};
use std::sync::mpsc::Sender;

pub struct Adapter;

impl Adapter {
    pub fn new(
        _action_handler: impl ActionHandler + Send + 'static,
        _event_tx: Sender<AppEvent>,
    ) -> Self {
        Self
    }

    pub fn update_if_active(&mut self, update_factory: impl FnOnce() -> TreeUpdate + Send + 'static) {
        let _ = update_factory();
    }
}
