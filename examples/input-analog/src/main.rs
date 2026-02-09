//! Controller input with analog deadzone normalization.

#![no_std]
#![no_main]

use psp::input::{self, Controller};
use psp::sys::CtrlButtons;

psp::module!("input_analog_example", 1, 1);

const DEADZONE: f32 = 0.2;

fn psp_main() {
    psp::enable_home_button();
    input::enable_analog();

    let mut ctrl = Controller::new();

    psp::dprintln!("Move the analog stick or press CROSS. START exits.");

    loop {
        ctrl.update();

        if ctrl.is_pressed(CtrlButtons::START) {
            psp::dprintln!("START pressed, exiting.");
            break;
        }

        if ctrl.is_pressed(CtrlButtons::CROSS) {
            psp::dprintln!("CROSS pressed!");
        }

        let x = ctrl.analog_x_f32(DEADZONE);
        let y = ctrl.analog_y_f32(DEADZONE);

        if x != 0.0 || y != 0.0 {
            // Scale to integer display since PSP debug print has no float formatting
            let xi = (x * 100.0) as i32;
            let yi = (y * 100.0) as i32;
            psp::dprintln!("Analog: x={} y={} (x100)", xi, yi);
        }

        psp::display::wait_vblank();
    }
}
