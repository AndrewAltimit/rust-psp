//! Spawn threads sharing a SpinMutex counter.

#![no_std]
#![no_main]

use psp::sync::SpinMutex;
use psp::thread;

psp::module!("thread_sync_example", 1, 1);

static COUNTER: SpinMutex<u32> = SpinMutex::new(0);

const THREAD_COUNT: usize = 4;
const INCREMENTS: u32 = 100;

fn psp_main() {
    psp::enable_home_button();

    psp::dprintln!(
        "Spawning {} threads, each incrementing {} times",
        THREAD_COUNT,
        INCREMENTS
    );

    let mut handles = [const { None }; THREAD_COUNT];
    let names: [&[u8]; THREAD_COUNT] = [b"worker_0\0", b"worker_1\0", b"worker_2\0", b"worker_3\0"];

    for i in 0..THREAD_COUNT {
        match thread::spawn(names[i], || {
            for _ in 0..INCREMENTS {
                *COUNTER.lock() += 1;
            }
            0
        }) {
            Ok(h) => handles[i] = Some(h),
            Err(e) => {
                psp::dprintln!("Failed to spawn thread {}: {:?}", i, e);
                return;
            },
        }
    }

    for (i, slot) in handles.into_iter().enumerate() {
        if let Some(h) = slot {
            match h.join() {
                Ok(code) => psp::dprintln!("Thread {} exited with code {}", i, code),
                Err(e) => psp::dprintln!("Thread {} join failed: {:?}", i, e),
            }
        }
    }

    let total = *COUNTER.lock();
    psp::dprintln!(
        "Final counter value: {} (expected {})",
        total,
        THREAD_COUNT as u32 * INCREMENTS
    );
}
