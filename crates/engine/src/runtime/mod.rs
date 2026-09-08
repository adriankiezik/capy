mod actions;
mod application;
mod config;
mod error;
mod input;
mod window;

pub use actions::{Actions, Binding, InputContext, InputContextId, InputMap};
pub use anyhow::Result as ApplicationResult;
pub use application::{Engine, Frame, Tick, View, run};
pub use config::{RuntimeSettings, Settings};
pub use error::{Result, RuntimeError};
pub use input::{Axis2Binding, AxisBinding, Button, ButtonBinding, ButtonState, Input};
pub use window::{Window, WindowAction, WindowControls, WindowError, WindowSettings};
pub use winit::{
    dpi::LogicalSize,
    event::MouseButton,
    keyboard::{KeyCode, PhysicalKey},
    window::Fullscreen,
};
