//! CSV renderings.
//!
//! `samples.csv` is the raw data: one row per operation per repetition,
//! including the ones that were unsupported or failed, so nothing an analysis
//! might need has been filtered out before it gets there. Empty cells mean
//! "not measured"; they never mean zero.

use crate::metrics::{OpStatus, Phase};

use super::Report;

/// Escapes a field for RFC 4180 CSV.
fn field(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn num<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|x| x.to_string()).unwrap_or_default()
}

fn status(s: &OpStatus) -> &'static str {
    match s {
        OpStatus::Ok => "ok",
        OpStatus::Unsupported => "unsupported",
        OpStatus::Failed => "failed",
    }
}

fn phase(p: Phase) -> &'static str {
    match p {
        Phase::Setup => "setup",
        Phase::Measured => "measured",
        Phase::Verify => "verify",
    }
}

/// One row per operation per repetition.
///
/// `rep_valid` says whether the repetition the sample came from was healthy.
/// An analysis that wants only comparable numbers filters on it; one that is
/// diagnosing a failure keeps everything.
pub fn samples(r: &Report) -> String {
    let mut out = String::from(
        "run_id,group,scenario,backend,rep,rep_valid,order_index,op,phase,status,reason,\
         wall_ns,cpu_user_ns,cpu_sys_ns,max_rss_bytes,logical_bytes,\
         throughput_bytes_per_s,store_apparent_bytes,store_allocated_bytes,\
         reclaimed_bytes\n",
    );
    for run in &r.runs {
        let valid = run.healthy();
        for op in &run.ops {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                field(&r.run_id),
                field(&run.group),
                field(&run.scenario),
                field(&run.backend),
                run.rep,
                valid,
                run.order_index,
                field(&op.name),
                phase(op.phase),
                status(&op.status),
                field(op.reason.as_deref().unwrap_or("")),
                num(op.wall_ns),
                num(op.cpu_user_ns),
                num(op.cpu_sys_ns),
                num(op.max_rss_bytes),
                num(op.logical_bytes),
                num(op.throughput_bytes_per_s()),
                num(op.store_after.map(|s| s.apparent_bytes)),
                num(op.store_after.map(|s| s.allocated_bytes)),
                num(op.reclaimed_bytes),
            ));
        }
    }
    out
}

/// One row per aggregated comparison.
///
/// Every row carries the run's own validity verdict, so a spreadsheet that
/// only ever sees this file cannot present the numbers of a failed run as a
/// result.
pub fn summary(r: &Report) -> String {
    let valid = r.summary_valid();
    let mut out = String::from(
        "run_valid,group,scenario,backend,op,ok,excluded_invalid_samples,\
         unsupported,failed,reason,metric,unit,n,\
         min,median,p95,max,mean,stddev,cv,method\n",
    );
    for row in &r.summary {
        let metrics: [(&str, &Option<crate::stats::Summary>); 8] = [
            ("wall_ns", &row.wall_ns),
            ("cpu_ns", &row.cpu_ns),
            ("max_rss_bytes", &row.max_rss_bytes),
            ("throughput_bytes_per_s", &row.throughput_bytes_per_s),
            ("logical_bytes", &row.logical_bytes),
            ("store_apparent_bytes", &row.store_apparent_bytes),
            ("store_allocated_bytes", &row.store_allocated_bytes),
            ("reclaimed_bytes", &row.reclaimed_bytes),
        ];
        let mut any = false;
        for (name, s) in metrics {
            let Some(s) = s else { continue };
            any = true;
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                valid,
                field(&row.group),
                field(&row.scenario),
                field(&row.backend),
                field(&row.op),
                row.ok,
                row.excluded_invalid_samples,
                row.unsupported,
                row.failed,
                field(row.reason.as_deref().unwrap_or("")),
                name,
                field(&s.unit),
                s.n,
                s.min,
                s.median,
                s.p95,
                s.max,
                s.mean,
                num(s.stddev),
                num(s.cv),
                field(&s.method),
            ));
        }
        if !any {
            // An operation with no usable sample still gets a row, so it
            // cannot be mistaken for one that was never attempted.
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},,,,,,,,,,,\n",
                valid,
                field(&row.group),
                field(&row.scenario),
                field(&row.backend),
                field(&row.op),
                row.ok,
                row.excluded_invalid_samples,
                row.unsupported,
                row.failed,
                field(row.reason.as_deref().unwrap_or("")),
            ));
        }
    }
    out
}

/// One row per counter per operation per repetition.
pub fn counters(r: &Report) -> String {
    let mut out = String::from("group,scenario,backend,rep,op,counter,value\n");
    for run in &r.runs {
        for op in &run.ops {
            for (k, v) in &op.counters {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{}\n",
                    field(&run.group),
                    field(&run.scenario),
                    field(&run.backend),
                    run.rep,
                    field(&op.name),
                    field(k),
                    v
                ));
            }
        }
    }
    out
}

/// One row per correctness check.
pub fn verifications(r: &Report) -> String {
    let mut out =
        String::from("group,scenario,backend,rep,check,passed,detail,missing,extra,corrupt\n");
    for run in &r.runs {
        for v in &run.verifications {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{}\n",
                field(&run.group),
                field(&run.scenario),
                field(&run.backend),
                run.rep,
                field(&v.name),
                v.passed,
                field(&v.detail),
                v.missing.len(),
                v.extra.len(),
                v.corrupt.len(),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_with_commas_and_quotes_are_escaped() {
        assert_eq!(field("plain"), "plain");
        assert_eq!(field("a,b"), "\"a,b\"");
        assert_eq!(field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(field("line\nbreak"), "\"line\nbreak\"");
    }

    #[test]
    fn a_missing_number_renders_as_an_empty_cell_not_a_zero() {
        assert_eq!(num(None::<u64>), "");
        assert_eq!(num(Some(0u64)), "0");
    }

    #[test]
    fn statuses_and_phases_have_stable_names() {
        assert_eq!(status(&OpStatus::Unsupported), "unsupported");
        assert_eq!(phase(Phase::Measured), "measured");
    }
}
