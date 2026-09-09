mod application;
mod config;
mod error;
mod window;

pub use crate::input::{Actions, Binding, InputContext, InputContextId, InputMap};
pub use crate::input::{Axis2Binding, AxisBinding, Button, ButtonBinding, ButtonState, Input};
pub use anyhow::Result as ApplicationResult;
pub use application::{Engine, Frame, InputUpdate, View, run};
pub use config::{RuntimeSettings, Settings};
pub use error::{Result, RuntimeError};
pub use window::{Window, WindowAction, WindowControls, WindowError, WindowSettings};
pub use winit::{
    dpi::LogicalSize,
    event::MouseButton,
    keyboard::{KeyCode, PhysicalKey},
    window::Fullscreen,
};
