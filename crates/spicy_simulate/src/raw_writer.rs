use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;

use chrono::Local;
use spicy_parser::instance_parser::Deck;

use crate::{DcSweepResult, OperatingPointResult, TransientResult};

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

fn build_trace_variables_from_names(
    node_names: &[String],
    source_names: &[String],
) -> Vec<(String, String)> {
    let mut vars = Vec::new();
    // Node voltages
    for n in node_names {
        if n.is_empty() {
            continue;
        }
        vars.push((format!("V({})", n), "voltage".to_string()));
    }
    // Source currents
    for s in source_names {
        if s.is_empty() {
            continue;
        }
        vars.push((format!("I({})", s), "device_current".to_string()));
    }
    vars
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
    variables: &[(String, String)],
    start_index: usize,
) -> std::io::Result<()> {
    for (i, (name, kind)) in variables.iter().enumerate() {
        writeln!(w, "\t{}\t{}\t{}", start_index + i, name, kind)?;
    }
    Ok(())
}

fn write_binary_series_real_f32(
    mut w: impl Write,
    x_values: &[f64],
    traces_per_point: &[Vec<f64>],
) -> std::io::Result<()> {
    writeln!(w, "Binary:")?;
    for (idx, &x) in x_values.iter().enumerate() {
        let _ = idx;
        w.write_all(&x.to_le_bytes())?;
        for &v in &traces_per_point[idx] {
            let f = v as f32;
            w.write_all(&f.to_le_bytes())?;
        }
    }
    Ok(())
}

pub(crate) fn write_transient_raw(
    deck: &Deck,
    result: &TransientResult,
    output_base: &str,
) -> std::io::Result<PathBuf> {
    let filename = format!("{}.raw", sanitize_filename(output_base));
    let path = PathBuf::from(filename);
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    let traces = build_trace_variables_from_names(&result.node_names, &result.source_names);
    let nvars = 1 + traces.len();
    let npoints = result.times.len();

    write_header(
        &mut writer,
        &deck.title,
        "Transient Analysis",
        "real forward",
        nvars,
        npoints,
    )?;
    writeln!(&mut writer, "\t0\ttime\ttime")?;
    write_variables_with_offset(&mut writer, &traces, 1)?;
    write_binary_series_real_f32(&mut writer, &result.times, &result.samples)?;

    writer.flush()?;
    Ok(path)
}

pub(crate) fn write_operating_point_raw(
    deck: &Deck,
    op: &OperatingPointResult,
    output_base: &str,
) -> std::io::Result<PathBuf> {
    let filename = format!("{}.raw", sanitize_filename(output_base));
    let path = PathBuf::from(filename);
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    // Build variable names from the provided result ordering
    let mut variables: Vec<(String, String)> = Vec::new();
    for (name, _) in &op.voltages {
        variables.push((format!("V({})", name), "voltage".to_string()));
    }
    for (name, _) in &op.currents {
        variables.push((format!("I({})", name), "device_current".to_string()));
    }
    let nvars = variables.len();

    // Preamble: OP has no forward flag
    write_header(
        &mut writer,
        &deck.title,
        "Operation Point",
        "real",
        nvars,
        1,
    )?;
    write_variables_with_offset(&mut writer, &variables, 0)?;
    writeln!(&mut writer, "Binary:")?;
    // Single point: write f32 for each variable in order
    for (_, v) in &op.voltages {
        writer.write_all(&(*v as f32).to_le_bytes())?;
    }
    for (_, i) in &op.currents {
        writer.write_all(&(*i as f32).to_le_bytes())?;
    }
    writer.flush()?;
    Ok(path)
}

pub(crate) fn write_dc_raw(
    deck: &Deck,
    dc: &DcSweepResult,
    output_base: &str,
    sweep_name: &str,
    is_voltage_source: bool,
) -> std::io::Result<PathBuf> {
    let filename = format!("{}.raw", sanitize_filename(output_base));
    let path = PathBuf::from(filename);
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    // Assume non-empty results
    let (first_op, _) = dc.results.first().expect("dc results not empty");
    let mut variables: Vec<(String, String)> = Vec::new();
    // var0: swept value
    let sweep_var_name = sweep_name.to_string();
    let sweep_type = if is_voltage_source {
        "voltage"
    } else {
        "device_current"
    };

    // Preamble
    let trace_count = first_op.voltages.len() + first_op.currents.len();
    write_header(
        &mut writer,
        &deck.title,
        "DC transfer characteristic",
        "real forward",
        trace_count + 1,
        dc.results.len(),
    )?;
    // index 0
    writeln!(&mut writer, "\t0\t{}\t{}", sweep_var_name, sweep_type)?;
    // Then traces
    for (idx, (name, _)) in first_op.voltages.iter().enumerate() {
        variables.push((format!("V({})", name), "voltage".to_string()));
        writeln!(&mut writer, "\t{}\tV({})\tvoltage", idx + 1, name)?;
    }
    for (iidx, (name, _)) in first_op.currents.iter().enumerate() {
        writeln!(
            &mut writer,
            "\t{}\tI({})\tdevice_current",
            first_op.voltages.len() + 1 + iidx,
            name
        )?;
    }

    // Binary
    writeln!(&mut writer, "Binary:")?;
    for (op, sweep) in &dc.results {
        writer.write_all(&sweep.to_le_bytes())?; // swept value as f64
        for (_, v) in &op.voltages {
            writer.write_all(&(*v as f32).to_le_bytes())?;
        }
        for (_, i) in &op.currents {
            writer.write_all(&(*i as f32).to_le_bytes())?;
        }
    }
    writer.flush()?;
    Ok(path)
}

pub(crate) fn write_ac_raw(
    deck: &Deck,
    ac: &Vec<(f64, ndarray::Array1<f64>, ndarray::Array1<f64>)>,
    output_base: &str,
) -> std::io::Result<PathBuf> {
    let filename = format!("{}.raw", sanitize_filename(output_base));
    let path = PathBuf::from(filename);
    let file = File::create(&path)?;
    let mut writer = BufWriter::new(file);

    // Rebuild names
    let nodes = crate::nodes::Nodes::new(&deck.devices);
    let node_names = nodes.get_node_names();
    let source_names = nodes.get_source_names();
    let traces = build_trace_variables_from_names(&node_names, &source_names);
    let trace_count = traces.len();

    // Preamble
    write_header(
        &mut writer,
        &deck.title,
        "AC Analysis",
        "complex forward",
        trace_count + 1,
        ac.len(),
    )?;
    writeln!(&mut writer, "\t0\tfrequency\tfrequency")?;
    write_variables_with_offset(&mut writer, &traces, 1)?;

    // Binary: per point -> f64 frequency, then for each trace: f64 re, f64 im
    writeln!(&mut writer, "Binary:")?;
    let n = nodes.node_len();
    let k = nodes.source_len();
    for (f, xr, xi) in ac {
        writer.write_all(&f.to_le_bytes())?;
        // node voltages
        for i in 0..n {
            let re = xr[i];
            let im = xi[i];
            writer.write_all(&re.to_le_bytes())?;
            writer.write_all(&im.to_le_bytes())?;
        }
        // source currents
        for i in 0..k {
            let re = xr[n + i];
            let im = xi[n + i];
            writer.write_all(&re.to_le_bytes())?;
            writer.write_all(&im.to_le_bytes())?;
        }
    }
    writer.flush()?;
    Ok(path)
}
