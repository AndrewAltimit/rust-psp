use psp::test_runner::TestRunner;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const BEFORE_PSP: Duration = Duration::from_secs(1000212400);

pub fn test_main(test_runner: &mut TestRunner) {
    let now = SystemTime::now();
    test_runner.check_list(&[
        ("system_time_sane", (now - BEFORE_PSP) > UNIX_EPOCH, true),
        ("instant_increments", Instant::now() < Instant::now(), true),
    ]);
}
