//! Opt-in RSS target. This sheds view-distance work, not arbitrary live allocations.
use std::sync::Mutex;
use std::time::{Duration, Instant};

struct Budget {
    target_mib: u32,
    cap: i32,
    resident_mib: Option<u64>,
    checked: Option<Instant>,
    cool_samples: u8,
}
impl Budget {
    const fn new(target_mib: u32) -> Self {
        Self {
            target_mib,
            cap: 32,
            resident_mib: None,
            checked: None,
            cool_samples: 0,
        }
    }
    fn observe(&mut self, resident_mib: u64, requested: i32) {
        self.resident_mib = Some(resident_mib);
        if self.target_mib == 0 {
            return;
        }
        let target = u64::from(self.target_mib);
        if resident_mib >= target * 9 / 10 {
            // Work below the target so outstanding jobs have some headroom.
            self.cap = self.cap.min(requested).saturating_sub(2).max(2);
            self.cool_samples = 0;
        } else if resident_mib < target * 3 / 4 {
            self.cool_samples += 1;
            if self.cool_samples >= 5 {
                self.cap = (self.cap + 1).min(32);
                self.cool_samples = 0;
            }
        } else {
            self.cool_samples = 0;
        }
    }
}
static BUDGET: Mutex<Budget> = Mutex::new(Budget::new(0));

pub fn configure(target_mib: u32) {
    let target = if target_mib == 0 {
        0
    } else {
        target_mib.clamp(512, 16384)
    };
    *BUDGET
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Budget::new(target);
}

fn resident_mib(status: &str) -> Option<u64> {
    let line = status.lines().find(|line| line.starts_with("VmRSS:"))?;
    let mut fields = line.split_whitespace();
    fields.next()?;
    let kib: u64 = fields.next()?.parse().ok()?;
    (fields.next()? == "kB").then_some(kib.div_ceil(1024))
}

fn total_mib(info: &str) -> Option<u64> {
    let line = info.lines().find(|line| line.starts_with("MemTotal:"))?;
    let mut fields = line.split_whitespace();
    fields.next()?;
    let kib: u64 = fields.next()?.parse().ok()?;
    (fields.next()? == "kB").then_some(kib.div_ceil(1024))
}
/// MemTotal excludes reserved hardware memory. Include modest rounding headroom.
pub fn low_memory_device() -> bool {
    cfg!(target_os = "android")
        && std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|s| total_mib(&s))
            .is_some_and(|mib| mib > 0 && mib <= 2304)
}

/// Desktop behavior stays unchanged. Android samples once every two seconds,
/// shared by the client and integrated server; never writes the player's options.
pub fn render_distance(requested: i32) -> i32 {
    if !cfg!(target_os = "android") {
        return requested;
    }
    let mut budget = BUDGET
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if budget.target_mib == 0 {
        return requested;
    }
    let now = Instant::now();
    if budget
        .checked
        .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(2))
    {
        budget.checked = Some(now);
        if let Some(resident) = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| resident_mib(&s))
        {
            budget.observe(resident, requested.clamp(2, 32));
        }
    }
    requested.min(budget.cap)
}

pub fn snapshot() -> (u32, Option<u64>, i32) {
    let budget = BUDGET
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    (budget.target_mib, budget.resident_mib, budget.cap)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physical_memory_uses_total_not_currently_free_ram() {
        assert_eq!(
            total_mib("MemTotal: 1900544 kB\nMemAvailable: 102400 kB\n"),
            Some(1856)
        );
        assert_eq!(
            total_mib("MemTotal: 4194304 kB\nMemFree: 102400 kB\n"),
            Some(4096)
        );
        assert_eq!(total_mib("MemAvailable: 900000 kB\n"), None);
    }
    #[test]
    fn status_requires_rss_and_correct_units() {
        assert_eq!(
            resident_mib("VmSize: 9000000 kB\nVmRSS:\t1048577 kB\n"),
            Some(1025)
        );
        assert_eq!(resident_mib("VmSize: 10000 kB\n"), None);
        assert_eq!(resident_mib("VmRSS: 1000 bytes"), None);
        assert_eq!(resident_mib("VmRSS: invalid kB"), None);
    }
    #[test]
    fn pressure_reduces_chunk_area_and_recovers_slowly_without_oscillation() {
        let mut budget = Budget::new(1024);
        budget.observe(940, 12);
        assert_eq!(budget.cap, 10);
        for _ in 0..20 {
            budget.observe(1024, 12);
        }
        assert_eq!(budget.cap, 2);
        for _ in 0..4 {
            budget.observe(700, 12);
        }
        assert_eq!(budget.cap, 2);
        budget.observe(800, 12); // Headroom must be sustained, not cumulative.
        for _ in 0..4 {
            budget.observe(700, 12);
        }
        assert_eq!(budget.cap, 2);
        budget.observe(700, 12);
        assert_eq!(budget.cap, 3);
        for _ in 0..200 {
            budget.observe(600, 12);
        }
        assert_eq!(budget.cap, 32);
    }
    #[test]
    fn disabled_guard_keeps_the_requested_view() {
        let mut budget = Budget::new(0);
        budget.observe(16000, 16);
        assert_eq!(budget.cap, 32);
    }
}
