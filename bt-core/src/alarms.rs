use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlarmTier {
    /// Level 0: Normal routine fluctuations (<5% drawdown). Complete acoustic silence.
    Level0SilentNormal,
    /// Level 1: Caution Warning (5% - 10% drawdown). Soft visual banner without acoustic noise.
    Level1CautionWarning,
    /// Level 2: Emergency Liquidation (>12% wipeout or <15% margin buffer). Acoustic alarms & emergency flatten prompt!
    Level2EmergencyLiquidation,
}

impl fmt::Display for AlarmTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Level0SilentNormal => write!(f, "🟢 LEVEL 0: NORMAL ROUTINE FLUX (SILENT)"),
            Self::Level1CautionWarning => write!(f, "🟡 LEVEL 1: CAUTION ADVISORY BANNER"),
            Self::Level2EmergencyLiquidation => write!(f, "🔴 LEVEL 2: EMERGENCY LIQUIDATION ALARM (ACOUSTIC + FLASH)"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmEvent {
    pub tier: AlarmTier,
    pub portfolio_drawdown_pct: f64,
    pub margin_safety_buffer_pct: f64,
    pub acoustic_siren_triggered: bool,
    pub kill_switch_armed: bool,
    pub advisory_msg: String,
    pub emergency_action_prompt: Option<String>,
}

pub struct AcousticAlarmShield {
    drawdown_warning_threshold: f64,
    drawdown_panic_threshold: f64,
    margin_liquidation_buffer: f64,
}

impl Default for AcousticAlarmShield {
    fn default() -> Self {
        Self::new(5.0, 12.0, 15.0)
    }
}

impl AcousticAlarmShield {
    pub fn new(warn_pct: f64, panic_pct: f64, margin_min_pct: f64) -> Self {
        Self {
            drawdown_warning_threshold: warn_pct,
            drawdown_panic_threshold: panic_pct,
            margin_liquidation_buffer: margin_min_pct,
        }
    }

    /// Evaluates current portfolio drawdown and maintenance margin safety buffer.
    /// Emits acoustic terminal sirens (`\x07`) EXCLUSIVELY for Level 2 catastrophic liquidation scenarios.
    pub fn evaluate_risk_state(
        &self,
        drawdown_pct: f64,
        margin_buffer_pct: f64,
        enable_sound: bool,
    ) -> AlarmEvent {
        if drawdown_pct >= self.drawdown_panic_threshold || margin_buffer_pct <= self.margin_liquidation_buffer {
            if enable_sound {
                // Emit standard ASCII acoustic bell siren (\x07) repeatedly to draw immediate human attention
                print!("\x07\x07\x07");
                use std::io::Write;
                let _ = std::io::stdout().flush();
            }
            AlarmEvent {
                tier: AlarmTier::Level2EmergencyLiquidation,
                portfolio_drawdown_pct: drawdown_pct,
                margin_safety_buffer_pct: margin_buffer_pct,
                acoustic_siren_triggered: true,
                kill_switch_armed: true,
                advisory_msg: format!(
                    "CRITICAL DANGER: Intraday drawdown at {:.1}% exceeds safety threshold of {:.1}% or margin buffer ({:.1}%) approaching liquidation boundary! Acoustic alarms ringing.",
                    drawdown_pct, self.drawdown_panic_threshold, margin_buffer_pct
                ),
                emergency_action_prompt: Some(">>> [GLOBAL KILL SWITCH ARMED]: PRESS ENTER OR RUN 'b-terminal autopilot --stop' TO FLATTEN ALL POSITIONS & HALT BROKER EXECUTIONS IMMEDIATELY! <<<".to_string()),
            }
        } else if drawdown_pct >= self.drawdown_warning_threshold {
            AlarmEvent {
                tier: AlarmTier::Level1CautionWarning,
                portfolio_drawdown_pct: drawdown_pct,
                margin_safety_buffer_pct: margin_buffer_pct,
                acoustic_siren_triggered: false,
                kill_switch_armed: false,
                advisory_msg: format!(
                    "Caution: Portfolio drawdown at {:.1}% reached visual warning zone. Kelly sizing trimmed. No audible alarm sounded.",
                    drawdown_pct
                ),
                emergency_action_prompt: None,
            }
        } else {
            AlarmEvent {
                tier: AlarmTier::Level0SilentNormal,
                portfolio_drawdown_pct: drawdown_pct,
                margin_safety_buffer_pct: margin_buffer_pct,
                acoustic_siren_triggered: false,
                kill_switch_armed: false,
                advisory_msg: format!(
                    "All systems nominal. Drawdown at healthy {:.2}% with robust {:.1}% margin safety buffer. Acoustic shield silent.",
                    drawdown_pct, margin_buffer_pct
                ),
                emergency_action_prompt: None,
            }
        }
    }

    /// Generates a realistic simulation of any specified alarm tier for user training and audio/visual testing.
    pub fn simulate(tier: AlarmTier, enable_sound: bool) -> AlarmEvent {
        let shield = Self::default();
        match tier {
            AlarmTier::Level0SilentNormal => shield.evaluate_risk_state(1.8, 85.0, enable_sound),
            AlarmTier::Level1CautionWarning => shield.evaluate_risk_state(6.5, 45.0, enable_sound),
            AlarmTier::Level2EmergencyLiquidation => shield.evaluate_risk_state(14.8, 12.2, enable_sound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silent_normal_regime() {
        let shield = AcousticAlarmShield::default();
        let event = shield.evaluate_risk_state(2.5, 60.0, false);
        
        assert_eq!(event.tier, AlarmTier::Level0SilentNormal);
        assert!(!event.acoustic_siren_triggered);
        assert!(!event.kill_switch_armed);
        assert!(event.emergency_action_prompt.is_none());
    }

    #[test]
    fn test_caution_visual_warning() {
        let shield = AcousticAlarmShield::default();
        let event = shield.evaluate_risk_state(7.2, 50.0, false);
        
        assert_eq!(event.tier, AlarmTier::Level1CautionWarning);
        assert!(!event.acoustic_siren_triggered, "Level 1 must remain acoustically silent to avoid user fatigue");
        assert!(!event.kill_switch_armed);
    }

    #[test]
    fn test_emergency_liquidation_alarm() {
        let shield = AcousticAlarmShield::default();
        let event = shield.evaluate_risk_state(15.4, 10.5, false);
        
        assert_eq!(event.tier, AlarmTier::Level2EmergencyLiquidation);
        assert!(event.kill_switch_armed);
        assert!(event.emergency_action_prompt.is_some());
        assert!(event.advisory_msg.contains("CRITICAL DANGER"));
    }
}
