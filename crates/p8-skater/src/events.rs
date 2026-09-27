//! Synchronous physics-to-script calls, with a separate log for callers.
//!
//! Research notes identify LaunchEvent (`82225758` -> `82228628`) as an
//! immediate call into the handler dispatcher (`82224D78`). Keeping a log
//! must not defer the handler's changes until after physics finishes.
use crate::core_physics::{CorePhysics, Event};

pub(crate) enum ScriptAction {
    Event(Event),
    Goto(u32),
}

type Dispatch<'a> = dyn FnMut(&mut CorePhysics, ScriptAction) -> Vec<Event> + 'a;

#[derive(Default)]
pub(crate) struct PhysicsEvents<'a> {
    pub log: Vec<Event>,
    dispatch: Option<&'a mut Dispatch<'a>>,
}

impl<'a> PhysicsEvents<'a> {
    pub fn new(dispatch: &'a mut Dispatch<'a>) -> Self {
        Self { log: Vec::new(), dispatch: Some(dispatch) }
    }

    pub fn emit(&mut self, physics: &mut CorePhysics, event: Event) {
        self.log.push(event);
        if let Some(dispatch) = self.dispatch.as_mut() {
            self.log.extend(dispatch(physics, ScriptAction::Event(event)));
        }
    }

    pub fn goto(&mut self, physics: &mut CorePhysics, name: u32) {
        if let Some(dispatch) = self.dispatch.as_mut() {
            self.log.extend(dispatch(physics, ScriptAction::Goto(name)));
        } else {
            // Retain the standalone physics API's notification. The scripted
            // path executes at this call site instead of storing a request.
            physics.script_goto = Some(name);
        }
    }
}
