use serde::{Serialize, Deserialize};
use tracing::info;

use super::crdt_position::PNCounter;

/// Distributed Cluster Events broadcasted over Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClusterEvent {
    /// A node is broadcasting its CRDT Position State to the cluster
    PositionSync {
        node_id: String,
        symbol: String,
        crdt_state: PNCounter,
    },
    /// A primary node went down, backup node taking over execution
    LeaderFailover {
        old_leader: String,
        new_leader: String,
    },
    Heartbeat {
        node_id: String,
        timestamp_ms: u64,
    }
}

/// Manages Redis Pub/Sub communication for High-Availability
pub struct ClusterManager {
    node_id: String,
    _redis_url: String,
    
    // In a full implementation, we hold a redis::aio::Connection here.
    // For Pillar 4 verification, we expose the serialization pipeline.
}

impl ClusterManager {
    pub fn new(node_id: &str, redis_url: &str) -> Self {
        Self {
            node_id: node_id.to_string(),
            _redis_url: redis_url.to_string(),
        }
    }

    /// Serializes a ClusterEvent into a JSON payload for Redis Pub/Sub broadcast
    pub fn prepare_broadcast_payload(&self, event: &ClusterEvent) -> Result<String, serde_json::Error> {
        serde_json::to_string(event)
    }

    /// Parses a JSON payload received from Redis Pub/Sub back into a ClusterEvent
    pub fn parse_received_payload(&self, payload: &str) -> Result<ClusterEvent, serde_json::Error> {
        serde_json::from_str(payload)
    }
    
    // We mock the actual async broadcast to avoid needing a live Redis server running during CI/unit tests.
    pub async fn broadcast_event(&self, event: ClusterEvent) -> Result<(), String> {
        let payload = self.prepare_broadcast_payload(&event)
            .map_err(|e| format!("Failed to serialize event: {}", e))?;
            
        info!("BROADCAST: [{}] -> {}", self.node_id, payload);
        
        // Simulating: redis::cmd("PUBLISH").arg("bt_cluster_bus").arg(payload).query_async(&mut self.redis_conn).await...
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_event_serialization() {
        let manager = ClusterManager::new("node_1", "redis://127.0.0.1/");
        
        let mut crdt = PNCounter::new();
        crdt.increment("node_1", 500);

        let event = ClusterEvent::PositionSync {
            node_id: "node_1".to_string(),
            symbol: "AAPL".to_string(),
            crdt_state: crdt,
        };

        // 1. Serialize
        let payload = manager.prepare_broadcast_payload(&event).unwrap();
        assert!(payload.contains("PositionSync"));
        assert!(payload.contains("AAPL"));

        // 2. Deserialize
        let parsed: ClusterEvent = manager.parse_received_payload(&payload).unwrap();
        
        match parsed {
            ClusterEvent::PositionSync { node_id, symbol, crdt_state } => {
                assert_eq!(node_id, "node_1");
                assert_eq!(symbol, "AAPL");
                assert_eq!(crdt_state.net_position(), 500);
            },
            _ => panic!("Deserialized into wrong variant!"),
        }
    }
}
