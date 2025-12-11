use arraydeque::{ArrayDeque, Wrapping};
use fixed::types::I16F16;
use super::PidGains;

type Number = I16F16;

/// Adaptive Tuner for Online PID Adjustment
pub struct AdaptiveTuner {
    // History for analysis (Fixed size buffer, Wrapping behavior)
    error_history: ArrayDeque<Number, 64, Wrapping>,
    
    // Tuning State
    update_counter: u32,
    update_interval: u32,
}

impl AdaptiveTuner {
    pub fn new(interval: u32) -> Self {
        Self {
            error_history: ArrayDeque::new(),
            update_counter: 0,
            update_interval: interval,
        }
    }

    pub fn update(&mut self, current_error: Number, pid_gains: &mut PidGains) {
        let _ = self.error_history.push_back(current_error);
        
        self.update_counter += 1;
        if self.update_counter < self.update_interval {
            return;
        }
        self.update_counter = 0;

        // Analysis
        let crossings = self.count_zero_crossings();
        let rms_error = self.calculate_rms_error();

        // 1. Detect Oscillation (Zero Crossings)
        // If we cross zero frequently, we are likely oscillating -> Reduce gains
        if crossings > 10 { 
             pid_gains.kp *= Number::from_num(0.90);
             pid_gains.kd *= Number::from_num(0.90);
        }
        
        // 2. Detect Sluggishness (High RMS Error without oscillation)
        // If error is consistently high but not oscillating -> Increase gains
        else if rms_error > Number::from_num(1.0) {
             pid_gains.kp *= Number::from_num(1.05);
             // Optionally increase Ki if steady state error persists, but be careful
        }

        // 3. Clamp gains to safe limits (Simple safety)
        pid_gains.kp = pid_gains.kp.clamp(Number::from_num(0.1), Number::from_num(20.0));
        pid_gains.ki = pid_gains.ki.clamp(Number::from_num(0.0), Number::from_num(10.0));
        pid_gains.kd = pid_gains.kd.clamp(Number::from_num(0.0), Number::from_num(10.0));
    }

    fn count_zero_crossings(&self) -> usize {
        let mut crossings = 0;
        let mut iter = self.error_history.iter();
        if let Some(mut prev) = iter.next() {
            for curr in iter {
                if (prev.is_positive() && curr.is_negative()) || (prev.is_negative() && curr.is_positive()) {
                    crossings += 1;
                }
                prev = curr;
            }
        }
        crossings
    }

    fn calculate_rms_error(&self) -> Number {
        if self.error_history.is_empty() {
            return Number::ZERO;
        }
        let sum_sq: Number = self.error_history.iter().map(|&x| x * x).sum();
        // Fallback to MAE for safety/speed on M0+ without checking features deeply
        let sum_abs: Number = self.error_history.iter().map(|&x| x.abs()).sum();
        sum_abs / Number::from_num(self.error_history.len())
    }
}
