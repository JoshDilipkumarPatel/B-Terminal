use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};
use reqwest::Client;
use tracing::{info, warn, error};
use crate::kill_switch::GlobalKillSwitch;
use crate::events::KillReason;
use sha2::{Sha256, Digest};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub timestamp: String,
    pub source: String, // e.g., "SYNDICATE", "RISK", "HUMAN"
    pub action: String,
    pub payload: String,
}

pub struct VeritasChain {
    pub current_head_hash: String,
    pub event_log: Vec<AuditEvent>,
}

impl Default for VeritasChain {
    fn default() -> Self {
        Self::new()
    }
}

impl VeritasChain {
    pub fn new() -> Self {
        // The genesis hash
        Self {
            current_head_hash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            event_log: Vec::new(),
        }
    }

    /// Logs an event cryptographically by chaining it with the previous hash
    pub fn log_event(&mut self, source: &str, action: &str, payload: &str) {
        let event = AuditEvent {
            timestamp: chrono::Utc::now().to_rfc3339(),
            source: source.to_string(),
            action: action.to_string(),
            payload: payload.to_string(),
        };

        let event_json = serde_json::to_string(&event).unwrap_or_default();
        
        let mut hasher = Sha256::new();
        hasher.update(self.current_head_hash.as_bytes());
        hasher.update(event_json.as_bytes());
        
        self.current_head_hash = format!("{:x}", hasher.finalize());
        self.event_log.push(event);
    }

    pub fn head_hash(&self) -> String {
        self.current_head_hash.clone()
    }
}

pub struct AuditAnchor {
    checkpoint_url: String,
    kill_switch: Arc<GlobalKillSwitch>,
    consecutive_failures: Arc<RwLock<u32>>,
    chain: Arc<RwLock<VeritasChain>>,
}

impl AuditAnchor {
    pub fn new(checkpoint_url: String, kill_switch: Arc<GlobalKillSwitch>) -> Self {
        Self {
            checkpoint_url,
            kill_switch,
            consecutive_failures: Arc::new(RwLock::new(0)),
            chain: Arc::new(RwLock::new(VeritasChain::new())),
        }
    }

    /// Appends a new event to the VeritasChain.
    pub async fn log_event(&self, source: &str, action: &str, payload: &str) {
        let mut chain = self.chain.write().await;
        chain.log_event(source, action, payload);
    }

    /// Spawns a background task that periodically POSTs local audit hashes to the SIEM WORM server.
    /// FAIL-CLOSED: If the server is unreachable for 3 consecutive intervals, the global kill switch is triggered.
    pub fn spawn_background_task(self: Arc<Self>) {
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(300)); // 5 minutes
            let client = Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default();

            loop {
                ticker.tick().await;

                // Grab the true cryptographic chain head
                let local_hash = self.chain.read().await.head_hash();

                let payload = serde_json::json!({
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "hash_root": local_hash
                });

                let result = client.post(&self.checkpoint_url)
                    .json(&payload)
                    .send()
                    .await;

                match result {
                    Ok(resp) if resp.status().is_success() => {
                        *self.consecutive_failures.write().await = 0;
                        info!("Successfully anchored VeritasChain head ({}) to SIEM WORM root.", local_hash);
                    }
                    Ok(resp) => {
                        warn!("WORM anchor rejected payload with status: {}", resp.status());
                        Self::handle_failure(&self).await;
                    }
                    Err(e) => {
                        warn!("Failed to reach WORM anchor URL ({}): {}", self.checkpoint_url, e);
                        Self::handle_failure(&self).await;
                    }
                }
            }
        });
    }

    async fn handle_failure(anchor: &Arc<Self>) {
        let mut fails = anchor.consecutive_failures.write().await;
        *fails += 1;
        if *fails >= 3 {
            error!("FAIL-CLOSED: WORM Anchor unreachable for 3 consecutive attempts. Triggering Global Kill Switch to halt trading.");
            let _ = anchor.kill_switch.activate(KillReason::SystemError).await;
        }
    }
}
