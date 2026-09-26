use crate::expr::Value;

#[derive(Debug, Clone)]
pub enum WaveForm {
    Pulse {
        /// v1 (volts, amps)
        voltage1: Value,
        /// v2 (volts, amps)
        voltage2: Value,
        /// TD (seconds)
        delay: Option<Value>,
        /// TR (seconds)
        rise_time: Option<Value>,
        /// TF (seconds)
        fall_time: Option<Value>,
        /// PW (seconds)
        pulse_width: Option<Value>,
        /// PER (seconds)
        period: Option<Value>,
        /// NP (whole number)
        number_of_pulses: Option<u64>,
    },
    Sinusoidal {
        // VO (volts, amps)
        offset: Value,
        // Va (volts, amps)
        amplitude: Value,
        // FREQ (Hz)
        frequency: Option<Value>,
        // TD (seconds)
        delay: Option<Value>,
        // THETA (1/second)
        damping_factor: Option<Value>,
        // PHASE (degrees by default; use deg/rad suffix to override)
        phase: Option<Value>,
    },
    Exponential {
        /// V1 (volts, amps)
        initial_value: Value,
        /// V2 (volts, amps)
        pulsed_value: Value,
        /// TD1 (seconds)
        rise_delay_time: Option<Value>,
        /// TAU1 (seconds)
        rise_time_constant: Option<Value>,
        /// TD2 (seconds)
        fall_delay_time: Option<Value>,
        /// TAU2 (seconds)
        fall_time_constant: Option<Value>,
    },
    Constant(Value),
}
