mod actions;
mod state;

pub use actions::{Actions, Binding, InputContext, InputContextId, InputMap};
pub use state::{Axis2Binding, AxisBinding, Button, ButtonBinding, ButtonState, Input};
pub use winit::{
    event::MouseButton,
    keyboard::{KeyCode, PhysicalKey},
};

#[cfg(test)]
mod tests;
