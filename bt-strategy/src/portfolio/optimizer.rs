use clarabel::algebra::CscMatrix;
use clarabel::solver::{
    DefaultSettings, DefaultSolver, IPSolver
};
use nalgebra::DVector;

/// Convex Optimization Engine using Clarabel.
/// Calculates the optimal rebalancing trade sizes to move the portfolio
/// from current inventory to target weights, while minimizing transaction costs 
/// (slippage / market impact) and enforcing risk constraints.
pub struct PortfolioOptimizer;

impl PortfolioOptimizer {
    /// Solves a simplified Quadratic Program (QP) to find optimal trade sizes.
    /// Objective: Minimize Tracking Error + Transaction Costs
    /// Min 0.5 * x^T P x + q^T x
    /// Subject to: x_i >= -inventory_i (Cannot short more than we own, assuming long-only for now)
    ///
    /// # Arguments
    /// * `target_weights` - The desired portfolio weights (from BL or HRP) (N x 1)
    /// * `current_inventory` - The current portfolio weights (N x 1)
    /// * `slippage_costs` - VPIN-adjusted market impact penalties (N x 1)
    pub fn optimize_rebalance(
        target_weights: &DVector<f64>,
        current_inventory: &DVector<f64>,
        slippage_costs: &DVector<f64>,
    ) -> Result<DVector<f64>, String> {
        let n = target_weights.len();
        if current_inventory.len() != n || slippage_costs.len() != n {
            return Err("Vector dimension mismatch".to_string());
        }

        // We want to find trades `u` such that new_weights = current_inventory + u
        // Objective: Minimize || (current_inventory + u) - target_weights ||^2 + lambda * slippage^T * |u|
        // To formulate as a standard QP for Clarabel without absolute values, we would need auxiliary variables.
        // For this high-speed simplified version, we'll solve:
        // Min 0.5 * u^T I u + (current - target)^T u
        // This is equivalent to minimizing ||current + u - target||^2.
        // We'll add the slippage as a linear penalty on `u` (this assumes directional slippage, which is a simplification).
        
        // P = Identity matrix (CSC format)
        let mut p_colptr = vec![0; n + 1];
        let mut p_rowval = vec![0; n];
        let mut p_nzval = vec![0.0; n];
        
        for i in 0..n {
            p_colptr[i] = i;
            p_rowval[i] = i;
            p_nzval[i] = 1.0; // Weight of tracking error
        }
        p_colptr[n] = n;
        
        let p = CscMatrix::new(n, n, p_colptr, p_rowval, p_nzval);
        
        // q = (current_inventory - target_weights) + slippage_costs
        let mut q = vec![0.0; n];
        for i in 0..n {
            q[i] = (current_inventory[i] - target_weights[i]) + (slippage_costs[i] * 0.1); // Scaled slippage penalty
        }

        // Constraints: current_inventory + u >= 0  =>  u >= -current_inventory  =>  u + current_inventory >= 0
        // Matrix A = Identity, b = -current_inventory
        // But Clarabel expects A * x + s = b, s \in K
        // So A = -Identity, b = current_inventory
        // -u + s = current_inventory => u = current_inventory - s. If s >= 0, u <= current_inventory.
        // Wait, standard form is A * x + s = b, s in K.
        // We want u >= -current_inventory => -u <= current_inventory => -u + s = current_inventory, s in R+.
        
        let mut a_colptr = vec![0; n + 1];
        let mut a_rowval = vec![0; 2 * n];
        let mut a_nzval = vec![0.0; 2 * n];
        for i in 0..n {
            a_colptr[i] = 2 * i;
            
            // Constraint 1: -u_i + s_1_i = current_i
            a_rowval[2 * i] = i;
            a_nzval[2 * i] = -1.0;
            
            // Constraint 2: sum(u) = 0  => 1.0 * u_i
            a_rowval[2 * i + 1] = n;
            a_nzval[2 * i + 1] = 1.0;
        }
        a_colptr[n] = 2 * n;
        
        let a = CscMatrix::new(n + 1, n, a_colptr, a_rowval, a_nzval);
        
        let mut b = vec![0.0; n + 1];
        for i in 0..n {
            b[i] = current_inventory[i];
        }
        b[n] = 0.0;

        let cones = [
            clarabel::solver::SupportedConeT::NonnegativeConeT(n),
            clarabel::solver::SupportedConeT::ZeroConeT(1)
        ];
        
        let settings = DefaultSettings {
            verbose: false,
            ..DefaultSettings::default()
        };

        let mut solver = DefaultSolver::new(&p, &q, &a, &b, &cones, settings).unwrap();
        solver.solve();

        if solver.solution.status == clarabel::solver::SolverStatus::Solved {
            let u = &solver.solution.x;
            let mut trades = DVector::zeros(n);
            for i in 0..n {
                trades[i] = u[i];
            }
            Ok(trades)
        } else {
            Err(format!("Optimization failed: {:?}", solver.solution.status))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convex_rebalance() {
        let _n = 3;
        // Target: 40%, 40%, 20%
        let target = DVector::from_vec(vec![0.4, 0.4, 0.2]);
        // Current: 50%, 30%, 20%
        let current = DVector::from_vec(vec![0.5, 0.3, 0.2]);
        // Slippage: High on asset 0, low on others
        let slippage = DVector::from_vec(vec![0.05, 0.01, 0.01]);

        let trades = PortfolioOptimizer::optimize_rebalance(&target, &current, &slippage).unwrap();
        
        assert_eq!(trades.len(), 3);
        
        // Asset 0 should be sold (negative trade) to reach target
        // Asset 1 should be bought (positive trade)
        assert!(trades[0] < 0.0);
        assert!(trades[1] > 0.0);
        
        let new_weights = current.clone() + trades;
        
        // The new weights should be closer to target
        let dist_before = (current.clone() - target.clone()).norm();
        let dist_after = (new_weights - target).norm();
        
        assert!(dist_after < dist_before);
    }
}
