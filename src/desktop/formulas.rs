//! Formula compilation scheduling.
use super::*;

impl ReaderView {
    pub(super) fn compile_formulas(&mut self, cx: &mut Context<Self>) {
        let mut display = std::collections::HashSet::new();
        let mut inline = std::collections::HashSet::new();
        for b in &self.state.paper.blocks {
            if b.kind == "math" && !b.tex.is_empty() && !self.formulas.contains_key(&b.tex) {
                display.insert(b.tex.clone());
            }
            if b.kind != "math" {
                let text = self.state.markdown(b, true);
                for math in formula::math_spans(&text) {
                    if math.display {
                        if !self.formulas.contains_key(&math.tex) {
                            display.insert(math.tex);
                        }
                    } else if !self.inline_formulas.contains_key(&math.tex) {
                        inline.insert(math.tex);
                    }
                }
            }
        }
        if display.is_empty() && inline.is_empty() {
            return;
        }
        self.worker(cx, move |cancel, tx| {
            for tex in display {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = formula::layout(&tex, true).map_err(|e| format!("{e:#}"));
                let _ = tx.send(Event::Formula(tex, result));
            }
            for tex in inline {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let result = formula::layout(&tex, false).map_err(|e| format!("{e:#}"));
                let _ = tx.send(Event::InlineFormula(tex, result));
            }
            let _ = tx.send(Event::Status("公式已编译".into()));
        });
    }
}
