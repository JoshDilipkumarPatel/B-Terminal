use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentimentResult {
    pub score: f64,
    pub magnitude: f64,
    pub keyword_hits: usize,
    pub classification: SentimentClass,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum SentimentClass {
    StrongBullish,
    Bullish,
    Neutral,
    Bearish,
    StrongBearish,
}

pub struct SentimentScorer;

impl SentimentScorer {
    pub fn score(text: &str) -> SentimentResult {
        let text = text.to_lowercase();
        let tokens: Vec<&str> = text.split_whitespace().collect();
        
        let mut score_sum = 0.0;
        let mut hits = 0;
        let mut magnitude_sum = 0.0;
        
        let dict = Self::dictionary();
        
        let mut i = 0;
        while i < tokens.len() {
            let mut matched = false;
            if i + 1 < tokens.len() {
                let bigram = format!("{} {}", tokens[i], tokens[i+1]);
                if let Some(&weight) = dict.iter().find(|(k, _)| *k == bigram).map(|(_, v)| v) {
                    score_sum += weight;
                    magnitude_sum += weight.abs();
                    hits += 1;
                    i += 2;
                    matched = true;
                }
            }
            if !matched {
                if let Some(&weight) = dict.iter().find(|(k, _)| *k == tokens[i]).map(|(_, v)| v) {
                    score_sum += weight;
                    magnitude_sum += weight.abs();
                    hits += 1;
                }
                i += 1;
            }
        }
        
        let normalized_score = score_sum.clamp(-1.0, 1.0);
        
        let classification = if normalized_score > 0.5 {
            SentimentClass::StrongBullish
        } else if normalized_score > 0.2 {
            SentimentClass::Bullish
        } else if normalized_score < -0.5 {
            SentimentClass::StrongBearish
        } else if normalized_score < -0.2 {
            SentimentClass::Bearish
        } else {
            SentimentClass::Neutral
        };
        
        SentimentResult {
            score: normalized_score,
            magnitude: magnitude_sum,
            keyword_hits: hits,
            classification,
        }
    }
    
    fn dictionary() -> &'static [(&'static str, f64)] {
        &[
            ("growth", 0.8),
            ("profit", 0.9),
            ("upgrade", 0.8),
            ("bullish", 1.0),
            ("surge", 0.7),
            ("fii inflow", 0.9),
            ("rupee strengthens", 0.6),
            ("expansion", 0.7),
            ("outperform", 0.8),
            ("dividend", 0.6),
            
            ("loss", -0.9),
            ("downgrade", -0.8),
            ("bearish", -1.0),
            ("crash", -1.0),
            ("plunge", -0.8),
            ("fii outflow", -0.9),
            ("rupee weakens", -0.6),
            ("inflation", -0.5),
            ("underperform", -0.8),
            ("default", -1.0),
            
            ("rbi", 0.0),
            ("repo rate", -0.3),
            ("nifty", 0.0),
            ("sensex", 0.0),
            ("guidance", 0.3),
            ("stable", 0.2),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sentiment_scoring() {
        let res = SentimentScorer::score("fii inflow and profit growth makes me bullish on nifty");
        assert!(res.score > 0.5);
        assert_eq!(res.classification, SentimentClass::StrongBullish);
        
        let res = SentimentScorer::score("heavy fii outflow and market crash leads to loss");
        assert!(res.score < -0.5);
        assert_eq!(res.classification, SentimentClass::StrongBearish);
        
        let res = SentimentScorer::score("rbi updates repo rate for sensex");
        assert!(res.score < 0.0);
        assert_eq!(res.classification, SentimentClass::Bearish);
    }
}
