//! Static SVG charts for a recorded run.
//!
//! These are written once, committed, and read on a page that runs no
//! JavaScript — so every chart has to carry its own context. Each one states
//! its units, how many samples are behind each bar, the cache policy of the
//! run and the semantics a reader needs in order not to misread it.
//!
//! Two rules are enforced here rather than left to the caller:
//!
//! * A bar is only drawn for a value that was measured. An operation a
//!   backend cannot perform, or one that failed, is drawn as the word
//!   instead — never as a zero-length bar, which reads as "free".
//! * One chart covers one scenario and one metric. There is no way to ask
//!   this module for a bar of restic's backup next to a bar of Git's clone.

/// One bar: either a measured value or a stated reason there is none.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Cell {
    /// The value the bar length encodes. `None` draws `label` as text.
    pub value: Option<f64>,
    /// An upper marker drawn as a tick, e.g. p95. Ignored when `value` is
    /// `None` or when it equals the value.
    pub high: Option<f64>,
    /// What is written at the end of the bar, or in place of it.
    pub label: String,
    /// Sample count behind the value, written next to the label.
    pub n: Option<usize>,
}

impl Cell {
    pub fn measured(value: f64, high: Option<f64>, label: impl Into<String>, n: usize) -> Cell {
        Cell {
            value: Some(value),
            high,
            label: label.into(),
            n: Some(n),
        }
    }

    /// No value, and the reason in its place.
    pub fn absent(label: impl Into<String>) -> Cell {
        Cell {
            value: None,
            high: None,
            label: label.into(),
            n: None,
        }
    }
}

/// A horizontal grouped bar chart: one group per operation, one bar per
/// backend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BarChart {
    pub title: String,
    /// Lines under the title: what is plotted, in what unit.
    pub subtitle: Vec<String>,
    /// Lines under the plot: cache policy, semantics, sample-size caveats.
    pub footer: Vec<String>,
    /// Group labels, top to bottom.
    pub categories: Vec<String>,
    /// Series labels, in legend and bar order.
    pub series: Vec<String>,
    /// `cells[category][series]`.
    pub cells: Vec<Vec<Cell>>,
}

impl BarChart {
    /// Use the same pinned Matplotlib renderer as the exploration notebook.
    pub fn render(&self) -> Result<String, String> {
        let spec = serde_json::to_string(self).map_err(|e| e.to_string())?;
        let output = crate::util::proc::Run::new("python3")
            .args([
                "-c",
                include_str!("../../python/amber_bench_plot.py"),
                &spec,
            ])
            .timeout(std::time::Duration::from_secs(60))
            .ok()?;
        Ok(output.stdout_text())
    }
}

pub fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Control characters are not valid XML 1.0 content.
            c if (c as u32) < 0x20 && c != '\t' => out.push(' '),
            c => out.push(c),
        }
    }
    out
}

/// Wraps `text` to `width` characters, for footer lines.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}
