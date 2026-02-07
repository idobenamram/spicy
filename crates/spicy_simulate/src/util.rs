// TODO: this kinda sucks
pub(crate) fn get_voltage_diff(
    voltages: &[f64],
    positive: Option<usize>,
    negative: Option<usize>,
) -> f64 {
    match (positive, negative) {
        (Some(positive), Some(negative)) => voltages[positive] - voltages[negative],
        (Some(positive), None) => voltages[positive],
        (None, Some(negative)) => -voltages[negative],
        (None, None) => 0.0,
    }
}
