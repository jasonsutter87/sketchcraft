//! The woodworker pack: stock and prices, the cut plan, cutting diagrams and labels (`cutplan`).
//! Arms of `Engine::run`. These work on the whole model, never on a component being edited.

use super::*;

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
