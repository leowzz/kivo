use std::time::{Duration, Instant};

use sysinfo::System;

use super::{
    DisplayItem, DisplayPriority, DisplayProvider, DisplayState, ProviderUpdate, SourceHealth,
};

const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

pub(super) struct CpuDisplayProvider {
    system: System,
    sampled_at: Instant,
    item: Option<DisplayItem>,
}

impl CpuDisplayProvider {
    pub(super) fn new() -> Self {
        let mut system = System::new();
        // The first refresh establishes the baseline for usage deltas.
        system.refresh_cpu_usage();
        Self {
            system,
            sampled_at: Instant::now(),
            item: None,
        }
    }
}

impl DisplayProvider for CpuDisplayProvider {
    fn source_id(&self) -> &'static str {
        "system"
    }

    fn poll(&mut self, now: Instant) -> Result<ProviderUpdate, &'static str> {
        if now.saturating_duration_since(self.sampled_at)
            >= SAMPLE_INTERVAL.max(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL)
        {
            self.system.refresh_cpu_usage();
            self.sampled_at = now;
            let usage = self.system.global_cpu_usage();
            if self.system.cpus().is_empty() || !usage.is_finite() {
                self.item = None;
                return Err("cpu_usage_unavailable");
            }
            self.item = Some(
                DisplayItem::new(
                    "system.cpu",
                    "system",
                    DisplayPriority::Ambient,
                    DisplayState::Idle,
                    "CPU",
                )?
                .with_metric("cpu_percent", usage.round().clamp(0.0, 100.0) as u32)
                .with_updated_at(now)
                .with_expiry(now + Duration::from_secs(3)),
            );
        }
        let item = self.item.as_ref().ok_or("cpu_usage_unavailable")?;
        Ok(ProviderUpdate {
            source: "system",
            health: SourceHealth::Healthy,
            items: vec![item.clone()],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_provider_waits_for_a_real_sample_and_caches_it_between_refreshes() {
        let mut provider = CpuDisplayProvider::new();
        assert!(matches!(
            provider.poll(Instant::now()),
            Err("cpu_usage_unavailable")
        ));
        std::thread::sleep(SAMPLE_INTERVAL.max(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL));
        let now = Instant::now();
        let sample = provider.poll(now).unwrap();
        assert_eq!(sample.source, "system");
        assert_eq!(sample.health, SourceHealth::Healthy);
        assert!(sample.items[0].metrics["cpu_percent"] <= 100);
        assert_eq!(
            sample.items[0].expires_at,
            Some(now + Duration::from_secs(3))
        );
        let cached = provider.poll(now + Duration::from_millis(100)).unwrap();
        assert_eq!(sample.items, cached.items);
    }
}
