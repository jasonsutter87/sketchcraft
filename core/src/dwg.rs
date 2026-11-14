//! DWG, AutoCAD's own (closed, binary) format, through a converter the user
//! installs: LibreDWG (`brew install libredwg`, GPL, run as a separate
//! program) or the free ODA File Converter. SketchCraft reads and writes DXF
//! itself; the converter only translates between DXF and DWG.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Result};

const HELP: &str = "Opening and saving DWG needs a converter: install LibreDWG (brew install libredwg) or the free ODA File Converter, then try again. DXF works without one.";

fn find(name: &str) -> Option<PathBuf> {
    let dirs = std::env::var("PATH").unwrap_or_default();
    std::env::split_paths(&dirs).chain(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from)).map(|d| d.join(name)).find(|p| p.is_file())
}

fn oda() -> Option<PathBuf> {
    ["/Applications/ODAFileConverter.app/Contents/MacOS/ODAFileConverter", "/usr/bin/ODAFileConverter", "/usr/local/bin/ODAFileConverter"].iter().map(PathBuf::from).find(|p| p.is_file()).or_else(|| find("ODAFileConverter"))
}

/// Which converter is available (for the app to say so).
pub fn converter() -> Option<&'static str> {
    if find("dwg2dxf").is_some() {
        Some("LibreDWG")
    } else if oda().is_some() {
        Some("ODA File Converter")
    } else {
        None
    }
}

fn run(mut c: Command) -> Result<()> {
    let out = c.output()?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("the DWG converter failed: {}", err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no message"));
    }
    Ok(())
}

/// Convert a DWG to DXF in `tmp`; returns the DXF path.
pub fn to_dxf(dwg: &Path, tmp: &Path) -> Result<PathBuf> {
    if !dwg.is_file() {
        bail!("no file at {}", dwg.display());
    }
    let out = tmp.join("drawing.dxf");
    if let Some(bin) = find("dwg2dxf") {
        let mut c = Command::new(bin);
        c.arg("-y").arg("-o").arg(crate::safety::path_arg(&out)).arg(crate::safety::path_arg(dwg));
        run(c)?;
    } else if let Some(bin) = oda() {
        // ODA converts folders: copy the one file into its own input folder
        let input = tmp.join("in");
        std::fs::create_dir_all(&input)?;
        std::fs::copy(dwg, input.join("drawing.dwg"))?;
        let mut c = Command::new(bin);
        c.arg(crate::safety::path_arg(&input)).arg(crate::safety::path_arg(tmp)).args(["ACAD2018", "DXF", "0", "1", "*.DWG"]);
        run(c)?;
    } else {
        bail!("{HELP}");
    }
    if !out.is_file() {
        bail!("the DWG converter didn't produce a drawing (is the file a DWG?)");
    }
    Ok(out)
}

/// Convert our DXF to a DWG at `dwg`.
pub fn from_dxf(dxf: &Path, dwg: &Path) -> Result<()> {
    if let Some(bin) = find("dxf2dwg") {
        let mut c = Command::new(bin);
        c.arg("-y").arg("-o").arg(crate::safety::path_arg(dwg)).arg(crate::safety::path_arg(dxf));
        run(c)?;
    } else if let Some(bin) = oda() {
        let dir = dxf.parent().unwrap_or(Path::new("."));
        let out = dir.join("out");
        std::fs::create_dir_all(&out)?;
        let mut c = Command::new(bin);
        c.arg(crate::safety::path_arg(dir)).arg(crate::safety::path_arg(&out)).args(["ACAD2018", "DWG", "0", "1", "*.DXF"]);
        run(c)?;
        let made = out.join(dxf.with_extension("dwg").file_name().unwrap());
        std::fs::copy(&made, dwg)?;
    } else {
        bail!("{HELP}");
    }
    if !dwg.is_file() {
        bail!("the DWG converter didn't write {}", dwg.display());
    }
    Ok(())
}
