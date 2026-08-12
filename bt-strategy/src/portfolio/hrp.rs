use nalgebra::{DMatrix, DVector};

/// Represents the Hierarchical Risk Parity (HRP) Portfolio Optimizer.
/// HRP uses machine learning (hierarchical clustering) to group correlated assets,
/// then recursively allocates risk across the clusters. This avoids the instability 
/// of matrix inversion found in standard Markowitz Mean-Variance Optimization.
pub struct HierarchicalRiskParity;

impl HierarchicalRiskParity {
    /// Computes the HRP portfolio weights given an asset covariance matrix.
    pub fn optimize(covariance: &DMatrix<f64>) -> Result<DVector<f64>, String> {
        let n = covariance.nrows();
        if n == 0 || n != covariance.ncols() {
            return Err("Covariance matrix must be square and non-empty".to_string());
        }

        // 1. Convert covariance to correlation matrix
        let correlation = Self::cov_to_corr(covariance);

        // 2. Compute distance matrix: D_i,j = sqrt(0.5 * (1 - rho_i,j))
        let distance = Self::corr_to_dist(&correlation);

        // 3. Hierarchical Clustering (Simplified single-linkage agglomerative clustering)
        // Returns an ordered list of indices (Quasi-Diagonalization)
        let sort_ix = Self::quasi_diagonalization(&distance);

        // 4. Recursive Bisection to allocate weights
        let weights = Self::recursive_bisection(covariance, &sort_ix);

        // Map sorted weights back to original indices
        let mut final_weights = DVector::zeros(n);
        for i in 0..n {
            final_weights[sort_ix[i]] = weights[i];
        }

        Ok(final_weights)
    }

    fn cov_to_corr(cov: &DMatrix<f64>) -> DMatrix<f64> {
        let n = cov.nrows();
        let mut corr = DMatrix::zeros(n, n);
        let std_devs: Vec<f64> = (0..n).map(|i| cov[(i, i)].sqrt()).collect();

        for i in 0..n {
            for j in 0..n {
                if std_devs[i] == 0.0 || std_devs[j] == 0.0 {
                    corr[(i, j)] = 0.0;
                } else {
                    corr[(i, j)] = cov[(i, j)] / (std_devs[i] * std_devs[j]);
                }
            }
        }
        corr
    }

    fn corr_to_dist(corr: &DMatrix<f64>) -> DMatrix<f64> {
        let n = corr.nrows();
        let mut dist = DMatrix::zeros(n, n);
        for i in 0..n {
            for j in 0..n {
                dist[(i, j)] = (0.5 * (1.0 - corr[(i, j)])).max(0.0).sqrt();
            }
        }
        dist
    }

    /// Simplified quasi-diagonalization: Sorts assets so that correlated assets are adjacent.
    /// In a full implementation, this traces a dendrogram tree.
    /// For this version, we'll use a greedy nearest-neighbor approach.
    fn quasi_diagonalization(dist: &DMatrix<f64>) -> Vec<usize> {
        let n = dist.nrows();
        if n == 0 { return vec![]; }
        
        let mut unvisited: Vec<usize> = (1..n).collect();
        let mut sorted = vec![0]; // Start with asset 0

        while !unvisited.is_empty() {
            let last = *sorted.last().unwrap();
            
            // Find nearest neighbor in unvisited
            let mut nearest_idx = 0;
            let mut min_dist = f64::MAX;
            
            for (i, &asset) in unvisited.iter().enumerate() {
                let d = dist[(last, asset)];
                if d < min_dist {
                    min_dist = d;
                    nearest_idx = i;
                }
            }
            
            sorted.push(unvisited.remove(nearest_idx));
        }

        sorted
    }

    fn recursive_bisection(cov: &DMatrix<f64>, sort_ix: &[usize]) -> Vec<f64> {
        let n = sort_ix.len();
        let mut w = vec![1.0; n];
        
        // Items to process: (start_index, end_index) inclusive
        let mut clusters = vec![(0, n - 1)];

        while let Some((start, end)) = clusters.pop() {
            if start >= end { continue; } // single item cluster

            let mid = start + (end - start) / 2;
            
            // Cluster 1: start..=mid
            // Cluster 2: mid+1..=end
            
            let v1 = Self::get_cluster_variance(cov, sort_ix, start, mid);
            let v2 = Self::get_cluster_variance(cov, sort_ix, mid + 1, end);
            
            // Alpha is the weight allocated to Cluster 1
            // Inversely proportional to cluster variance
            let alpha = if v1 + v2 == 0.0 { 0.5 } else { 1.0 - v1 / (v1 + v2) };
            
            // Update weights
            for i in start..=mid { w[i] *= alpha; }
            for i in (mid + 1)..=end { w[i] *= 1.0 - alpha; }
            
            // Recurse
            if start < mid { clusters.push((start, mid)); }
            if mid + 1 < end { clusters.push((mid + 1, end)); }
        }

        w
    }

    fn get_cluster_variance(cov: &DMatrix<f64>, sort_ix: &[usize], start: usize, end: usize) -> f64 {
        // Inverse-variance weights within the cluster
        let mut inv_var = Vec::new();
        let mut sum_inv_var = 0.0;
        
        for i in start..=end {
            let asset_idx = sort_ix[i];
            let v = cov[(asset_idx, asset_idx)];
            let iv = if v > 0.0 { 1.0 / v } else { 0.0 };
            inv_var.push(iv);
            sum_inv_var += iv;
        }

        if sum_inv_var == 0.0 { return 0.0; }

        let w: Vec<f64> = inv_var.iter().map(|&iv| iv / sum_inv_var).collect();
        
        let mut cv = 0.0;
        for (i, w_i) in w.iter().enumerate() {
            for (j, w_j) in w.iter().enumerate() {
                let asset_i = sort_ix[start + i];
                let asset_j = sort_ix[start + j];
                cv += w_i * w_j * cov[(asset_i, asset_j)];
            }
        }
        
        cv
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hrp_optimization() {
        // Create a 3x3 mock covariance matrix
        // Assets 0 and 1 are highly correlated (and volatile)
        // Asset 2 is uncorrelated and low volatility
        let cov = DMatrix::from_row_slice(3, 3, &[
            0.10, 0.08, 0.00,
            0.08, 0.10, 0.00,
            0.00, 0.00, 0.02
        ]);

        let weights = HierarchicalRiskParity::optimize(&cov).unwrap();
        
        assert_eq!(weights.len(), 3);
        
        // Sum of weights should be 1.0
        let sum: f64 = weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6);
        
        // Asset 2 should have the highest weight due to low variance and no correlation
        assert!(weights[2] > weights[0]);
        assert!(weights[2] > weights[1]);
    }
}
