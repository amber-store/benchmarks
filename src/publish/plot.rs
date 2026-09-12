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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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

const WIDTH: f64 = 980.0;
const LEFT: f64 = 220.0;
const RIGHT: f64 = 34.0;
const BAR_H: f64 = 15.0;
const BAR_GAP: f64 = 2.0;
const GROUP_GAP: f64 = 13.0;
const LINE: f64 = 15.0;

/// A colour-blind-safe qualitative palette, assigned in series order so the
/// same chart always looks the same.
const PALETTE: [&str; 8] = [
    "#4477aa", "#ee6677", "#228833", "#ccbb44", "#66ccee", "#aa3377", "#bbbbbb", "#000000",
];

impl BarChart {
    fn colour(&self, series: usize) -> &'static str {
        PALETTE[series % PALETTE.len()]
    }

    /// The largest value any bar encodes, or `None` when nothing was
    /// measured at all.
    fn scale_max(&self) -> Option<f64> {
        let mut max: Option<f64> = None;
        for row in &self.cells {
            for c in row {
                for v in c.value.iter().chain(c.high.iter()) {
                    if v.is_finite() && (max.is_none() || *v > max.unwrap()) {
                        max = Some(*v);
                    }
                }
            }
        }
        max.filter(|m| *m > 0.0)
    }

    /// Renders the chart. Returns `None` when there is nothing to draw,
    /// which is a fact about the run and not an empty picture to commit.
    pub fn render(&self) -> Option<String> {
        let max = self.scale_max()?;
        let plot_w = WIDTH - LEFT - RIGHT;
        let head = 34.0 + LINE * (self.subtitle.len() as f64) + 26.0;
        let legend_h = LINE + 12.0;
        let group_h = |n: usize| n as f64 * (BAR_H + BAR_GAP) + GROUP_GAP;
        let body_h: f64 = self
            .categories
            .iter()
            .enumerate()
            .map(|(i, _)| group_h(self.cells[i].len()))
            .sum();
        let foot_h = LINE * (self.footer.len() as f64) + 16.0;
        let height = head + legend_h + body_h + 24.0 + foot_h;

        let mut s = String::new();
        s.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{WIDTH:.0}\" \
             height=\"{height:.0}\" viewBox=\"0 0 {WIDTH:.0} {height:.0}\" \
             font-family=\"DejaVu Sans, Verdana, sans-serif\" role=\"img\">\n"
        ));
        s.push_str(&format!(
            "<title>{}</title>\n<rect width=\"{WIDTH:.0}\" height=\"{height:.0}\" \
             fill=\"#ffffff\"/>\n",
            esc(&self.title)
        ));

        // --- heading ------------------------------------------------------
        s.push_str(&format!(
            "<text x=\"14\" y=\"24\" font-size=\"16\" font-weight=\"bold\" \
             fill=\"#111111\">{}</text>\n",
            esc(&self.title)
        ));
        let mut y = 34.0 + LINE;
        for line in &self.subtitle {
            s.push_str(&format!(
                "<text x=\"14\" y=\"{y:.1}\" font-size=\"11\" fill=\"#333333\">{}</text>\n",
                esc(line)
            ));
            y += LINE;
        }

        // --- legend -------------------------------------------------------
        y += 6.0;
        let mut x = 14.0;
        for (i, name) in self.series.iter().enumerate() {
            s.push_str(&format!(
                "<rect x=\"{x:.1}\" y=\"{:.1}\" width=\"10\" height=\"10\" fill=\"{}\"/>\n",
                y - 9.0,
                self.colour(i)
            ));
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{y:.1}\" font-size=\"11\" fill=\"#111111\">{}</text>\n",
                x + 14.0,
                esc(name)
            ));
            x += 22.0 + 7.0 * name.chars().count() as f64;
        }
        y += LINE;

        // --- bars ---------------------------------------------------------
        let top = y;
        for (gi, category) in self.categories.iter().enumerate() {
            let row = &self.cells[gi];
            let gh = row.len() as f64 * (BAR_H + BAR_GAP);
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"11\" text-anchor=\"end\" \
                 fill=\"#111111\">{}</text>\n",
                LEFT - 8.0,
                y + gh / 2.0 + 3.0,
                esc(category)
            ));
            for (si, cell) in row.iter().enumerate() {
                let by = y + si as f64 * (BAR_H + BAR_GAP);
                match cell.value {
                    Some(v) if v.is_finite() && v >= 0.0 => {
                        let w = (v / max * plot_w).max(1.0);
                        s.push_str(&format!(
                            "<rect x=\"{LEFT:.1}\" y=\"{by:.1}\" width=\"{w:.2}\" \
                             height=\"{BAR_H:.1}\" fill=\"{}\"/>\n",
                            self.colour(si)
                        ));
                        // Where a label placed outside the bar has to start,
                        // so it never runs into the p95 tick.
                        let mut outside = LEFT + w;
                        if let Some(hi) = cell.high.filter(|h| *h > v && h.is_finite()) {
                            let hx = LEFT + (hi / max * plot_w);
                            outside = outside.max(hx);
                            s.push_str(&format!(
                                "<line x1=\"{:.2}\" y1=\"{:.1}\" x2=\"{:.2}\" y2=\"{:.1}\" \
                                 stroke=\"#555555\" stroke-width=\"1\"/>\n",
                                LEFT + w,
                                by + BAR_H / 2.0,
                                hx,
                                by + BAR_H / 2.0
                            ));
                            s.push_str(&format!(
                                "<line x1=\"{hx:.2}\" y1=\"{:.1}\" x2=\"{hx:.2}\" y2=\"{:.1}\" \
                                 stroke=\"#555555\" stroke-width=\"1\"/>\n",
                                by + 2.0,
                                by + BAR_H - 2.0
                            ));
                        }
                        let label = match cell.n {
                            Some(n) => format!("{} (n={n})", cell.label),
                            None => cell.label.clone(),
                        };
                        // Inside the bar when it is long enough to hold the
                        // text, outside it otherwise.
                        let text_w = 6.2 * label.chars().count() as f64;
                        let (tx, anchor, fill) = if w > text_w + 10.0 {
                            (LEFT + w - 5.0, "end", "#ffffff")
                        } else {
                            (outside + 5.0, "start", "#111111")
                        };
                        s.push_str(&format!(
                            "<text x=\"{tx:.2}\" y=\"{:.1}\" font-size=\"10\" \
                             text-anchor=\"{anchor}\" fill=\"{fill}\">{}</text>\n",
                            by + BAR_H - 4.0,
                            esc(&label)
                        ));
                    }
                    _ => {
                        // No measurement: say so where the bar would be.
                        s.push_str(&format!(
                            "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10\" \
                             fill=\"#777777\" font-style=\"italic\">{}</text>\n",
                            LEFT + 3.0,
                            by + BAR_H - 4.0,
                            esc(&format!("{} — {}", self.series[si], cell.label))
                        ));
                    }
                }
            }
            y += gh + GROUP_GAP;
        }
        // Baseline of the value axis.
        s.push_str(&format!(
            "<line x1=\"{LEFT:.1}\" y1=\"{:.1}\" x2=\"{LEFT:.1}\" y2=\"{:.1}\" \
             stroke=\"#999999\" stroke-width=\"1\"/>\n",
            top - 4.0,
            y - GROUP_GAP + 2.0
        ));

        // --- footer -------------------------------------------------------
        y += 10.0;
        for line in &self.footer {
            s.push_str(&format!(
                "<text x=\"14\" y=\"{y:.1}\" font-size=\"10\" fill=\"#444444\">{}</text>\n",
                esc(line)
            ));
            y += LINE;
        }
        s.push_str("</svg>\n");
        Some(s)
    }
}

/// XML text escaping. Everything written into an SVG goes through this.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn chart(cells: Vec<Vec<Cell>>) -> BarChart {
        BarChart {
            title: "tree/lifecycle — elapsed time".into(),
            subtitle: vec!["median of 3 repetitions, milliseconds".into()],
            footer: vec!["the page cache was not dropped".into()],
            categories: vec!["ingest_fresh".into()],
            series: vec!["amber-rust".into(), "git".into()],
            cells,
        }
    }

    #[test]
    fn escaping_covers_every_xml_metacharacter() {
        assert_eq!(esc("a<b>&\"'"), "a&lt;b&gt;&amp;&quot;&apos;");
        assert_eq!(esc("a\u{1}b"), "a b");
    }

    #[test]
    fn a_rendered_chart_is_well_formed_and_states_its_units() {
        let svg = chart(vec![vec![
            Cell::measured(120.0, Some(180.0), "120 ms", 3),
            Cell::measured(60.0, None, "60 ms", 3),
        ]])
        .render()
        .expect("something was measured");
        assert!(svg.starts_with("<svg "), "{svg}");
        assert!(svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains("milliseconds"), "the unit is stated");
        assert!(svg.contains("n=3"), "the sample count is stated");
        assert!(svg.contains("page cache"), "the cache policy is stated");
        assert_eq!(
            svg.matches("<rect").count(),
            2 + 2 + 1,
            "bars + legend + bg"
        );
    }

    /// The point of the whole module: an operation a backend cannot do is
    /// not a short bar.
    #[test]
    fn an_absent_value_draws_words_and_never_a_bar() {
        let svg = chart(vec![vec![
            Cell::measured(120.0, None, "120 ms", 3),
            Cell::absent("unsupported"),
        ]])
        .render()
        .unwrap();
        assert!(svg.contains("git — unsupported"), "{svg}");
        // One bar, plus the legend swatches and the background.
        assert_eq!(svg.matches("<rect").count(), 1 + 2 + 1, "{svg}");
    }

    #[test]
    fn a_chart_with_nothing_measured_renders_nothing_at_all() {
        assert!(
            chart(vec![vec![Cell::absent("failed"), Cell::absent("failed")]])
                .render()
                .is_none()
        );
        // And so does one whose only values are zero: a zero-wide bar chart
        // has no scale.
        assert!(
            chart(vec![vec![
                Cell::measured(0.0, None, "0", 1),
                Cell::measured(0.0, None, "0", 1)
            ]])
            .render()
            .is_none()
        );
    }

    #[test]
    fn bar_lengths_are_proportional_to_the_largest_value() {
        let svg = chart(vec![vec![
            Cell::measured(100.0, None, "100", 1),
            Cell::measured(50.0, None, "50", 1),
        ]])
        .render()
        .unwrap();
        let widths: Vec<f64> = svg
            .lines()
            .filter(|l| l.starts_with("<rect x=\"220"))
            .map(|l| {
                let w = l.split("width=\"").nth(1).unwrap();
                w.split('"').next().unwrap().parse().unwrap()
            })
            .collect();
        assert_eq!(widths.len(), 2, "{svg}");
        assert!((widths[0] / widths[1] - 2.0).abs() < 1e-6, "{widths:?}");
    }

    #[test]
    fn wrapping_never_splits_a_word_and_keeps_every_one() {
        let lines = wrap("alpha beta gamma delta", 11);
        assert_eq!(lines, vec!["alpha beta", "gamma delta"]);
        assert_eq!(wrap("", 10), Vec::<String>::new());
        assert_eq!(
            wrap("supercalifragilistic", 5),
            vec!["supercalifragilistic"]
        );
    }
}
