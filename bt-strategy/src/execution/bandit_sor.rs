use std::collections::HashMap;
use rand_distr::{Beta, Distribution};
use rand::thread_rng;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RoutingAction {
    Aggressive, // Cross the spread, take liquidity
    Passive,    // Post limit order, wait for fill
    DarkPool,   // Route to non-displayed venue to avoid signaling
}

/// Contextual Multi-Armed Bandit for Smart Order Routing (SOR).
/// Uses Thompson Sampling to continuously explore and exploit the optimal routing venue
/// based on the real-time probability of execution success vs slippage penalties.
pub struct BanditSor {
    /// Tracks (alpha, beta) parameters for the Beta distribution of each action.
    /// Alpha = prior successes, Beta = prior failures.
    action_priors: HashMap<RoutingAction, (f64, f64)>,
}

impl Default for BanditSor {
    fn default() -> Self {
        Self::new()
    }
}

impl BanditSor {
    pub fn new() -> Self {
        let mut priors = HashMap::new();
        // Initialize with a uniform prior (Beta(1, 1))
        priors.insert(RoutingAction::Aggressive, (1.0, 1.0));
        priors.insert(RoutingAction::Passive, (1.0, 1.0));
        priors.insert(RoutingAction::DarkPool, (1.0, 1.0));

        Self {
            action_priors: priors,
        }
    }

    /// Select the next optimal routing action using Thompson Sampling.
    /// Draws a random sample from each action's Beta distribution and picks the highest.
    pub fn select_route(&self) -> RoutingAction {
        let mut rng = thread_rng();
        let mut best_action = RoutingAction::Aggressive;
        let mut max_sample = -1.0;

        for (action, &(alpha, beta_param)) in &self.action_priors {
            let beta_dist = Beta::new(alpha, beta_param).unwrap();
            let sample = beta_dist.sample(&mut rng);
            
            if sample > max_sample {
                max_sample = sample;
                best_action = *action;
            }
        }

        best_action
    }

    /// Update the prior distribution based on the execution outcome.
    ///
    /// # Arguments
    /// * `action` - The route that was taken
    /// * `success` - True if filled without excess slippage, False if unfilled or slipped heavily
    pub fn update_prior(&mut self, action: RoutingAction, success: bool) {
        if let Some((alpha, beta_param)) = self.action_priors.get_mut(&action) {
            if success {
                *alpha += 1.0; // Increment success
            } else {
                *beta_param += 1.0; // Increment failure
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thompson_sampling_convergence() {
        let mut sor = BanditSor::new();

        // Simulate a regime where DarkPool has a 90% success rate, 
        // Aggressive has 50%, and Passive has 10%.
        // We manually update priors to simulate past learning.
        
        // Dark Pool: 90 successes, 10 failures
        sor.action_priors.insert(RoutingAction::DarkPool, (90.0, 10.0));
        
        // Aggressive: 50 successes, 50 failures
        sor.action_priors.insert(RoutingAction::Aggressive, (50.0, 50.0));
        
        // Passive: 10 successes, 90 failures
        sor.action_priors.insert(RoutingAction::Passive, (10.0, 90.0));

        // Over 1000 selections, Dark Pool should be selected the vast majority of the time.
        let mut dark_pool_selections = 0;
        for _ in 0..1000 {
            let action = sor.select_route();
            if action == RoutingAction::DarkPool {
                dark_pool_selections += 1;
            }
        }

        // Extremely high probability (>99%) that Dark Pool is chosen most often
        assert!(dark_pool_selections > 800, "Thompson sampling failed to exploit optimal route");
    }
}
