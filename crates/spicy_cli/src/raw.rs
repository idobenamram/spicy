use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use spicy_circuit::{Lowered, SourceRef};
use spicy_simulate::{
    AcResult, DcSweepResult, OperatingPointResult, TransientResult, Unknown, unknowns,
};

// TODO: kinda vibe coded this so it can definitly be improved

fn sanitize_filename(input: &str) -> String {
    let mut out = String::new();
    for c in input.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' | '.' => out.push(c),
            ' ' => out.push('_'),
            _ => {}
        }
    }
    if out.is_empty() {
        "spicy".to_string()
    } else {
        out
    }
}

/// Path of the raw file for `stem` in `dir`, e.g. `<dir>/amp.raw`.
pub(crate) fn raw_file_path(dir: &Path, stem: &str) -> PathBuf {
    dir.join(format!("{}.raw", sanitize_filename(stem)))
}

/// The raw-file variable for each entry of a solution vector, in order:
/// `V(node)` for node voltages, `I(device)` for branch currents.
fn solution_variables(lowered: &Lowered) -> Vec<(String, &'static str)> {
    unknowns(&lowered.circuit)
        .iter()
        .map(|unknown| {
            let name = unknown.name(&lowered.names);
            match unknown {
                Unknown::Voltage(_) => (format!("V({name})"), "voltage"),
                Unknown::Current(_) => (format!("I({name})"), "device_current"),
            }
        })
        .collect()
}

fn write_header(
    mut w: impl Write,
    title: &str,
    plotname: &str,
    flags: &str,
    nvars: usize,
    npoints: usize,
) -> std::io::Result<()> {
    writeln!(w, "Title: *{}", title.trim())?;
    let now = Local::now();
    writeln!(w, "Date: {}", now.format("%a %b %d %H:%M:%S %Y"))?;
    writeln!(w, "Plotname: {}", plotname)?;
    writeln!(w, "Flags: {}", flags)?;
    writeln!(w, "No. Variables: {}", nvars)?;
    writeln!(w, "No. Points: {}", npoints)?;
    writeln!(w, "Command: spicy")?;
    writeln!(w, "Variables:")?;
    Ok(())
}

fn write_variables_with_offset(
    mut w: impl Write,
    variables: &[(String, &str)],
    start_index: usize,
) -> std::io::Result<()> {
    for (i, (name, kind)) in variables.iter().enumerate() {
        writeln!(w, "\t{}\t{}\t{}", start_index + i, name, kind)?;
    }
    Ok(())
}

/// Per point: the x value as f64, then each trace as f32.
fn write_binary_series_real_f32<'a>(
    mut w: impl Write,
    points: impl IntoIterator<Item = (f64, &'a [f64])>,
) -> std::io::Result<()> {
    writeln!(w, "Binary:")?;
    for (x, traces) in points {
        w.write_all(&x.to_le_bytes())?;
        for &v in traces {
            w.write_all(&(v as f32).to_le_bytes())?;
        }
    }
    Ok(())
}

pub(crate) fn write_transient_raw(
    lowered: &Lowered,
    result: &TransientResult,
    path: &Path,
) -> std::io::Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    let traces = solution_variables(lowered);
    write_header(
        &mut writer,
        &lowered.names.title,
        "Transient Analysis",
        "real forward",
        traces.len() + 1,
        result.times.len(),
    )?;
    writeln!(&mut writer, "\t0\ttime\ttime")?;
    write_variables_with_offset(&mut writer, &traces, 1)?;
    let samples = result.samples.iter().map(Vec::as_slice);
    write_binary_series_real_f32(&mut writer, result.times.iter().copied().zip(samples))?;
    writer.flush()
}

pub(crate) fn write_operating_point_raw(
    lowered: &Lowered,
    op: &OperatingPointResult,
    path: &Path,
) -> std::io::Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    let variables = solution_variables(lowered);
    // An operating point has no x variable and no `forward` flag.
    write_header(
        &mut writer,
        &lowered.names.title,
        "Operation Point",
        "real",
        variables.len(),
        1,
    )?;
    write_variables_with_offset(&mut writer, &variables, 0)?;
    writeln!(&mut writer, "Binary:")?;
    for &v in &op.solution {
        writer.write_all(&(v as f32).to_le_bytes())?;
    }
    writer.flush()
}

pub(crate) fn write_dc_raw(
    lowered: &Lowered,
    dc: &DcSweepResult,
    source: SourceRef,
    path: &Path,
) -> std::io::Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    let traces = solution_variables(lowered);
    let (sweep_name, sweep_type) = match source {
        SourceRef::Voltage(v) => (&lowered.names.vsources[v.index()], "voltage"),
        SourceRef::Current(i) => (&lowered.names.isources[i.index()], "device_current"),
    };
    write_header(
        &mut writer,
        &lowered.names.title,
        "DC transfer characteristic",
        "real forward",
        traces.len() + 1,
        dc.results.len(),
    )?;
    writeln!(&mut writer, "\t0\t{sweep_name}\t{sweep_type}")?;
    write_variables_with_offset(&mut writer, &traces, 1)?;
    let points = dc
        .results
        .iter()
        .map(|(op, sweep)| (*sweep, op.solution.as_slice()));
    write_binary_series_real_f32(&mut writer, points)?;
    writer.flush()
}

pub(crate) fn write_ac_raw(lowered: &Lowered, ac: &AcResult, path: &Path) -> std::io::Result<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    let traces = solution_variables(lowered);
    write_header(
        &mut writer,
        &lowered.names.title,
        "AC Analysis",
        "complex forward",
        traces.len() + 1,
        ac.len(),
    )?;
    writeln!(&mut writer, "\t0\tfrequency\tfrequency")?;
    write_variables_with_offset(&mut writer, &traces, 1)?;
    // Per point: f64 frequency, then (re, im) as f64 for each trace.
    writeln!(&mut writer, "Binary:")?;
    for (f, xr, xi) in ac {
        writer.write_all(&f.to_le_bytes())?;
        for (re, im) in xr.iter().zip(xi) {
            writer.write_all(&re.to_le_bytes())?;
            writer.write_all(&im.to_le_bytes())?;
        }
    }
    writer.flush()
}
