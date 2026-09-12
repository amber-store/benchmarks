// Command amber-core-ops-go measures the exported operations of the Go
// Amber-Store core in process, through its library APIs.
//
// The driver is deliberately small: it owns fixture construction, a case
// registry and a timing loop, and writes one JSON document with the shared
// raw-sample schema (see ../SCHEMA.md). Every counterpart case in the Rust
// driver (../rust) carries the same op and workload names, so the report
// pairs them without any per-language table.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"runtime"
	"sort"
	"time"
)

// ---------------------------------------------------------------------------
// Case registry
// ---------------------------------------------------------------------------

// Case is one measured operation at one workload. Setup builds whatever the
// operation reads; only Run is timed. Ops is the number of core operations
// performed by one Run call — short operations are batched so the measured
// interval stays far above the clock's resolution — and Bytes is the payload
// the operation moved, where a throughput figure is meaningful (0 otherwise).
type Case struct {
	Group    string
	Op       string
	Workload string
	// Threads records the concurrency the operation was asked for: 1 for a
	// strictly single-threaded call, n for an explicit worker count, and 0
	// for an operation that picks its own parallelism (GOMAXPROCS here,
	// available_parallelism in Rust — both bounded by the CPU set the
	// driver runs under).
	Threads int
	Ops     int
	Bytes   int64
	// PerRep re-runs Setup before every repetition. Destructive operations
	// (compaction, wipe, repair, removal) need a fresh copy each time; the
	// copy is made outside the measured interval.
	PerRep bool
	Setup  func(*Env) any
	Run    func(*Env, any) uint64
	Free   func(*Env, any)
}

// Check is one correctness assertion. Digest, when set, is a canonical
// fingerprint of the operation's output; the report compares digests of the
// same check id across the two cores, which is how format agreement is
// established rather than assumed.
type Check struct {
	ID     string `json:"id"`
	Group  string `json:"group"`
	Op     string `json:"op"`
	Passed bool   `json:"passed"`
	Detail string `json:"detail,omitempty"`
	Digest string `json:"digest,omitempty"`
	// Comparable says whether the digest is supposed to be equal in the
	// other core. It is false where the two implementations are documented
	// to be interoperable without being byte-identical — zstd-compressed
	// record payloads are produced by klauspost/compress in Go and libzstd
	// in Rust, so records, segment bodies and wire packs containing them
	// differ. Such a digest is still recorded, as a within-core regression
	// anchor; the report never fails on a mismatch it was told not to
	// compare.
	Comparable bool `json:"comparable"`
}

// Unsupported records an exported operation that exists in one core only, or
// a workload the core cannot express. It never produces a sample: an absent
// operation must not be readable as an infinitely fast one.
type Unsupported struct {
	Op       string `json:"op"`
	Workload string `json:"workload,omitempty"`
	Reason   string `json:"reason"`
}

// Sample is one repetition of one case.
type Sample struct {
	Group     string `json:"group"`
	Op        string `json:"op"`
	Workload  string `json:"workload"`
	Threads   int    `json:"threads"`
	Rep       int    `json:"rep"`
	Ops       int    `json:"ops"`
	Bytes     int64  `json:"bytes"`
	WallNs    int64  `json:"wall_ns"`
	CPUUserNs int64  `json:"cpu_user_ns"`
	CPUSysNs  int64  `json:"cpu_sys_ns"`
	MaxRSSKiB int64  `json:"max_rss_kib"`
	Status    string `json:"status"`
}

// Counter is a runtime-specific number that has no comparable counterpart in
// the other core. The report prints these in their own table and never uses
// them in a paired comparison.
type Counter struct {
	Op       string `json:"op"`
	Workload string `json:"workload"`
	Rep      int    `json:"rep"`
	Name     string `json:"name"`
	Value    int64  `json:"value"`
}

// Env is everything the cases share: the profile, the scratch directory and
// the fixtures built once at startup.
type Env struct {
	Profile Profile
	Scratch string
	Xattrs  bool
	Sink    uint64

	fx *Fixtures

	checks      []Check
	samples     []Sample
	counters    []Counter
	unsupported []Unsupported
}

func (e *Env) check(c Check) { e.checks = append(e.checks, c) }

// pass records a passing check whose digest must equal the other core's.
func (e *Env) pass(group, op, id, digest string) {
	e.check(Check{ID: id, Group: group, Op: op, Passed: true, Digest: digest, Comparable: true})
}

// passLocal records a passing check whose digest is implementation-specific
// and must not be compared across cores.
func (e *Env) passLocal(group, op, id, digest string) {
	e.check(Check{ID: id, Group: group, Op: op, Passed: true, Digest: digest, Comparable: false})
}

// fail records a failing check. A failing check makes the whole run invalid;
// the report refuses to compare anything from an invalid run.
func (e *Env) fail(group, op, id, detail string) {
	e.check(Check{ID: id, Group: group, Op: op, Passed: false, Detail: detail})
}

// want records a check whose outcome is a boolean the caller computed.
func (e *Env) want(group, op, id string, ok bool, detail, digest string) {
	if ok {
		e.pass(group, op, id, digest)
		return
	}
	e.fail(group, op, id, detail)
}

// wantLocal is want for an implementation-specific digest.
func (e *Env) wantLocal(group, op, id string, ok bool, detail, digest string) {
	if ok {
		e.passLocal(group, op, id, digest)
		return
	}
	e.fail(group, op, id, detail)
}

func (e *Env) skip(op, workload, reason string) {
	e.unsupported = append(e.unsupported, Unsupported{Op: op, Workload: workload, Reason: reason})
}

// ---------------------------------------------------------------------------
// The timing loop
// ---------------------------------------------------------------------------

// run executes one case: Warmup unmeasured repetitions followed by Reps
// measured ones. Setup (and, for destructive cases, the per-repetition
// rebuild) stays outside every measured interval.
func (e *Env) run(c Case) {
	if c.Ops <= 0 {
		c.Ops = 1
	}
	var state any
	if !c.PerRep {
		state = c.Setup(e)
	}
	reset := func() any {
		if c.PerRep {
			return c.Setup(e)
		}
		return state
	}
	release := func(s any) {
		if c.PerRep && c.Free != nil {
			c.Free(e, s)
		}
	}

	for i := 0; i < e.Profile.Warmup; i++ {
		s := reset()
		e.Sink += c.Run(e, s)
		release(s)
	}
	for rep := 0; rep < e.Profile.Reps; rep++ {
		s := reset()
		runtime.GC()
		var m0, m1 runtime.MemStats
		runtime.ReadMemStats(&m0)
		u0, s0 := cpuTime()
		t0 := time.Now()
		e.Sink += c.Run(e, s)
		wall := time.Since(t0)
		u1, s1 := cpuTime()
		runtime.ReadMemStats(&m1)
		e.samples = append(e.samples, Sample{
			Group: c.Group, Op: c.Op, Workload: c.Workload, Threads: c.Threads,
			Rep: rep, Ops: c.Ops, Bytes: c.Bytes,
			WallNs:    wall.Nanoseconds(),
			CPUUserNs: u1 - u0, CPUSysNs: s1 - s0,
			MaxRSSKiB: maxRSS(),
			Status:    "ok",
		})
		e.counters = append(e.counters,
			Counter{c.Op, c.Workload, rep, "go.heap_alloc_bytes", int64(m1.TotalAlloc - m0.TotalAlloc)},
			Counter{c.Op, c.Workload, rep, "go.mallocs", int64(m1.Mallocs - m0.Mallocs)},
		)
		release(s)
	}
	if !c.PerRep && c.Free != nil {
		c.Free(e, state)
	}
}

// runAll executes the registry in a stable order so two runs of the same
// profile touch the machine in the same sequence.
func (e *Env) runAll(cases []Case) {
	sort.SliceStable(cases, func(i, j int) bool {
		if cases[i].Group != cases[j].Group {
			return cases[i].Group < cases[j].Group
		}
		if cases[i].Op != cases[j].Op {
			return cases[i].Op < cases[j].Op
		}
		return cases[i].Workload < cases[j].Workload
	})
	for _, c := range cases {
		started := time.Now()
		e.run(c)
		if os.Getenv("AMBER_CORE_OPS_TRACE") != "" {
			fmt.Fprintf(os.Stderr, "  %-34s %-28s %8.1f ms\n", c.Op, c.Workload,
				float64(time.Since(started).Nanoseconds())/1e6)
		}
	}
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------

type report struct {
	Schema      string        `json:"schema"`
	Core        string        `json:"core"`
	Profile     string        `json:"profile"`
	Config      Profile       `json:"config"`
	Identity    Identity      `json:"identity"`
	Environment Environment   `json:"environment"`
	StartedAt   string        `json:"started_at"`
	FinishedAt  string        `json:"finished_at"`
	Checks      []Check       `json:"checks"`
	Samples     []Sample      `json:"samples"`
	Counters    []Counter     `json:"counters"`
	Unsupported []Unsupported `json:"unsupported"`
	Blackhole   uint64        `json:"blackhole"`
}

func (e *Env) write(path string, id Identity, env Environment, started, finished time.Time) error {
	r := report{
		Schema: sampleSchema, Core: "go", Profile: e.Profile.Name,
		Config: e.Profile, Identity: id, Environment: env,
		StartedAt:  started.UTC().Format(time.RFC3339Nano),
		FinishedAt: finished.UTC().Format(time.RFC3339Nano),
		Checks:     e.checks, Samples: e.samples, Counters: e.counters,
		Unsupported: e.unsupported, Blackhole: e.Sink,
	}
	if r.Checks == nil {
		r.Checks = []Check{}
	}
	if r.Samples == nil {
		r.Samples = []Sample{}
	}
	if r.Counters == nil {
		r.Counters = []Counter{}
	}
	if r.Unsupported == nil {
		r.Unsupported = []Unsupported{}
	}
	b, err := json.MarshalIndent(r, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(b, '\n'), 0o644)
}

const sampleSchema = "amber-core-ops/samples/1"
