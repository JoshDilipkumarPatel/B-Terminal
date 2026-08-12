use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Positive-Negative (PN) Counter CRDT for Distributed Position Tracking.
/// 
/// In a multi-node high-availability setup, if Node A buys 100 shares and Node B sells 50,
/// relying on standard database updates causes race conditions and missed updates.
/// A PN-Counter guarantees eventual consistency. It tracks additions and subtractions 
/// independently for every node in the cluster.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PNCounter {
    /// Tracks total buys (increments) by node ID
    p_counters: HashMap<String, i64>,
    /// Tracks total sells (decrements) by node ID
    n_counters: HashMap<String, i64>,
}

impl PNCounter {
    pub fn new() -> Self {
        Self {
            p_counters: HashMap::new(),
            n_counters: HashMap::new(),
        }
    }

    /// Increment the position (e.g. bought shares)
    pub fn increment(&mut self, node_id: &str, amount: i64) {
        if amount < 0 { return; }
        let count = self.p_counters.entry(node_id.to_string()).or_insert(0);
        *count += amount;
    }

    /// Decrement the position (e.g. sold shares)
    pub fn decrement(&mut self, node_id: &str, amount: i64) {
        if amount < 0 { return; }
        let count = self.n_counters.entry(node_id.to_string()).or_insert(0);
        *count += amount;
    }

    /// Returns the exact net position guaranteed across the cluster
    pub fn net_position(&self) -> i64 {
        let total_p: i64 = self.p_counters.values().sum();
        let total_n: i64 = self.n_counters.values().sum();
        total_p - total_n
    }

    /// Merge the state of another node's replica into this one.
    /// The merge operation is Commutative, Associative, and Idempotent (a true CRDT).
    /// It takes the pointwise maximum for every node's P and N counter.
    pub fn merge(&mut self, other: &Self) {
        for (node, val) in &other.p_counters {
            let entry = self.p_counters.entry(node.clone()).or_insert(0);
            if *val > *entry {
                *entry = *val;
            }
        }
        
        for (node, val) in &other.n_counters {
            let entry = self.n_counters.entry(node.clone()).or_insert(0);
            if *val > *entry {
                *entry = *val;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pn_counter_convergence() {
        // Node 1 Replica
        let mut replica1 = PNCounter::new();
        replica1.increment("node_1", 100); // Node 1 buys 100

        // Node 2 Replica (completely independent, starts blank or outdated)
        let mut replica2 = PNCounter::new();
        replica2.increment("node_2", 50); // Node 2 buys 50
        replica2.decrement("node_2", 30); // Node 2 sells 30

        // Network partitions heal, they broadcast their state to each other
        replica1.merge(&replica2);
        replica2.merge(&replica1);

        // Net position should be 100 + 50 - 30 = 120
        assert_eq!(replica1.net_position(), 120);
        
        // Both replicas must converge to the exact same state
        assert_eq!(replica1.net_position(), replica2.net_position());
    }

    #[test]
    fn test_idempotent_merges() {
        let mut replica1 = PNCounter::new();
        replica1.increment("node_1", 100);

        let mut replica2 = PNCounter::new();
        replica2.merge(&replica1);
        
        // Merging multiple times should have no additional effect
        replica2.merge(&replica1);
        replica2.merge(&replica1);
        
        assert_eq!(replica2.net_position(), 100);
    }
}
