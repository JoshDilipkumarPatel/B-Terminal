use serde::{Deserialize, Serialize};


/// Represents an N-dimensional historical trading feature vector (e.g. price returns, volume momentum, volatility)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatternRecord {
    pub id: String,
    pub symbol: String,
    pub timestamp: String,
    pub regime_label: String,
    pub raw_features: Vec<f64>,
    pub quantized_code: Vec<i8>,
}

#[derive(Debug, Clone)]
pub struct PatternMatchResult {
    pub id: String,
    pub symbol: String,
    pub timestamp: String,
    pub regime_label: String,
    pub distance: u32,
    pub similarity_pct: f64,
}

/// TurboQuantIndex implements data-oblivious vector quantization for real-time market pattern recognition.
/// Instead of heavy floating-point Euclidean calculations, it maps vectors via orthogonal projection
/// and scalar quantization to integer bit representations, achieving ~8x compression and ultra-low latency searching.
pub struct TurboQuantIndex {
    dimensions: usize,
    rotation_matrix: Vec<Vec<f64>>,
    catalog: Vec<PatternRecord>,
}

impl TurboQuantIndex {
    pub fn new(dimensions: usize, seed: u64) -> Self {
        let rotation_matrix = Self::generate_pseudo_orthogonal_matrix(dimensions, seed);
        Self {
            dimensions,
            rotation_matrix,
            catalog: Vec::new(),
        }
    }

    /// Generates a deterministic pseudo-orthogonal transformation matrix for data-oblivious projection
    fn generate_pseudo_orthogonal_matrix(dim: usize, seed: u64) -> Vec<Vec<f64>> {
        let mut matrix = vec![vec![0.0; dim]; dim];
        // Simple Hadamard / Trigo rotation generation for deterministic data-oblivious uniform spread
        let mut current_seed = seed;
        for i in 0..dim {
            for j in 0..dim {
                current_seed = current_seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let val = ((current_seed >> 33) as i32) as f64 / (i32::MAX as f64);
                matrix[i][j] = if val > 0.0 { 1.0 / (dim as f64).sqrt() } else { -1.0 / (dim as f64).sqrt() };
            }
        }
        matrix
    }

    /// Applies rotation and scalar quantization to compress f64 float vectors to tiny i8 quantized vectors
    pub fn quantize(&self, features: &[f64]) -> Vec<i8> {
        let mut rotated = vec![0.0; self.dimensions];
        let len = features.len().min(self.dimensions);
        for i in 0..self.dimensions {
            let mut sum = 0.0;
            for j in 0..len {
                sum += self.rotation_matrix[i][j] * features[j];
            }
            rotated[i] = sum;
        }

        // Scalar int8 quantization mapping across [-3.0, +3.0] normalized deviation domain
        rotated.into_iter().map(|val| {
            let clamped = val.clamp(-3.0, 3.0);
            ((clamped / 3.0) * 127.0).round() as i8
        }).collect()
    }

    /// Inserts a new market pattern into the TurboQuant index
    pub fn add_pattern(&mut self, id: &str, symbol: &str, timestamp: &str, regime: &str, features: Vec<f64>) -> Vec<i8> {
        let quantized_code = self.quantize(&features);
        let record = PatternRecord {
            id: id.to_string(),
            symbol: symbol.to_string(),
            timestamp: timestamp.to_string(),
            regime_label: regime.to_string(),
            raw_features: features,
            quantized_code: quantized_code.clone(),
        };
        self.catalog.push(record);
        quantized_code
    }

    /// Computes high-speed quantized L1/L2 approximation distance directly over integer codes
    fn quantized_distance(code_a: &[i8], code_b: &[i8]) -> u32 {
        let mut dist_sq = 0u32;
        for (&a, &b) in code_a.iter().zip(code_b.iter()) {
            let diff = (a as i32) - (b as i32);
            dist_sq += (diff * diff) as u32;
        }
        dist_sq
    }

    /// Searches the index to locate the Top-K most similar historical market episodes to a live query vector
    pub fn find_similar(&self, query_features: &[f64], top_k: usize) -> Vec<PatternMatchResult> {
        let query_code = self.quantize(query_features);
        let mut matches: Vec<PatternMatchResult> = self.catalog.iter().map(|record| {
            let dist = Self::quantized_distance(&query_code, &record.quantized_code);
            // Normalize distance to a similarity percentage (0 distance -> 100%, higher -> lower)
            let max_possible_dist = (256 * 256 * self.dimensions) as f64;
            let sim_pct = ((1.0 - ((dist as f64) / max_possible_dist).sqrt()) * 100.0).max(0.0);
            PatternMatchResult {
                id: record.id.clone(),
                symbol: record.symbol.clone(),
                timestamp: record.timestamp.clone(),
                regime_label: record.regime_label.clone(),
                distance: dist,
                similarity_pct: sim_pct,
            }
        }).collect();

        matches.sort_by_key(|m| m.distance);
        matches.truncate(top_k);
        matches
    }

    pub fn catalog_size(&self) -> usize {
        self.catalog.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turbo_quant_compression_and_similarity() {
        let mut tq = TurboQuantIndex::new(8, 20260803);
        
        // Bullish momentum pattern (uptrend features)
        let bull_vec = vec![0.5, 0.8, 1.2, 1.5, 1.8, 2.1, 2.5, 3.0];
        let bull_code = tq.add_pattern("ep_101", "NSE:RELIANCE", "2026-07-15", "Bullish Breakout", bull_vec.clone());
        assert_eq!(bull_code.len(), 8); // Quantized from 8 * f64 (64 bytes) to 8 * i8 (8 bytes) -> 8x compression!

        // Bearish breakdown pattern
        let bear_vec = vec![-0.4, -0.9, -1.3, -1.8, -2.2, -2.6, -2.9, -3.2];
        tq.add_pattern("ep_102", "NSE:RELIANCE", "2026-07-20", "Bearish Dump", bear_vec);

        // Rangebound chop pattern
        let chop_vec = vec![0.1, -0.2, 0.15, -0.1, 0.05, -0.05, 0.12, -0.08];
        tq.add_pattern("ep_103", "NSE:RELIANCE", "2026-07-25", "Rangebound Chop", chop_vec);

        // Query today's real-time market: looks almost identical to the Bullish pattern!
        let today_live = vec![0.48, 0.79, 1.15, 1.48, 1.75, 2.05, 2.40, 2.95];
        let results = tq.find_similar(&today_live, 2);
        
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "ep_101"); // Best match MUST be the bullish pattern
        assert_eq!(results[0].regime_label, "Bullish Breakout");
        assert!(results[0].similarity_pct > 95.0, "Similarity should exceed 95% for virtually identical patterns");
    }
}
