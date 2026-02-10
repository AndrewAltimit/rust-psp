//! Controller input with state change detection.
//!
//! Wraps `sceCtrlReadBufferPositive` with a high-level [`Controller`] that
//! tracks previous/current state for press/release detection and provides
//! normalized analog stick values with deadzone support.
//!
//! # Example
//!
//! ```ignore
//! use psp::input::Controller;
//! use psp::sys::CtrlButtons;
//!
//! psp::input::enable_analog();
//! let mut ctrl = Controller::new();
//!
//! loop {
//!     ctrl.update();
//!     if ctrl.is_pressed(CtrlButtons::CROSS) {
//!         // just pressed this frame
//!     }
//!     let x = ctrl.analog_x_f32(0.2);
//!     // x is -1.0..1.0 with 20% deadzone
//! }
//! ```

use crate::sys::{CtrlButtons, CtrlMode, SceCtrlData, sceCtrlReadBufferPositive};

/// Initialize analog input mode.
///
/// Call this once at startup before reading the analog stick.
/// Sets the sampling cycle to 0 (default) and mode to Analog.
pub fn enable_analog() {
    unsafe {
        crate::sys::sceCtrlSetSamplingCycle(0);
        crate::sys::sceCtrlSetSamplingMode(CtrlMode::Analog);
    }
}

/// High-level controller input with state change detection.
///
/// Call [`update()`](Self::update) once per frame to refresh the state,
/// then query buttons and analog stick.
pub struct Controller {
    current: SceCtrlData,
    previous: SceCtrlData,
}

impl Controller {
    /// Create a new controller with zeroed initial state.
    pub fn new() -> Self {
        Self {
            current: SceCtrlData::default(),
            previous: SceCtrlData::default(),
        }
    }

    /// Read the current controller state.
    ///
    /// Must be called once per frame for press/release detection to work.
    pub fn update(&mut self) {
        self.previous = self.current;
        unsafe {
            sceCtrlReadBufferPositive(&mut self.current, 1);
        }
    }

    /// Returns `true` if the button is currently held down.
    pub fn is_held(&self, button: CtrlButtons) -> bool {
        self.current.buttons.contains(button)
    }

    /// Returns `true` if the button was just pressed this frame.
    ///
    /// (Down now, was not down last frame.)
    pub fn is_pressed(&self, button: CtrlButtons) -> bool {
        self.current.buttons.contains(button) && !self.previous.buttons.contains(button)
    }

    /// Returns `true` if the button was just released this frame.
    ///
    /// (Not down now, was down last frame.)
    pub fn is_released(&self, button: CtrlButtons) -> bool {
        !self.current.buttons.contains(button) && self.previous.buttons.contains(button)
    }

    /// Raw analog stick X value (0..=255, 128 is center).
    pub fn analog_x(&self) -> u8 {
        self.current.lx
    }

    /// Raw analog stick Y value (0..=255, 128 is center).
    pub fn analog_y(&self) -> u8 {
        self.current.ly
    }

    /// Normalized analog X in -1.0..=1.0 with deadzone.
    ///
    /// `deadzone` is the fraction of travel to ignore (e.g. 0.2 = 20%).
    /// Returns 0.0 if within the deadzone.
    pub fn analog_x_f32(&self, deadzone: f32) -> f32 {
        normalize_axis(self.current.lx, deadzone)
    }

    /// Normalized analog Y in -1.0..=1.0 with deadzone.
    pub fn analog_y_f32(&self, deadzone: f32) -> f32 {
        normalize_axis(self.current.ly, deadzone)
    }

    /// Access the raw current controller data.
    pub fn raw(&self) -> &SceCtrlData {
        &self.current
    }

    /// Access the raw previous-frame controller data.
    pub fn raw_previous(&self) -> &SceCtrlData {
        &self.previous
    }
}

impl Default for Controller {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalize a raw 0..=255 axis value to -1.0..=1.0 with deadzone.
fn normalize_axis(raw: u8, deadzone: f32) -> f32 {
    // Map 0..255 to -1.0..1.0 (128 is center)
    let normalized = (raw as f32 - 128.0) / 127.0;
    let abs = if normalized < 0.0 {
        -normalized
    } else {
        normalized
    };
    if abs < deadzone {
        0.0
    } else {
        // Remap so the edge of the deadzone maps to 0.0
        let sign = if normalized < 0.0 { -1.0 } else { 1.0 };
        let remapped = (abs - deadzone) / (1.0 - deadzone);
        // Clamp to 1.0 (raw=0 or raw=255 can slightly exceed 1.0)
        let clamped = if remapped > 1.0 { 1.0 } else { remapped };
        sign * clamped
    }
}
