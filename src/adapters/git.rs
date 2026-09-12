//! Git, driven through `git(1)`.
//!
//! Every invocation runs with a fixed identity, fixed timestamps, UTC and the
//! host's global and system configuration switched off, so the same seed
//! produces the same commit object ids on any machine. Without that, "the
//! repository after 40 commits" would not be a reproducible artefact.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::dataset::gitgen::{BASE_COMMIT_TIME, IDENTITY_EMAIL, IDENTITY_NAME};
use crate::util::proc::Run;

/// A configured `git` command line rooted at one repository.
#[derive(Debug, Clone)]
pub struct GitCli {
    pub bin: PathBuf,
    /// Working directory (`-C`) for every invocation.
    pub repo: PathBuf,
    pub timeout: Duration,
}

impl GitCli {
    /// The base command: `-C repo`, deterministic identity and environment.
    pub fn git(&self) -> Run {
        Run::new(&self.bin)
            .arg("-C")
            .arg(&self.repo)
            .timeout(self.timeout)
            // Never read the host's configuration: it can change hooks,
            // signing, packing and line endings.
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", IDENTITY_NAME)
            .env("GIT_AUTHOR_EMAIL", IDENTITY_EMAIL)
            .env("GIT_COMMITTER_NAME", IDENTITY_NAME)
            .env("GIT_COMMITTER_EMAIL", IDENTITY_EMAIL)
            .env("GIT_AUTHOR_DATE", stamp(BASE_COMMIT_TIME))
            .env("GIT_COMMITTER_DATE", stamp(BASE_COMMIT_TIME))
            .env("TZ", "UTC")
            .env("LC_ALL", "C")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
    }

    /// The base command with both timestamps pinned to `unix_secs`.
    pub fn at(&self, unix_secs: i64) -> Run {
        self.git()
            .env("GIT_AUTHOR_DATE", stamp(unix_secs))
            .env("GIT_COMMITTER_DATE", stamp(unix_secs))
    }

    /// `git init`, with the settings the comparison depends on.
    pub fn init(&self, bare: bool) -> Vec<Run> {
        let mut init = Run::new(&self.bin)
            .arg("init")
            .arg("--quiet")
            .args(["--initial-branch", "main"])
            .timeout(self.timeout)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        if bare {
            init = init.arg("--bare");
        }
        let mut cmds = vec![init.arg(&self.repo)];
        for (k, v) in [
            ("user.name", IDENTITY_NAME),
            ("user.email", IDENTITY_EMAIL),
            ("commit.gpgsign", "false"),
            ("tag.gpgsign", "false"),
            ("core.autocrlf", "false"),
            ("core.fsmonitor", "false"),
            ("gc.auto", "0"),
            ("maintenance.auto", "false"),
            ("advice.detachedHead", "false"),
            ("uploadpack.allowFilter", "true"),
            ("receive.denyCurrentBranch", "ignore"),
        ] {
            cmds.push(self.git().args(["config", k, v]));
        }
        cmds
    }

    /// `git add -A`: stage the whole working tree.
    pub fn add_all(&self) -> Run {
        self.git().args(["add", "-A"])
    }

    /// `git commit`, allowing an empty commit so a state that changes nothing
    /// still produces one.
    pub fn commit(&self, subject: &str, unix_secs: i64) -> Run {
        self.at(unix_secs)
            .args(["commit", "--quiet", "--allow-empty", "-m", subject])
    }

    /// `git switch -c`: start a branch at HEAD.
    pub fn branch_create(&self, name: &str) -> Run {
        self.git().args(["switch", "--quiet", "-c", name])
    }

    /// `git switch`: move to an existing branch.
    pub fn switch(&self, name: &str) -> Run {
        self.git().args(["switch", "--quiet", name])
    }

    /// `git checkout` of an arbitrary revision into the working tree.
    pub fn checkout(&self, rev: &str) -> Run {
        self.git().args(["checkout", "--quiet", "--force", rev])
    }

    /// Merges `branch` recording both parents. `-s ours` keeps the current
    /// tree; the caller writes the merged tree itself, so the merge can never
    /// stop on a conflict and the history stays deterministic.
    pub fn merge_ours(&self, branch: &str, unix_secs: i64) -> Run {
        self.at(unix_secs).args([
            "merge",
            "--quiet",
            "--no-ff",
            "-s",
            "ours",
            "--no-commit",
            branch,
        ])
    }

    /// Creates a tag, annotated or lightweight.
    pub fn tag(&self, name: &str, annotated: bool, unix_secs: i64) -> Run {
        let r = self.at(unix_secs).arg("tag");
        if annotated {
            r.args(["-a", "-m", "release"]).arg(name)
        } else {
            r.arg(name)
        }
    }

    /// `git gc --prune=now --aggressive=false`: repack and drop unreachable
    /// objects immediately.
    pub fn gc(&self) -> Run {
        self.git().args(["gc", "--quiet", "--prune=now"])
    }

    /// `git fsck`: the integrity check.
    pub fn fsck(&self) -> Run {
        self.git()
            .args(["fsck", "--full", "--strict", "--no-progress"])
    }

    /// Full recursive listing of a revision's tree.
    pub fn ls_tree(&self, rev: &str) -> Run {
        self.git().args(["ls-tree", "-r", "-l", "--full-tree", rev])
    }

    /// `git rev-parse`.
    pub fn rev_parse(&self, rev: &str) -> Run {
        self.git().args(["rev-parse", rev])
    }

    /// `git count-objects -v`: object and pack accounting.
    pub fn count_objects(&self) -> Run {
        self.git().args(["count-objects", "-v"])
    }

    /// Clones `source` into `dest`. Not run with `-C repo`: the destination
    /// does not exist yet.
    pub fn clone_to(&self, source: &str, dest: &Path, bare: bool) -> Run {
        let mut r = Run::new(&self.bin)
            .arg("clone")
            .arg("--quiet")
            .timeout(self.timeout)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null");
        if bare {
            r = r.arg("--bare");
        }
        r.arg(source).arg(dest)
    }

    /// `git fetch --all --tags --prune`.
    pub fn fetch(&self, remote: &str) -> Run {
        self.git()
            .args(["fetch", "--quiet", "--tags", "--prune", remote])
    }

    /// `git push` of every branch and tag.
    pub fn push(&self, remote: &str) -> Run {
        self.git().args(["push", "--quiet", "--all", remote])
    }

    /// `git push --tags`.
    pub fn push_tags(&self, remote: &str) -> Run {
        self.git().args(["push", "--quiet", "--tags", remote])
    }

    /// `git remote add`.
    pub fn remote_add(&self, name: &str, url: &str) -> Run {
        self.git().args(["remote", "add", name, url])
    }

    /// `git bundle create`: the documented way to publish a repository as a
    /// single file to storage that speaks no Git protocol.
    pub fn bundle_create(&self, bundle: &Path, revs: &[&str]) -> Run {
        self.git()
            .args(["bundle", "create", "--quiet"])
            .arg(bundle)
            .args(revs)
    }

    /// An incremental bundle carrying only what `basis` does not have.
    pub fn bundle_create_since(&self, bundle: &Path, basis: &str, revs: &[&str]) -> Run {
        let mut args: Vec<String> = vec!["bundle".into(), "create".into(), "--quiet".into()];
        let mut r = self.git().args(args.drain(..)).arg(bundle);
        for rev in revs {
            r = r.arg(format!("{basis}..{rev}"));
        }
        r
    }

    /// `git bundle verify`.
    pub fn bundle_verify(&self, bundle: &Path) -> Run {
        self.git().args(["bundle", "verify"]).arg(bundle)
    }

    /// `git update-server-info`: what makes a plain directory of objects
    /// usable as a dumb-HTTP publication.
    pub fn update_server_info(&self) -> Run {
        self.git().args(["update-server-info"])
    }
}

/// Git's timestamp form: seconds since the epoch, explicitly UTC.
pub fn stamp(unix_secs: i64) -> String {
    format!("{unix_secs} +0000")
}

/// Parses `git count-objects -v` into its key/value pairs.
pub fn parse_count_objects(text: &str) -> std::collections::BTreeMap<String, i64> {
    text.lines()
        .filter_map(|l| l.split_once(':'))
        .filter_map(|(k, v)| {
            v.trim()
                .parse::<i64>()
                .ok()
                .map(|v| (k.trim().to_string(), v))
        })
        .collect()
}

/// Parses `git ls-tree -r -l` into `(mode, path, size)` triples.
pub fn parse_ls_tree(text: &str) -> Vec<(String, String, Option<u64>)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((meta, path)) = line.split_once('\t') else {
            continue;
        };
        let fields: Vec<&str> = meta.split_whitespace().collect();
        if fields.len() < 3 {
            continue;
        }
        let size = fields.get(3).and_then(|s| s.parse().ok());
        out.push((fields[0].to_string(), path.to_string(), size));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cli() -> GitCli {
        GitCli {
            bin: PathBuf::from("/bin/git"),
            repo: PathBuf::from("/scratch/repo"),
            timeout: Duration::from_secs(60),
        }
    }

    #[test]
    fn every_invocation_pins_identity_and_ignores_host_configuration() {
        let r = cli().git();
        // The environment is not part of display(), so assert by running a
        // command that echoes it back would need a shell; instead check the
        // builder records the removals and overrides by constructing a second
        // command and comparing debug output.
        let dbg = format!("{r:?}");
        assert!(dbg.contains("GIT_CONFIG_GLOBAL"), "{dbg}");
        assert!(dbg.contains("GIT_CONFIG_SYSTEM"), "{dbg}");
        assert!(dbg.contains("GIT_AUTHOR_DATE"), "{dbg}");
        assert!(dbg.contains("GIT_COMMITTER_DATE"), "{dbg}");
        assert!(dbg.contains("GIT_DIR"), "{dbg}");
    }

    #[test]
    fn timestamps_are_explicit_and_utc() {
        assert_eq!(stamp(1_700_000_000), "1700000000 +0000");
        let dbg = format!("{:?}", cli().at(42));
        assert!(dbg.contains("42 +0000"), "{dbg}");
    }

    #[test]
    fn init_disables_automatic_maintenance_that_would_pollute_measurements() {
        let cmds: Vec<String> = cli().init(false).iter().map(|c| c.display()).collect();
        let all = cmds.join("\n");
        assert!(all.contains("config gc.auto 0"), "{all}");
        assert!(all.contains("config maintenance.auto false"), "{all}");
        assert!(all.contains("config commit.gpgsign false"), "{all}");
        assert!(all.contains("--initial-branch main"), "{all}");
    }

    #[test]
    fn a_merge_cannot_stop_on_a_conflict() {
        let cmd = cli().merge_ours("feature/a", 7).display();
        assert!(cmd.contains("-s ours"), "{cmd}");
        assert!(cmd.contains("--no-ff"), "{cmd}");
        assert!(cmd.contains("--no-commit"), "{cmd}");
    }

    #[test]
    fn an_incremental_bundle_names_the_basis() {
        let cmd = cli()
            .bundle_create_since(Path::new("/tmp/b.bundle"), "v1.0", &["main", "feature/b"])
            .display();
        assert!(cmd.contains("v1.0..main"), "{cmd}");
        assert!(cmd.contains("v1.0..feature/b"), "{cmd}");
    }

    #[test]
    fn count_objects_output_is_parsed() {
        let text = "count: 12\nsize: 48\nin-pack: 300\npacks: 1\nsize-pack: 4096\nprune-packable: 0\ngarbage: 0\nsize-garbage: 0\n";
        let m = parse_count_objects(text);
        assert_eq!(m.get("in-pack"), Some(&300));
        assert_eq!(m.get("size-pack"), Some(&4096));
        assert_eq!(m.get("packs"), Some(&1));
    }

    #[test]
    fn ls_tree_output_is_parsed_including_sizes_and_modes() {
        let text = "100644 blob 0123456789abcdef0123456789abcdef01234567     123\tsrc/a.txt\n\
                    100755 blob fedcba9876543210fedcba9876543210fedcba98      45\tbin/run.sh\n\
                    120000 blob aaaabbbbccccddddaaaabbbbccccddddaaaabbbb       6\tlink\n";
        let rows = parse_ls_tree(text);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], ("100644".into(), "src/a.txt".into(), Some(123)));
        assert_eq!(rows[1].0, "100755");
        assert_eq!(rows[2].1, "link");
    }
}
