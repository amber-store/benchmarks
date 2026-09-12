package main

import (
	"crypto/sha256"
	"encoding/hex"
	"flag"
	"fmt"
	"io"
	"os"
	"runtime"
	"runtime/debug"
	"syscall"
	"time"
)

// Profile fixes every dimension of a run: how many repetitions are measured
// and how big the fixtures are. Both cores are handed the same profile, so a
// paired comparison always compares the same amount of work.
type Profile struct {
	Name   string `json:"name"`
	Seed   uint64 `json:"seed"`
	Reps   int    `json:"reps"`
	Warmup int    `json:"warmup"`

	// Threads for the explicitly single- and multi-threaded variants of the
	// operations that take a worker count.
	ThreadsSingle int `json:"threads_single"`
	ThreadsMulti  int `json:"threads_multi"`

	// CorpusBytes is the size of the byte corpus the chunkers and the
	// content paths run over, per variant (compressible and random).
	CorpusBytes int64 `json:"corpus_bytes"`
	// TreeFiles, TreeWide and TreeDepth shape the on-disk fixture tree.
	TreeFiles int `json:"tree_files"`
	TreeWide  int `json:"tree_wide"`
	TreeDepth int `json:"tree_depth"`
	// SyntheticWide is the entry count of the in-memory directory the
	// lookup/list/collect cases walk.
	SyntheticWide int `json:"synthetic_wide"`
	// StoreObjects is the object count of the prebuilt packstore fixture.
	StoreObjects int `json:"store_objects"`
	// SegmentBytes is the rotation threshold of the multi-segment store
	// fixture: small enough that the fixture really has several sealed
	// segments to index, scrub, compact and reap.
	SegmentBytes int64 `json:"segment_bytes"`
	// RefRecords is the record count of the refstore fixture.
	RefRecords int `json:"ref_records"`
	// InboxPacks is the number of wire packs the inbox fixture drains.
	InboxPacks int `json:"inbox_packs"`
	// BatchOps scales the batch size of the short, per-call operations.
	BatchOps int `json:"batch_ops"`
}

func quickProfile(seed uint64) Profile {
	return Profile{
		Name: "quick", Seed: seed, Reps: 1, Warmup: 0,
		ThreadsSingle: 1, ThreadsMulti: 4,
		CorpusBytes: 4 << 20, TreeFiles: 120, TreeWide: 200, TreeDepth: 8,
		SyntheticWide: 2000, StoreObjects: 1500, SegmentBytes: 2 << 20,
		RefRecords: 200, InboxPacks: 8, BatchOps: 200,
	}
}

func standardProfile(seed uint64) Profile {
	return Profile{
		Name: "standard", Seed: seed, Reps: 7, Warmup: 2,
		ThreadsSingle: 1, ThreadsMulti: 8,
		CorpusBytes: 64 << 20, TreeFiles: 1200, TreeWide: 4000, TreeDepth: 24,
		SyntheticWide: 40000, StoreObjects: 30000, SegmentBytes: 16 << 20,
		RefRecords: 5000, InboxPacks: 48, BatchOps: 2000,
	}
}

// Identity is what the report needs to trust a paired result: which core
// source the driver was linked against, and the hash of the executable that
// produced the samples. The harness revision is recorded by run.sh, separately
// from these, so a harness edit is never mistaken for a core change.
type Identity struct {
	Core         string `json:"core"`
	CoreRepo     string `json:"core_repo"`
	CoreRevision string `json:"core_revision"`
	CoreDirty    bool   `json:"core_dirty"`
	CoreModule   string `json:"core_module"`
	CoreVersion  string `json:"core_version"`
	DriverPath   string `json:"driver_path"`
	DriverSHA256 string `json:"driver_sha256"`
	ToolchainID  string `json:"toolchain"`
	// The harness revision the driver source came from, kept apart from the
	// core identity above so a harness edit is never read as a core change.
	HarnessRevision string            `json:"harness_revision"`
	HarnessDirty    bool              `json:"harness_dirty"`
	BuildSettings   map[string]string `json:"build_settings"`
}

// Environment records the machine the samples were taken on.
type Environment struct {
	Host       string `json:"host"`
	OS         string `json:"os"`
	Arch       string `json:"arch"`
	NumCPU     int    `json:"num_cpu"`
	GOMAXPROCS int    `json:"gomaxprocs"`
	Scratch    string `json:"scratch"`
	ScratchFS  string `json:"scratch_fs"`
	Xattrs     bool   `json:"xattrs"`
}

func cpuTime() (userNs, sysNs int64) {
	var ru syscall.Rusage
	if err := syscall.Getrusage(syscall.RUSAGE_SELF, &ru); err != nil {
		return 0, 0
	}
	return ru.Utime.Nano(), ru.Stime.Nano()
}

func maxRSS() int64 {
	var ru syscall.Rusage
	if err := syscall.Getrusage(syscall.RUSAGE_SELF, &ru); err != nil {
		return 0
	}
	return int64(ru.Maxrss)
}

func fileSHA256(path string) string {
	f, err := os.Open(path)
	if err != nil {
		return ""
	}
	defer f.Close()
	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return ""
	}
	return hex.EncodeToString(h.Sum(nil))
}

func main() {
	var (
		profileName = flag.String("profile", "quick", "quick or standard")
		out         = flag.String("out", "", "path of the JSON sample document to write")
		scratch     = flag.String("scratch", "", "scratch directory for on-disk fixtures (ext4, not tmpfs)")
		seed        = flag.Uint64("seed", 0x5EEDC0DE, "fixture seed; both cores must be given the same one")
		coreRepo    = flag.String("core-repo", "", "recorded path of the core source the driver links against")
		coreRev     = flag.String("core-revision", "", "recorded revision of that core source")
		coreDirty   = flag.Bool("core-dirty", false, "recorded dirty state of that core source")
		xattrs      = flag.Bool("xattrs", false, "the scratch filesystem accepts user.* extended attributes")
		scratchFS   = flag.String("scratch-fs", "", "recorded filesystem type of the scratch directory")
		only        = flag.String("only", "", "run only the groups in this comma-separated list")
	)
	flag.Parse()

	if *out == "" || *scratch == "" {
		fmt.Fprintln(os.Stderr, "amber-core-ops-go: --out and --scratch are required")
		os.Exit(2)
	}

	var prof Profile
	switch *profileName {
	case "quick":
		prof = quickProfile(*seed)
	case "standard":
		prof = standardProfile(*seed)
	default:
		fmt.Fprintf(os.Stderr, "amber-core-ops-go: unknown profile %q\n", *profileName)
		os.Exit(2)
	}

	if err := os.MkdirAll(*scratch, 0o755); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}

	exe, _ := os.Executable()
	id := Identity{
		Core: "go", CoreRepo: *coreRepo, CoreRevision: *coreRev, CoreDirty: *coreDirty,
		CoreModule: coreModulePath, DriverPath: exe, DriverSHA256: fileSHA256(exe),
		ToolchainID: runtime.Version(), BuildSettings: map[string]string{},
	}
	if bi, ok := debug.ReadBuildInfo(); ok {
		for _, d := range bi.Deps {
			if d.Path == coreModulePath {
				id.CoreVersion = d.Version
			}
		}
		for _, s := range bi.Settings {
			switch s.Key {
			case "GOARCH", "GOOS", "GOAMD64", "-tags", "CGO_ENABLED":
				id.BuildSettings[s.Key] = s.Value
			case "vcs.revision":
				id.HarnessRevision = s.Value
			case "vcs.modified":
				id.HarnessDirty = s.Value == "true"
			}
		}
	}
	host, _ := os.Hostname()
	envInfo := Environment{
		Host: host, OS: runtime.GOOS, Arch: runtime.GOARCH,
		NumCPU: runtime.NumCPU(), GOMAXPROCS: runtime.GOMAXPROCS(0),
		Scratch: *scratch, ScratchFS: *scratchFS, Xattrs: *xattrs,
	}

	env := &Env{Profile: prof, Scratch: *scratch, Xattrs: *xattrs}
	started := time.Now()

	// Correctness first: the fixtures are built and validated before any
	// timing runs, and the report refuses to compare a run whose checks did
	// not all pass.
	env.fx = buildFixtures(env)

	cases := registry(env, splitList(*only))
	runChecks(env, splitList(*only))

	// Correctness precedes comparison: a failed check stops the run before
	// a single timing is taken, and the partial document is still written
	// so the failure is inspectable.
	finish := func(code int) {
		if env.fx.RO != nil {
			env.fx.RO.close()
		}
		if err := env.write(*out, id, envInfo, started, time.Now()); err != nil {
			fmt.Fprintln(os.Stderr, err)
			os.Exit(1)
		}
		failed := 0
		for _, c := range env.checks {
			if !c.Passed {
				failed++
				fmt.Fprintf(os.Stderr, "FAIL %s: %s\n", c.ID, c.Detail)
			}
		}
		fmt.Printf("go: %d samples, %d checks (%d failed), %d unsupported, blackhole %d\n",
			len(env.samples), len(env.checks), failed, len(env.unsupported), env.Sink)
		if failed > 0 || code != 0 {
			os.Exit(1)
		}
		os.Exit(0)
	}
	for _, c := range env.checks {
		if !c.Passed {
			finish(1)
		}
	}

	env.runAll(cases)
	finish(0)
}

const coreModulePath = "github.com/amber-store/core"

func splitList(s string) map[string]bool {
	if s == "" {
		return nil
	}
	m := map[string]bool{}
	start := 0
	for i := 0; i <= len(s); i++ {
		if i == len(s) || s[i] == ',' {
			if i > start {
				m[s[start:i]] = true
			}
			start = i + 1
		}
	}
	return m
}
