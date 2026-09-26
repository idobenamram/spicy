//! Source waveforms: their value over time.

use spicy_circuit::Waveform;
use std::f64::consts::PI;

/// The source's value in DC analyses: its waveform at t = 0, as in ngspice
/// (`vsrcload.c`). A pulse or an exponential starts at its initial value; a
/// sine at its offset plus the amplitude times the sine of its phase.
pub(crate) fn dc_value(waveform: &Waveform) -> f64 {
    match *waveform {
        Waveform::Dc(value) => value,
        Waveform::Pulse { initial, .. } | Waveform::Exp { initial, .. } => initial,
        Waveform::Sine {
            offset,
            amplitude,
            phase,
            ..
        } => offset + amplitude * phase.sin(),
    }
}

/// The waveform's value at time `t`. Unset timings default to the step `dt` or
/// the stop time `tstop`, as in SPICE.
pub(crate) fn value_at(waveform: &Waveform, t: f64, dt: f64, tstop: f64) -> f64 {
    match *waveform {
        Waveform::Dc(value) => value,
        Waveform::Pulse {
            initial: v1,
            pulsed: v2,
            delay: td,
            rise,
            fall,
            width,
            period,
            count,
        } => {
            let tr = rise.unwrap_or(dt);
            let tf = fall.unwrap_or(dt);
            let pw = width.unwrap_or(tstop);
            let per = period.unwrap_or(tstop);
            let unlimited = count == 0;

            if t < td {
                return v1;
            }

            // End after `count` periods if finite and periodic
            if !unlimited && t >= td + (count as f64) * per {
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
        Waveform::Sine {
            offset: v0,
            amplitude: va,
            frequency,
            delay: td,
            damping: theta,
            phase,
        } => {
            let f = frequency.unwrap_or(1.0 / tstop);

            if t < td {
                return v0;
            }

            let input = (2.0 * PI * f * (t - td) + phase).rem_euclid(2.0 * PI);

            v0 + va * f64::exp(-(t - td) * theta) * f64::sin(input)
        }
        Waveform::Exp {
            initial: v1,
            pulsed: v2,
            rise_delay: td1,
            rise_tau,
            fall_delay,
            fall_tau,
        } => {
            let tau1 = rise_tau.unwrap_or(dt);
            let td2 = fall_delay.unwrap_or(td1 + dt);
            let tau2 = fall_tau.unwrap_or(dt);

            let v21 = v2 - v1;
            let v12 = v1 - v2;

            if t < td1 {
                return v1;
            }
            if td1 <= t && t < td2 {
                return v1 + v21 * (1. - f64::exp(-(t - td1) / tau1));
            }

            v1 + v21 * (1. - f64::exp(-(t - td1) / tau1)) + v12 * (1. - f64::exp(-(t - td2) / tau2))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0 V to 5 V: 1 s delay, 1 s edges, 2 s high, 10 s period.
    fn pulse(count: u64) -> Waveform {
        Waveform::Pulse {
            initial: 0.0,
            pulsed: 5.0,
            delay: 1.0,
            rise: Some(1.0),
            fall: Some(1.0),
            width: Some(2.0),
            period: Some(10.0),
            count,
        }
    }

    #[test]
    fn dc_value_is_the_waveform_at_time_zero() {
        assert_eq!(dc_value(&Waveform::Dc(3.0)), 3.0);
        assert_eq!(dc_value(&pulse(0)), 0.0);
        let sine = Waveform::Sine {
            offset: 1.0,
            amplitude: 2.0,
            frequency: None,
            delay: 1.0,
            damping: 0.0,
            phase: PI / 2.0,
        };
        assert_eq!(dc_value(&sine), 3.0);
        let exp = Waveform::Exp {
            initial: 0.25,
            pulsed: 5.0,
            rise_delay: 0.0,
            rise_tau: None,
            fall_delay: None,
            fall_tau: None,
        };
        assert_eq!(dc_value(&exp), 0.25);
    }

    #[test]
    fn pulse_rises_holds_falls_and_repeats() {
        let at = |t| value_at(&pulse(0), t, 0.1, 100.0);
        assert_eq!(at(0.5), 0.0, "before the delay");
        assert_eq!(at(1.5), 2.5, "halfway up");
        assert_eq!(at(3.0), 5.0, "high");
        assert_eq!(at(4.5), 2.5, "halfway down");
        assert_eq!(at(6.0), 0.0, "low");
        assert_eq!(at(11.5), 2.5, "halfway up, second period");
    }

    #[test]
    fn pulse_stops_after_its_count() {
        assert_eq!(value_at(&pulse(1), 11.5, 0.1, 100.0), 0.0);
    }

    #[test]
    fn pulse_timing_defaults_to_the_step_and_stop_time() {
        let pulse = Waveform::Pulse {
            initial: 0.0,
            pulsed: 5.0,
            delay: 0.0,
            rise: None,
            fall: None,
            width: None,
            period: None,
            count: 0,
        };
        // Rise time = step (1), width = period = stop time (10).
        assert_eq!(value_at(&pulse, 0.5, 1.0, 10.0), 2.5);
        assert_eq!(value_at(&pulse, 5.0, 1.0, 10.0), 5.0);
    }

    #[test]
    fn sine_starts_at_its_delay() {
        let sine = Waveform::Sine {
            offset: 1.0,
            amplitude: 2.0,
            frequency: Some(0.25),
            delay: 1.0,
            damping: 0.0,
            phase: 0.0,
        };
        assert_eq!(value_at(&sine, 0.5, 0.1, 10.0), 1.0, "before the delay");
        assert_eq!(value_at(&sine, 2.0, 0.1, 10.0), 3.0, "a quarter period in");
    }

    #[test]
    fn sine_frequency_defaults_to_one_period_over_the_stop_time() {
        let sine = Waveform::Sine {
            offset: 0.0,
            amplitude: 1.0,
            frequency: None,
            delay: 0.0,
            damping: 0.0,
            phase: 0.0,
        };
        assert_eq!(value_at(&sine, 1.0, 0.1, 4.0), 1.0);
    }

    #[test]
    fn exp_rises_then_falls() {
        let exp = Waveform::Exp {
            initial: 0.0,
            pulsed: 1.0,
            rise_delay: 1.0,
            rise_tau: Some(1.0),
            fall_delay: Some(3.0),
            fall_tau: Some(1.0),
        };
        let one_tau = 1.0 - (-1.0f64).exp();
        let three_tau = 1.0 - (-3.0f64).exp();
        assert_eq!(value_at(&exp, 0.5, 0.1, 10.0), 0.0, "before the delay");
        assert_eq!(value_at(&exp, 2.0, 0.1, 10.0), one_tau, "rising");
        assert_eq!(
            value_at(&exp, 4.0, 0.1, 10.0),
            three_tau - one_tau,
            "falling"
        );
    }
}
