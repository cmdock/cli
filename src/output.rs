use std::io::{self, Write};

use anyhow::Result;
use serde::Serialize;

pub fn note(msg: impl AsRef<str>) {
    let _ = writeln!(io::stderr(), "{}", msg.as_ref());
}

pub fn print_json<T: Serialize>(value: &T) -> Result<()> {
    serde_json::to_writer_pretty(io::stdout(), value)?;
    writeln!(io::stdout())?;
    Ok(())
}

pub fn print_lines(lines: &[String]) -> Result<()> {
    let mut out = io::stdout();
    for line in lines {
        writeln!(out, "{line}")?;
    }
    Ok(())
}

pub fn print_table(headers: &[&str], rows: &[Vec<String>]) -> Result<()> {
    let mut widths = headers
        .iter()
        .map(|header| header.len())
        .collect::<Vec<_>>();
    for row in rows {
        for (idx, cell) in row.iter().enumerate() {
            if idx >= widths.len() {
                widths.push(cell.len());
            } else {
                widths[idx] = widths[idx].max(cell.len());
            }
        }
    }

    let mut out = io::stdout();
    for (idx, header) in headers.iter().enumerate() {
        if idx > 0 {
            write!(out, "  ")?;
        }
        write!(out, "{header:<width$}", width = widths[idx])?;
    }
    writeln!(out)?;

    for (idx, width) in widths.iter().enumerate() {
        if idx > 0 {
            write!(out, "  ")?;
        }
        write!(out, "{:-<width$}", "", width = *width)?;
    }
    writeln!(out)?;

    for row in rows {
        for (idx, width) in widths.iter().enumerate() {
            if idx > 0 {
                write!(out, "  ")?;
            }
            let cell = row.get(idx).map(String::as_str).unwrap_or("");
            write!(out, "{cell:<width$}", width = *width)?;
        }
        writeln!(out)?;
    }

    Ok(())
}
