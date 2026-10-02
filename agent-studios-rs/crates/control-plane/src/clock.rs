use chrono::{DateTime, Duration, Utc};
use std::sync::Mutex;

/// Abstraction for providing timestamps in domain logic.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// Production clock using system UTC time.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Deterministic mock clock for tests.
pub struct FixedClock {
    current: Mutex<DateTime<Utc>>,
}

impl FixedClock {
    pub fn new(initial: DateTime<Utc>) -> Self {
        Self {
            current: Mutex::new(initial),
        }
    }

    pub fn set(&self, time: DateTime<Utc>) {
        let mut lock = self.current.lock().expect("lock fixed clock");
        *lock = time;
    }

    pub fn advance(&self, duration: Duration) {
        let mut lock = self.current.lock().expect("lock fixed clock");
        *lock += duration;
    }
}

impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.current.lock().expect("lock fixed clock")
    }
}
