use std::f64::consts::PI;

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
        // PHASE (degrees)
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

impl WaveForm {
    pub fn compute(&self, t: f64, dt: f64, time_stop: f64) -> f64 {
        match self {
            WaveForm::Pulse {
                voltage1,
                voltage2,
                delay,
                rise_time,
                fall_time,
                pulse_width,
                period,
                number_of_pulses,
            } => {
                let v1 = voltage1.get_value();
                let v2 = voltage2.get_value();
                let td = delay.as_ref().unwrap_or(&Value::zero()).get_value();
                let tr = rise_time
                    .as_ref()
                    .unwrap_or(&Value::new(dt, None, None))
                    .get_value();
                let tf = fall_time
                    .as_ref()
                    .unwrap_or(&Value::new(dt, None, None))
                    .get_value();
                let pw = pulse_width
                    .as_ref()
                    .unwrap_or(&Value::new(time_stop, None, None))
                    .get_value();
                let per = period
                    .as_ref()
                    .unwrap_or(&Value::new(time_stop, None, None))
                    .get_value();
                let np = *number_of_pulses.as_ref().unwrap_or(&0);
                let unlimted = np == 0;

                if t < td {
                    return v1;
                }

                // End after NP periods if finite and periodic
                if !unlimted && t >= td + (np as f64) * per {
                    return v1;
                }

                let dv = v2 - v1;

                let s = (t - td).rem_euclid(per); // in [0, per)

                if s < 0.0 {
                    v1
                } else if s < tr {
                    if tr > 0.0 { v1 + dv * (s / tr) } else { v2 }
                } else if s < tr + pw {
                    v2
                } else if s < tr + pw + tf {
                    if tf > 0.0 {
                        v2 - dv * ((s - tr - pw) / tf)
                    } else {
                        v1
                    }
                } else {
                    v1
                }
            }
            WaveForm::Sinusoidal {
                offset,
                amplitude,
                frequency,
                delay,
                damping_factor,
                phase,
            } => {
                let v0 = offset.get_value();
                let va = amplitude.get_value();
                let f = frequency
                    .as_ref()
                    .unwrap_or(&Value::new(1.0 / time_stop, None, None))
                    .get_value();
                let td = delay.as_ref().unwrap_or(&Value::zero()).get_value();
                let theta = damping_factor
                    .as_ref()
                    .unwrap_or(&Value::zero())
                    .get_value();
                let ph = phase.as_ref().unwrap_or(&Value::zero()).get_value();

                if t < td {
                    return v0;
                }

                let input = (2.0 * PI * f * (t - td) + ph).rem_euclid(2.0 * PI);
                
                v0 + va * f64::exp(-(t - td) * theta) * f64::sin(input)
            }
            WaveForm::Exponential {
                initial_value,
                pulsed_value,
                rise_delay_time,
                rise_time_constant,
                fall_delay_time,
                fall_time_constant,
            } => {
                let v1 = initial_value.get_value();
                let v2 = pulsed_value.get_value();
                let td1 = rise_delay_time
                    .as_ref()
                    .unwrap_or(&Value::zero())
                    .get_value();
                let tau1 = rise_time_constant
                    .as_ref()
                    .unwrap_or(&Value::new(dt, None, None))
                    .get_value();
                let td2 = fall_delay_time
                    .as_ref()
                    .unwrap_or(&Value::new(td1 + dt, None, None))
                    .get_value();
                let tau2 = fall_time_constant
                    .as_ref()
                    .unwrap_or(&Value::new(dt, None, None))
                    .get_value();

                let v21 = v2 - v1;
                let v12 = v1 - v2;

                if t < td1 {
                    return v1;
                }
                if td1 <= t && t < td2 {
                    return v1 + v21 * (1. - f64::exp(-(t - td1) / tau1));
                }
                
                v1
                    + v21 * (1. - f64::exp(-(t - td1) / tau1))
                    + v12 * (1. - f64::exp(-(t - td2) / tau2))
            }
            WaveForm::Constant(value) => {
                value.get_value()
            }
        }
    }
}
