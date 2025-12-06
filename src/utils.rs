use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();

pub fn file_id() -> u64 {
    let start = START.get_or_init(Instant::now);
    let nanos = start.elapsed().as_nanos() as u64;
    1_000_000_000 + (nanos % 9_000_000_000)
}
