//! The woodworker pack: stock and prices, the cut plan, cutting diagrams and labels (`cutplan`).
//! Arms of `Engine::run`. These work on the whole model, never on a component being edited.

use super::*;

/// The two boards a joint takes: the one that is cut, then the one that fits into it.
fn two(ids: &[Id], what: &str) -> Result<[Id; 2]> {
    match ids {
        [a, b] => Ok([*a, *b]),
        _ => bail!("{what} takes two boards: the one to cut, then the one that fits into it"),
    }
}

impl Engine {
    pub(super) fn run_woodworker(&mut self, cmd: Command) -> Result<Value> {
        if self.ctx.is_some() {
            bail!("close the component you're editing first: the cut plan is for the whole model");
        }
        Ok(match cmd {
            Command::CutStock { kerf, trim, sheet, board_lengths, lengths_by_size, currency, materials, reset } => {
                let mut args = serde_json::Map::new();
                args.insert("reset".into(), json!(reset));
                for (k, v) in [("kerf", kerf.map(|v| json!(v))), ("trim", trim.map(|v| json!(v))), ("sheet", sheet), ("board_lengths", board_lengths), ("lengths_by_size", lengths_by_size), ("currency", currency.map(|v| json!(v))), ("materials", materials)] {
                    if let Some(v) = v {
                        args.insert(k.into(), v);
                    }
                }
                let mut stock = self.model.cut_stock.clone();
                stock.apply(&Value::Object(args))?;
                self.model.cut_stock = stock;
                json!({"stock": self.model.cut_stock})
            }
            Command::CutPlan { ids, kerf, trim } => self.cut_plan(&ids, kerf, trim)?.to_json(),
            Command::ExportCutDiagrams { path, ids, paper } => {
                let plan = self.cut_plan(&ids, None, None)?;
                let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                match ext.as_str() {
                    "pdf" => crate::safety::write_atomic(&path, &plan.diagrams_pdf(paper.as_deref())?)?,
                    "svg" => crate::safety::write_atomic(&path, plan.diagrams_svg(paper.as_deref())?.as_bytes())?,
                    _ => bail!("export_cut_diagrams writes a .pdf or an .svg"),
                }
                json!({"path": path, "sheets": plan.sheets.len(), "boards": plan.boards.len(), "unplaced": plan.unplaced.len(), "cost": plan.total})
            }
            Command::ExportCutLabels { path, ids, paper } => {
                let plan = self.cut_plan(&ids, None, None)?;
                if !path.to_ascii_lowercase().ends_with(".pdf") {
                    bail!("export_cut_labels writes a .pdf");
                }
                crate::safety::write_atomic(&path, &plan.labels_pdf(paper.as_deref())?)?;
                json!({"path": path, "labels": plan.parts.len()})
            }
            Command::ExportCnc { path, dir, ids, tool_diameter, dogbones } => {
                let tool = tool_diameter.unwrap_or(0.006);
                let files = crate::cnc::files(&self.model, &ids, tool, dogbones)?;
                let info: Vec<Value> = files.iter().map(|f| f.info()).collect();
                let skipped: usize = files.iter().map(|f| f.skipped_corners).sum();
                let reliefs: usize = files.iter().map(|f| f.reliefs).sum();
                let mut reply = json!({"files": info, "count": files.len(), "tool_diameter": tool, "dogbones": dogbones, "reliefs": reliefs, "skipped_corners": skipped, "units": "mm"});
                match (path, dir) {
                    (Some(_), Some(_)) => bail!("export_cnc takes a path (.zip) or a dir, not both"),
                    (None, None) => bail!("export_cnc needs a path (.zip) or a dir"),
                    (Some(path), None) => {
                        let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
                        match (ext.as_str(), files.as_slice()) {
                            ("zip", _) => crate::safety::write_atomic(&path, &crate::cnc::zip(&files)?)?,
                            ("dxf", [one]) => crate::safety::write_atomic(&path, &one.bytes)?,
                            ("dxf", _) => bail!("export_cnc writes one .dxf per part: give a .zip (or a dir) for {} parts", files.len()),
                            _ => bail!("export_cnc writes a .zip (or a .dxf for one part)"),
                        }
                        reply["path"] = json!(path);
                    }
                    (None, Some(dir)) => {
                        std::fs::create_dir_all(&dir)?;
                        for f in &files {
                            crate::safety::write_atomic(std::path::Path::new(&dir).join(&f.file), &f.bytes)?;
                        }
                        reply["dir"] = json!(dir);
                    }
                }
                reply
            }
            Command::Dado { ids, depth, width, clearance } => {
                let [cut, fit] = two(&ids, "dado")?;
                self.model.with_deadline(self.untrusted.then_some(crate::csg::SOLID_SECONDS), |m| crate::joinery::dado(m, cut, fit, depth, width, clearance))?
            }
            Command::Rabbet { ids, depth, width, clearance } => {
                let [cut, fit] = two(&ids, "rabbet")?;
                self.model.with_deadline(self.untrusted.then_some(crate::csg::SOLID_SECONDS), |m| crate::joinery::rabbet(m, cut, fit, depth, width, clearance))?
            }
            Command::Tenon { ids, length, thickness, shoulder, clearance } => {
                let [cut, fit] = two(&ids, "tenon")?;
                self.model.with_deadline(self.untrusted.then_some(crate::csg::SOLID_SECONDS), |m| crate::joinery::tenon(m, cut, fit, length, thickness, shoulder, clearance))?
            }
            other => unreachable!("command routed to the wrong group: {other:?}"),
        })
    }

    fn cut_plan(&self, ids: &[Id], kerf: Option<f64>, trim: Option<f64>) -> Result<crate::cutplan::Plan> {
        let mut stock = self.model.cut_stock.clone();
        stock.kerf = kerf.unwrap_or(stock.kerf);
        stock.trim = trim.unwrap_or(stock.trim);
        crate::cutplan::plan(&self.model, ids, &stock)
    }
}
