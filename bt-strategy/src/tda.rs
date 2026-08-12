use std::cmp::Ordering;

/// Union-Find (Disjoint Set) data structure for efficiently tracking connected components.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
    pub components: usize,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
            rank: vec![0; size],
            components: size,
        }
    }

    fn find(&mut self, i: usize) -> usize {
        if self.parent[i] == i {
            i
        } else {
            let root = self.find(self.parent[i]);
            self.parent[i] = root; // Path compression
            root
        }
    }

    fn union(&mut self, i: usize, j: usize) {
        let root_i = self.find(i);
        let root_j = self.find(j);

        if root_i != root_j {
            match self.rank[root_i].cmp(&self.rank[root_j]) {
                Ordering::Less => self.parent[root_i] = root_j,
                Ordering::Greater => self.parent[root_j] = root_i,
                Ordering::Equal => {
                    self.parent[root_j] = root_i;
                    self.rank[root_i] += 1;
                }
            }
            self.components -= 1;
        }
    }
}

pub struct TopologicalCrashPredictor {
    /// Embedding dimension (m)
    pub dimension: usize,
    /// Time delay (tau)
    pub delay: usize,
    /// Filtration threshold (epsilon) for VR complex
    pub threshold: f64,
}

impl TopologicalCrashPredictor {
    pub fn new(dimension: usize, delay: usize, threshold: f64) -> Self {
        Self {
            dimension,
            delay,
            threshold,
        }
    }

    /// Converts a 1D time-series into a high-dimensional point cloud using Takens' Embedding Theorem.
    fn embed(&self, data: &[f64]) -> Vec<Vec<f64>> {
        let n = data.len();
        if n < (self.dimension - 1) * self.delay + 1 {
            return vec![]; // Not enough data
        }

        let num_points = n - (self.dimension - 1) * self.delay;
        let mut point_cloud = Vec::with_capacity(num_points);

        for i in 0..num_points {
            let mut point = Vec::with_capacity(self.dimension);
            for d in 0..self.dimension {
                point.push(data[i + d * self.delay]);
            }
            point_cloud.push(point);
        }

        point_cloud
    }

    /// Computes the Betti-0 number (number of connected components) of the Vietoris-Rips complex at the given epsilon threshold.
    fn calculate_betti_0(point_cloud: &[Vec<f64>], threshold: f64) -> usize {
        let num_points = point_cloud.len();
        if num_points == 0 {
            return 0;
        }

        let mut uf = UnionFind::new(num_points);

        for i in 0..num_points {
            for j in (i + 1)..num_points {
                let dist_sq: f64 = point_cloud[i].iter().zip(point_cloud[j].iter())
                    .map(|(a, b)| (a - b).powi(2))
                    .sum();
                
                if dist_sq.sqrt() <= threshold {
                    uf.union(i, j);
                }
            }
        }

        uf.components
    }

    /// Analyzes the recent price history and returns a crash probability (0.0 to 1.0).
    /// 
    /// In a healthy trending or range-bound market, the phase-space embedding remains 
    /// relatively tight (a single manifold), resulting in a low Betti-0 number.
    /// When the market undergoes a liquidity vacuum or regime shift, the point cloud 
    /// "shatters" into multiple disconnected components, causing Betti-0 to spike.
    pub fn calculate_crash_probability(&self, recent_prices: &[f64]) -> f64 {
        let point_cloud = self.embed(recent_prices);
        if point_cloud.is_empty() {
            return 0.0;
        }

        let max_possible_components = point_cloud.len() as f64;
        let betti_0 = Self::calculate_betti_0(&point_cloud, self.threshold) as f64;

        // Normalize Betti-0 to a probability score.
        // A single component means healthy (0.0 prob).
        // Total shattering means crash (1.0 prob).
        let raw_prob = (betti_0 - 1.0) / (max_possible_components - 1.0).max(1.0);
        
        // We apply a sigmoid curve to make the signal more definitive
        let steepness = 10.0;
        let midpoint = 0.3; // If 30% of the points shatter, we are in danger
        
        let sigmoid_prob = 1.0 / (1.0 + (-steepness * (raw_prob - midpoint)).exp());
        
        sigmoid_prob.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tda_point_cloud_embedding() {
        let predictor = TopologicalCrashPredictor::new(3, 1, 1.0);
        let prices = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        
        let pc = predictor.embed(&prices);
        
        assert_eq!(pc.len(), 3);
        assert_eq!(pc[0], vec![1.0, 2.0, 3.0]);
        assert_eq!(pc[1], vec![2.0, 3.0, 4.0]);
        assert_eq!(pc[2], vec![3.0, 4.0, 5.0]);
    }

    #[test]
    fn test_tda_betti_0_calculation() {
        // Points that are close together
        let point_cloud = vec![
            vec![0.0, 0.0],
            vec![0.5, 0.5],
            vec![1.0, 1.0],
            // Far away cluster
            vec![10.0, 10.0],
            vec![10.5, 10.5],
        ];

        // With threshold 1.0, points (0,0)->(0.5,0.5)->(1,1) connect.
        // And (10,10)->(10.5,10.5) connect.
        // Total components = 2.
        let betti_0 = TopologicalCrashPredictor::calculate_betti_0(&point_cloud, 1.0);
        assert_eq!(betti_0, 2);

        // With threshold 100.0, everything connects.
        let betti_0 = TopologicalCrashPredictor::calculate_betti_0(&point_cloud, 100.0);
        assert_eq!(betti_0, 1);
    }

    #[test]
    fn test_crash_probability_spike() {
        let predictor = TopologicalCrashPredictor::new(2, 1, 0.5);
        
        // Healthy market, smooth price action
        let mut healthy_prices = vec![];
        for i in 0..20 {
            healthy_prices.push(i as f64 * 0.1);
        }
        
        let healthy_prob = predictor.calculate_crash_probability(&healthy_prices);
        assert!(healthy_prob < 0.2, "Healthy market probability too high: {}", healthy_prob);

        // Flash crash: prices suddenly drop violently and scatter
        let mut crash_prices = healthy_prices.clone();
        for i in 1..=20 {
            // Push values that are at least 5.0 apart, so they never connect
            // given threshold = 0.5
            crash_prices.push((i * 5) as f64);
        }
        
        let crash_prob = predictor.calculate_crash_probability(&crash_prices);
        assert!(crash_prob > 0.8, "Crash market probability too low: {}", crash_prob);
    }
}
