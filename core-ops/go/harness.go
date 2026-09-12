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
	// BytesKind names what Bytes counts, so a rate is never printed with a
	// denominator the reader has to guess. See the BytesKind constants.
	BytesKind string
	// Dims are the independent workload dimensions of this case, as
	// numbers. The workload string names the point; Dims is what a scaling
	// plot reads, and the report requires both cores to declare the same
	// dimensions for the same (op, workload).
	Dims Dims
	// PerRep re-runs Setup before every repetition. Destructive operations
	// (compaction, wipe, repair, removal) need a fresh copy each time; the
	// copy is made outside the measured interval.
	PerRep bool
	// Unstable marks a case whose accumulator legitimately differs from one
	// repetition to the next — a measurement of a randomised or
	// order-dependent result. Every other case must produce the same
	// checksum in every repetition; the report enforces that, which catches
	// a case that silently stopped doing its work.
	// CrossChecksum marks a case whose accumulator must additionally equal
	// the other core's. It is set wherever the two cores are specified to
	// produce the same bytes; it is not set where they legitimately differ
	// (zstd encodings, storage-engine internals, allocator addresses).
	Unstable      bool
	CrossChecksum bool
	Setup         func(*Env) any
	// Run performs Ops core operations and returns an accumulator built by
	// consuming their outputs. Consumption is deliberately cheap and
	// constant-time per call — a fixed-size output (a key, a header) is
	// folded whole, and a byte stream goes through sinkBytes, which is
	// noinline and reads only its length and its two ends. No O(payload)
	// pass is ever added inside a measured interval: that would turn an
	// encode measurement into encode-plus-a-second-hash. The full output
	// digests that prove the two cores agree are computed in the checks,
	// before any timing starts.
	Run  func(*Env, any) uint64
	Free func(*Env, any)
}

// What a case's Bytes field counts. A rate computed from Bytes is labelled
// with this, because "bytes per second" over a metadata scan and over a
// payload hash are not the same statement.
const (
	// BytesPayload: bytes of object payload the operation actually moved
	// through memory.
	BytesPayload = "payload"
	// BytesLogicalScanned: the logical size of the files a metadata walk
	// covered. The walk reads directory entries and inode metadata, not the
	// file bodies, so this is a measure of the tree it traversed and not of
	// the bandwidth it achieved.
	BytesLogicalScanned = "logical-scanned"
	// BytesIncluded: payload bytes of the subset an operation actually
	// included — the filtered tree, the changed files — as counted outside
	// the measured interval.
	BytesIncluded = "included"
	// BytesEncoded: bytes of encoded, possibly compressed wire output. The
	// two cores' encoders legitimately produce different sizes, so encoded
	// rates are reported per core and never divided by one another.
	BytesEncoded = "encoded"
)

// Dims are the independent workload dimensions of a case. Every field is a
// number or a small enumerated name, so the report can sweep one dimension
// with the others held fixed instead of pattern-matching workload strings.
type Dims struct {
	// ItemBytes is the size of one payload item; Items is how many of them
	// one measured call processes. ItemBytes * Items is the payload the
	// case moves.
	ItemBytes int64 `json:"item_bytes,omitempty"`
	Items     int64 `json:"items,omitempty"`
	// Content names how the payload bytes were generated: "random"
	// (incompressible), "text" (compressible), "duplicate" (every item
	// identical, so a content-addressed store sees one object), "empty",
	// "structured" (an encoded node rather than a blob) or
	// "partial-change" (a structured input with a minority of it rewritten).
	Content string `json:"content,omitempty"`
	// Entries, Depth and Width describe a tree: how many entries it holds,
	// how many directory levels deep the measured path goes, and how wide
	// the widest directory is.
	Entries int64  `json:"entries,omitempty"`
	Depth   int64  `json:"depth,omitempty"`
	Width   int64  `json:"width,omitempty"`
	Shape   string `json:"shape,omitempty"`
	// Files and Objects count the on-disk files and the stored objects an
	// operation covered, computed outside the measured interval.
	Files   int64 `json:"files,omitempty"`
	Objects int64 `json:"objects,omitempty"`
	// Workers is the worker count the operation was asked for; 0 means it
	// chose its own.
	Workers int64 `json:"workers,omitempty"`
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

// Encoding records how large one core's encoder made a given input. The two
// cores' compressors legitimately produce different sizes, so these are
// reported side by side and never divided by one another: a rate over
// encoded bytes would be a rate over two different denominators.
type Encoding struct {
	Op           string `json:"op"`
	Workload     string `json:"workload"`
	LogicalBytes int64  `json:"logical_bytes"`
	EncodedBytes int64  `json:"encoded_bytes"`
	Items        int64  `json:"items"`
}

// WireInput is one producing core's pack as this driver read it: the
// producer, the content hash of the file and its encoded size. Both drivers
// read the same files, so equal hashes here are the proof that a decode
// comparison was made on identical bytes.
type WireInput struct {
	Producer string `json:"producer"`
	SHA256   string `json:"sha256"`
	Bytes    int64  `json:"bytes"`
	Objects  int64  `json:"objects"`
}

// Sample is one repetition of one case.
type Sample struct {
	Group     string `json:"group"`
	Op        string `json:"op"`
	Workload  string `json:"workload"`
	Threads   int    `json:"threads"`
	Dims      Dims   `json:"dims"`
	Rep       int    `json:"rep"`
	Ops       int    `json:"ops"`
	Bytes     int64  `json:"bytes"`
	BytesKind string `json:"bytes_kind"`
	WallNs    int64  `json:"wall_ns"`
	CPUUserNs int64  `json:"cpu_user_ns"`
	CPUSysNs  int64  `json:"cpu_sys_ns"`
	// MaxRSSKiB is the whole process's high-water resident set at the end
	// of this repetition, from getrusage. It only ever grows over the
	// lifetime of the process, so it bounds the case rather than measuring
	// it; it is not a per-operation peak and the report says so.
	MaxRSSKiB int64 `json:"max_rss_kib"`
	// Checksum is the accumulator the measured calls produced, as described
	// on Case.Run. It is identical in every repetition of a stable case,
	// and identical to the other core's for a case marked CrossChecksum.
	//
	// It is evidence of output agreement and of the work not having been
	// elided — not, on its own, proof that the intended operation ran. The
	// proof of that is the correctness suite, which digests complete
	// outputs outside every measured interval.
	Checksum uint64 `json:"checksum"`
	// Stable and CrossChecksum repeat the case's policy so the report can
	// enforce it without a second table.
	Stable        bool   `json:"stable"`
	CrossChecksum bool   `json:"cross_checksum"`
	Status        string `json:"status"`
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
	// WireDir holds one wire pack per producing core, written by the
	// --emit-wire pass before either driver measures anything. Both drivers
	// read both files, so a decoder is always measured against byte-identical
	// input rather than against whatever its own encoder happened to emit.
	WireDir string
	Sink    uint64

	// RepBase and RepCount slice the profile's repetitions across the two
	// measurement passes run.sh makes. Pass one measures repetitions
	// [0, RepCount) with Go first; pass two measures the rest with Rust
	// first. Neither core is therefore systematically measured on a colder
	// or a busier machine than the other, and the merged document still has
	// exactly Profile.Reps repetitions of every case.
	RepBase  int
	RepCount int

	fx *Fixtures

	checks      []Check
	samples     []Sample
	counters    []Counter
	unsupported []Unsupported
	encodings   []Encoding
	wireInputs  []WireInput
}

// encoded records one encoder's output size for an input whose logical size
// is known. Always called outside a measured interval.
func (e *Env) encoded(op, workload string, logical, encoded, items int64) {
	e.encodings = append(e.encodings, Encoding{
		Op: op, Workload: workload,
		LogicalBytes: logical, EncodedBytes: encoded, Items: items,
	})
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

	dims := c.Dims
	dims.Workers = int64(c.Threads)

	for i := 0; i < e.Profile.Warmup; i++ {
		s := reset()
		e.Sink += blackBox(c.Run(e, s))
		release(s)
	}
	for rep := e.RepBase; rep < e.RepBase+e.RepCount; rep++ {
		s := reset()
		// The Go driver collects before every measured repetition so a case
		// is not charged for the previous case's garbage. The Rust driver
		// has no collector to run, so this is an asymmetry rather than a
		// symmetry: it is recorded in the document as
		// `environment.forced_gc_before_rep` and stated in the report.
		runtime.GC()
		var m0, m1 runtime.MemStats
		runtime.ReadMemStats(&m0)
		u0, s0 := cpuTime()
		t0 := time.Now()
		sum := c.Run(e, s)
		wall := time.Since(t0)
		u1, s1 := cpuTime()
		sum = blackBox(sum)
		runtime.ReadMemStats(&m1)
		e.Sink += sum
		e.samples = append(e.samples, Sample{
			Group: c.Group, Op: c.Op, Workload: c.Workload, Threads: c.Threads,
			Dims: dims,
			Rep:  rep, Ops: c.Ops, Bytes: c.Bytes, BytesKind: c.BytesKind,
			WallNs:    wall.Nanoseconds(),
			CPUUserNs: u1 - u0, CPUSysNs: s1 - s0,
			MaxRSSKiB: maxRSS(),
			Checksum:  sum,
			Stable:    !c.Unstable, CrossChecksum: c.CrossChecksum,
			Status: "ok",
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

// blackBox is the Go counterpart of Rust's std::hint::black_box: a function
// the compiler is forbidden to inline, so whatever is passed to it has to be
// computed and materialised. Every measured case's accumulator goes through
// it on the way out of the timed region, and the tiny accessor and codec
// cases route their individual results through it too.
//
//go:noinline
func blackBox[T any](v T) T { return v }

// sinkBytes consumes a byte-stream output in constant time. It is noinline,
// so the caller has to produce a real slice pointing at real memory, and it
// touches the length and both ends of that memory — enough that the buffer
// cannot be optimised away, and cheap enough that an encode or decode
// measurement stays a measurement of encode or decode.
//
// Hashing the whole output would be the stronger statement, and it is made:
// by the correctness checks, which run to completion before the first
// timing is taken.
//
//go:noinline
func sinkBytes(acc uint64, b []byte) uint64 {
	acc = foldU64(acc, uint64(len(b)))
	if len(b) > 0 {
		acc = foldU64(acc, uint64(b[0]))
		acc = foldU64(acc, uint64(b[len(b)-1]))
	}
	return acc
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
	Encodings   []Encoding    `json:"encodings"`
	WireInputs  []WireInput   `json:"wire_inputs"`
	Blackhole   uint64        `json:"blackhole"`
}

func (e *Env) write(path string, id Identity, env Environment, started, finished time.Time) error {
	r := report{
		Schema: sampleSchema, Core: "go", Profile: e.Profile.Name,
		Config: e.Profile, Identity: id, Environment: env,
		StartedAt:  started.UTC().Format(time.RFC3339Nano),
		FinishedAt: finished.UTC().Format(time.RFC3339Nano),
		Checks:     e.checks, Samples: e.samples, Counters: e.counters,
		Unsupported: e.unsupported, Encodings: e.encodings,
		WireInputs: e.wireInputs, Blackhole: e.Sink,
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
	if r.Encodings == nil {
		r.Encodings = []Encoding{}
	}
	if r.WireInputs == nil {
		r.WireInputs = []WireInput{}
	}
	b, err := json.MarshalIndent(r, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, append(b, '\n'), 0o644)
}

const sampleSchema = "amber-core-ops/samples/1"
