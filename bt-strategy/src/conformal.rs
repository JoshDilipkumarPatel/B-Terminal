use std::cmp::Ordering;

use crate::sentiment::SentimentClass;

/// Conformal Predictor for guaranteeing epistemic confidence bounds on AI outputs.
pub struct ConformalPredictor {
    pub non_conformity_scores: Vec<f64>,
    pub alpha: f64, // The maximum allowed error rate (e.g. 0.05 for a 95% confidence set)
    pub q_hat: Option<f64>,
}

impl ConformalPredictor {
    /// Creates a new ConformalPredictor with a target error rate `alpha` (e.g. 0.05 for 95% confidence).
    pub fn new(alpha: f64) -> Self {
        Self {
            non_conformity_scores: Vec::new(),
            alpha,
            q_hat: None,
        }
    }

    /// Calibrates the predictor using a historical dataset of predictions and their true labels.
    /// `calibration_data` is a slice of (prediction_probabilities, true_label).
    /// `prediction_probabilities` is a list of (Class, Probability) from the AI model.
    pub fn calibrate(&mut self, calibration_data: &[(Vec<(SentimentClass, f64)>, SentimentClass)]) {
        let mut scores = Vec::with_capacity(calibration_data.len());
        
        for (probs, true_label) in calibration_data {
            // Find the probability assigned to the true label
            let mut true_prob = 0.0;
            for (class, prob) in probs {
                if class == true_label {
                    true_prob = *prob;
                    break;
                }
            }
            
            // Non-conformity score is 1.0 - probability of the true class
            let s_i = 1.0 - true_prob;
            scores.push(s_i);
        }

        // Sort ascending
        scores.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        self.non_conformity_scores = scores;

        let n = self.non_conformity_scores.len() as f64;
        let index = ((n + 1.0) * (1.0 - self.alpha)).ceil() as usize;
        let array_idx = index.saturating_sub(1);
        
        if array_idx >= self.non_conformity_scores.len() {
            self.q_hat = Some(1.0); // If dataset is too small, be completely permissive
        } else {
            self.q_hat = Some(self.non_conformity_scores[array_idx]);
        }
    }

    /// Given a new model prediction (list of class probabilities), outputs a Prediction Set
    /// that is mathematically guaranteed to contain the true label with `1 - alpha` probability.
    pub fn predict_set(&self, probs: &[(SentimentClass, f64)]) -> Vec<SentimentClass> {
        let q_hat = self.q_hat.unwrap_or(1.0); // Fallback to 1.0 if not calibrated
        
        let mut prediction_set = Vec::new();
        
        for (class, prob) in probs {
            let s_i = 1.0 - prob;
            if s_i <= q_hat {
                prediction_set.push(class.clone());
            }
        }
        
        // If the set is empty (extreme edge case where q_hat is tiny and all probs are low),
        // we fallback to the highest probability class to prevent empty sets.
        if prediction_set.is_empty() && !probs.is_empty() {
            let mut max_prob = -1.0;
            let mut best_class = probs[0].0.clone();
            for (class, prob) in probs {
                if *prob > max_prob {
                    max_prob = *prob;
                    best_class = class.clone();
                }
            }
            prediction_set.push(best_class);
        }
        
        prediction_set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conformal_calibration_and_prediction() {
        let mut predictor = ConformalPredictor::new(0.05); // 95% confidence
        
        // Mock calibration set (n=19 so (20 * 0.95 = 19), index = 19 - 1 = 18)
        let mut cal_data = Vec::new();
        
        // Add 18 accurate predictions (high prob for the true class)
        for _ in 0..18 {
            cal_data.push((
                vec![
                    (SentimentClass::StrongBullish, 0.90),
                    (SentimentClass::Neutral, 0.05),
                    (SentimentClass::StrongBearish, 0.05),
                ],
                SentimentClass::StrongBullish,
            ));
        }
        
        // Add 1 outlier where the true class had a low probability
        cal_data.push((
            vec![
                (SentimentClass::StrongBullish, 0.80),
                (SentimentClass::Neutral, 0.10),
                (SentimentClass::StrongBearish, 0.10), // True label only got 0.10 prob -> s_i = 0.90
            ],
            SentimentClass::StrongBearish,
        ));
        
        predictor.calibrate(&cal_data);
        
        // Our alpha is 0.05, n=19. The 95% index is ceil(20 * 0.95) = 19.
        // The 19th element (index 18) in the sorted scores is the outlier (0.90).
        // So q_hat should be 0.90.
        assert_eq!(predictor.q_hat.unwrap(), 0.90);
        
        // Now test prediction
        let test_probs = vec![
            (SentimentClass::StrongBullish, 0.50), // s_i = 0.50 <= 0.90 (Included)
            (SentimentClass::Neutral, 0.30),       // s_i = 0.70 <= 0.90 (Included)
            (SentimentClass::StrongBearish, 0.20), // s_i = 0.80 <= 0.90 (Included)
        ];
        
        let set = predictor.predict_set(&test_probs);
        assert_eq!(set.len(), 3); // Prediction set includes all 3 because model is very uncertain
        
        // Test a highly confident prediction
        let test_confident = vec![
            (SentimentClass::StrongBullish, 0.95), // s_i = 0.05 <= 0.90 (Included)
            (SentimentClass::Neutral, 0.03),       // s_i = 0.97 > 0.90 (Excluded)
            (SentimentClass::StrongBearish, 0.02), // s_i = 0.98 > 0.90 (Excluded)
        ];
        let set2 = predictor.predict_set(&test_confident);
        assert_eq!(set2.len(), 1);
        assert_eq!(set2[0], SentimentClass::StrongBullish);
    }
}
