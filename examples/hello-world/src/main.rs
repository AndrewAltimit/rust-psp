#![no_std]
#![no_main]

psp::module!("sample_module", 1, 1);

fn psp_main() {
    psp::callback::setup_exit_callback().unwrap();
    psp::dprint!("Hello PSP from rust!");
}
