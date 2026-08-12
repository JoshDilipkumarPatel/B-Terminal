use rand::Rng;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{warn, error};

/// Fault injector middleware for Chaos Engineering.
/// Simulates adverse network conditions to ensure the system degrades gracefully.
pub struct TokioFaultInjector {
    pub packet_loss_probability: f64,
    pub latency_spike_probability: f64,
    pub max_latency_spike_ms: u64,
}

impl Default for TokioFaultInjector {
    fn default() -> Self {
        Self {
            packet_loss_probability: 0.05, // 5% packet loss
            latency_spike_probability: 0.10, // 10% chance of a latency spike
            max_latency_spike_ms: 500,
        }
    }
}

impl TokioFaultInjector {
    /// Intercepts an outbound network call (e.g. WebSocket or FIX API).
    /// May silently drop the packet, delay it massively, or let it pass through.
    pub async fn inject_fault(&self) -> Result<(), &'static str> {
        let mut rng = rand::thread_rng();

        // 1. Packet Loss Simulation
        if rng.gen::<f64>() < self.packet_loss_probability {
            error!("CHAOS MONKEY: Simulating 100% packet drop on WebSocket payload. Returning error.");
            return Err("Network Error: Packet dropped (Chaos Test)");
        }

        // 2. Latency Spike Simulation
        if rng.gen::<f64>() < self.latency_spike_probability {
            let delay_ms = rng.gen_range(50..self.max_latency_spike_ms);
            warn!("CHAOS MONKEY: Simulating network jitter. Delaying payload by {}ms.", delay_ms);
            sleep(Duration::from_millis(delay_ms)).await;
        }

        Ok(())
    }
}
