package main

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"time"

	"github.com/amber-store/core/fstree"
	"github.com/amber-store/core/gc"
	"github.com/amber-store/core/key"
	"github.com/amber-store/core/packstore"
	"github.com/amber-store/core/refstore"
)

// gcState is a collector over its own copy of the gc fixture: a packstore
// holding one referenced tree plus enough unreferenced objects to fill whole
// sealed segments, and the refstore naming that tree.
type gcState struct {
	dir  string
	objs *packstore.Store
	refs *refstore.Store
	col  *gc.Collector
}

func (s *gcState) close() {
	if s.col != nil {
		_ = s.col.Close()
		s.col = nil
	}
	if s.objs != nil {
		_ = s.objs.Close()
		s.objs = nil
	}
	if s.refs != nil {
		_ = s.refs.Close()
		s.refs = nil
	}
	_ = os.RemoveAll(s.dir)
}

// gcOptions is the collector configuration both cores are given. The grace
// period is one nanosecond so the freshly written fixture segments are
// eligible; the garbage line is passed explicitly to Run, so the policy
// fallback (and its free-space sensitivity) never enters the measurement.
func gcOptions(p Profile) gc.Options {
	return gc.Options{Grace: time.Nanosecond, Garbage: gc.DefaultGarbage, Jobs: p.ThreadsMulti}
}

// openGC copies the fixture and opens the two stores; the collector itself is
// opened by the caller, so `gc.open` can be measured on its own.
func openGC(e *Env, name string) *gcState {
	dir := copiedDir(e, e.fx.GCTemplate, name)
	objs := mustV(packstore.Open(filepath.Join(dir, "objects"),
		packstore.WithSegmentSize(e.Profile.SegmentBytes), packstore.WithSync(false)))
	refs := mustV(refstore.Open(filepath.Join(dir, "refs"), false))
	return &gcState{dir: dir, objs: objs, refs: refs}
}

func openGCFull(e *Env, name string) *gcState {
	s := openGC(e, name)
	s.col = mustV(gc.Open(filepath.Join(s.dir, "closures"), s.objs, s.refs, gcOptions(e.Profile)))
	return s
}

func gcCases(e *Env) []Case {
	p := e.Profile
	var out []Case

	out = append(out,
		Case{
			Group: "gc", Op: "gc.open", Workload: "populated", Threads: p.ThreadsMulti, Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return openGC(en, "gc-open") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*gcState)
				col, err := gc.Open(filepath.Join(st.dir, "closures"), st.objs, st.refs, gcOptions(en.Profile))
				if err != nil {
					panic(err)
				}
				st.col = col
				return 1
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		Case{
			Group: "gc", Op: "gc.close", Workload: "idle", Threads: p.ThreadsMulti, Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return openGCFull(en, "gc-close") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*gcState)
				if err := st.col.Close(); err != nil {
					panic(err)
				}
				st.col = nil
				return 1
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		Case{
			Group: "gc", Op: "gc.prepare_ref", Workload: "tree-root/commit", Threads: p.ThreadsMulti,
			Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return openGCFull(en, "gc-prepare") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*gcState)
				commit, _, err := st.col.PrepareRef(en.fx.GCLiveRoot)
				if err != nil {
					panic(err)
				}
				commit()
				return 1
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		Case{
			Group: "gc", Op: "gc.prepare_ref", Workload: "tree-root/abort", Threads: p.ThreadsMulti,
			Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return openGCFull(en, "gc-prepare-abort") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*gcState)
				_, abort, err := st.col.PrepareRef(en.fx.GCLiveRoot)
				if err != nil {
					panic(err)
				}
				abort()
				return 1
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		Case{
			Group: "gc", Op: "gc.prepare_ref", Workload: "missing-root", Threads: p.ThreadsMulti,
			Ops: 64, PerRep: true,
			Setup: func(en *Env) any { return openGCFull(en, "gc-prepare-missing") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*gcState)
				var acc uint64
				for i := 0; i < 64; i++ {
					if _, _, err := st.col.PrepareRef(en.fx.WideRoot); err != nil {
						acc++
					}
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		Case{
			Group: "gc", Op: "gc.release_ref", Workload: "batch", Threads: p.ThreadsMulti, Ops: 4096,
			Setup: func(en *Env) any { return openGCFull(en, "gc-release") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*gcState)
				var acc uint64
				for i := 0; i < 4096; i++ {
					if err := st.col.ReleaseRef(en.fx.GCLiveRoot); err == nil {
						acc++
					}
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		Case{
			Group: "gc", Op: "gc.status", Workload: "mark+score", Threads: p.ThreadsMulti, Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return openGCFull(en, "gc-status") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*gcState)
				status, err := st.col.Status(context.Background())
				if err != nil {
					panic(err)
				}
				return uint64(status.Marked + len(status.Packs))
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		Case{
			Group: "gc", Op: "gc.why", Workload: "live-root", Threads: p.ThreadsMulti, Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return openGCFull(en, "gc-why") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*gcState)
				names, err := st.col.Why(en.fx.GCLiveRoot)
				if err != nil {
					panic(err)
				}
				return uint64(len(names))
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		Case{
			Group: "gc", Op: "gc.run", Workload: "reclaimable-packs", Threads: p.ThreadsMulti, Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return openGCFull(en, "gc-run") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*gcState)
				stats, err := st.col.Run(context.Background(), gc.DefaultGarbage)
				if err != nil {
					panic(err)
				}
				return uint64(stats.Marked) + uint64(len(stats.Reaped))
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		Case{
			Group: "gc", Op: "gc.run", Workload: "nothing-to-reclaim", Threads: p.ThreadsMulti, Ops: 1, PerRep: true,
			Setup: func(en *Env) any {
				st := openGCFull(en, "gc-run-clean")
				_, err := st.col.Run(context.Background(), gc.DefaultGarbage)
				must(err)
				return st
			},
			Run: func(_ *Env, s any) uint64 {
				st := s.(*gcState)
				stats, err := st.col.Run(context.Background(), gc.DefaultGarbage)
				if err != nil {
					panic(err)
				}
				return uint64(stats.Marked) + uint64(stats.Scored)
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		Case{
			Group: "gc", Op: "gc.wipe", Workload: "store-reset", Threads: p.ThreadsMulti, Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return openGCFull(en, "gc-wipe") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*gcState)
				err := st.col.Wipe(func() error {
					if err := st.objs.Wipe(); err != nil {
						return err
					}
					return st.refs.Wipe()
				})
				if err != nil {
					panic(err)
				}
				return 1
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
		// BeginWrite is exported by the Go collector only; the Rust
		// collector keeps the equivalent write gate internal to the
		// packstore (`Store::begin_write`, pub(super)). The coverage matrix
		// records it as Go-only, and the report never pairs it.
		Case{
			Group: "gc", Op: "gc.begin_write", Workload: "gate-span", Threads: p.ThreadsMulti, Ops: 4096,
			Setup: func(en *Env) any { return openGCFull(en, "gc-beginwrite") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*gcState)
				var acc uint64
				for i := 0; i < 4096; i++ {
					done := st.col.BeginWrite()
					done()
					acc++
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*gcState).close() },
		},
	)
	return out
}

func gcChecks(e *Env) {
	fx := e.fx
	st := openGCFull(e, "check-gc")
	defer st.close()

	// The live tree must be complete before the cycle, and every one of its
	// objects must still be there after it.
	live, err := fstree.ReachableKeys(fx.GCLiveRoot, st.objs.Get)
	if err != nil {
		e.fail("gc", "gc.run", "gc/reachable-before", err.Error())
		return
	}
	beforeSegs, _ := st.objs.Segments()
	beforeBytes := dirBytes(filepath.Join(st.dir, "objects"))

	status, err := st.col.Status(context.Background())
	e.want("gc", "gc.status", "gc/status-marks-live",
		err == nil && status.Refs == 1 && status.Marked == len(live) && len(status.Packs) == len(beforeSegs),
		fmt.Sprintf("status: refs=%d marked=%d (live=%d) packs=%d (sealed=%d) err=%v",
			status.Refs, status.Marked, len(live), len(status.Packs), len(beforeSegs), err),
		fmt.Sprintf("marked=%d", status.Marked))
	e.want("gc", "gc.status", "gc/status-sees-garbage",
		err == nil && status.GarbageBytes > 0 && status.LiveBytes > 0,
		fmt.Sprintf("status found live=%d garbage=%d bytes", status.LiveBytes, status.GarbageBytes),
		fmt.Sprintf("garbage>0=%v", status.GarbageBytes > 0))

	names, err := st.col.Why(fx.GCLiveRoot)
	e.want("gc", "gc.why", "gc/why-names-the-reference",
		err == nil && len(names) == 1 && names[0] == "gc/live",
		fmt.Sprintf("Why returned %v (%v)", names, err), digestStrings(names))
	names, err = st.col.Why(fx.StoreMissKeys[0])
	e.want("gc", "gc.why", "gc/why-unreferenced", err == nil && len(names) == 0,
		fmt.Sprintf("an unreferenced key is explained by %v (%v)", names, err), "0")

	// PrepareRef refuses a root whose objects are not in the store.
	_, _, err = st.col.PrepareRef(fx.WideRoot)
	e.want("gc", "gc.prepare_ref", "gc/prepare-rejects-incomplete", err != nil,
		"PrepareRef accepted a root that is not stored", "rejected")
	commit, _, err := st.col.PrepareRef(fx.GCLiveRoot)
	if err == nil {
		commit()
	}
	e.want("gc", "gc.prepare_ref", "gc/prepare-accepts-complete", err == nil,
		fmt.Sprintf("PrepareRef rejected the stored tree: %v", err), "accepted")
	e.want("gc", "gc.release_ref", "gc/release-is-a-noop", st.col.ReleaseRef(fx.GCLiveRoot) == nil,
		"ReleaseRef reported an error", "ok")

	// The cycle must reclaim real bytes and keep every referenced object.
	stats, err := st.col.Run(context.Background(), gc.DefaultGarbage)
	afterBytes := dirBytes(filepath.Join(st.dir, "objects"))
	// How many packs a cycle reaps depends on how the objects packed into
	// segments, which follows the compressed record sizes; the count is a
	// within-core anchor, the reclamation itself is the cross-core
	// statement.
	e.wantLocal("gc", "gc.run", "gc/run-reclaims",
		err == nil && len(stats.Reaped) > 0 && stats.FreedBytes > 0 && afterBytes < beforeBytes,
		fmt.Sprintf("cycle reaped %d packs, freed %d bytes, directory %d -> %d (%v)",
			len(stats.Reaped), stats.FreedBytes, beforeBytes, afterBytes, err),
		fmt.Sprintf("reaped=%d", len(stats.Reaped)))
	e.want("gc", "gc.run", "gc/run-marks-live", err == nil && stats.Marked == len(live),
		fmt.Sprintf("the cycle marked %d of %d live objects", stats.Marked, len(live)),
		fmt.Sprintf("marked=%d", stats.Marked))

	retained := 0
	for _, k := range live {
		if ok, _ := st.objs.Has(k); ok {
			retained++
		}
	}
	e.want("gc", "gc.run", "gc/run-retains-live", retained == len(live),
		fmt.Sprintf("%d of %d referenced objects survived the sweep", retained, len(live)),
		fmt.Sprintf("%d", retained))
	// The tree is still readable end to end after collection.
	_, cerr := fstree.CheckComplete(fx.GCLiveRoot, st.objs.Get, st.objs.Has, 1)
	e.want("gc", "gc.run", "gc/run-tree-still-complete", cerr == nil,
		fmt.Sprintf("the referenced tree is incomplete after collection: %v", cerr), "complete")
	e.want("gc", "gc.run", "gc/store-scrubs-clean-after-run",
		st.objs.Verify(context.Background()) == nil,
		"the store did not scrub clean after a collection cycle", "clean")

	// Repeated cycles reach a fixed point: sealing the active segment can
	// expose one more mostly-dead pack to the next cycle, so the guarantee
	// is that the sweep terminates, not that the very next cycle is idle.
	cycles, quiet := 0, false
	for i := 0; i < 5 && !quiet; i++ {
		s2, err2 := st.col.Run(context.Background(), gc.DefaultGarbage)
		if err2 != nil {
			e.fail("gc", "gc.run", "gc/reaches-a-fixed-point", err2.Error())
			break
		}
		cycles++
		quiet = len(s2.Reaped) == 0
	}
	e.want("gc", "gc.run", "gc/reaches-a-fixed-point", quiet,
		fmt.Sprintf("the collector still reaped packs after %d extra cycles", cycles), "quiet")
	retainedAfter := 0
	for _, k := range live {
		if ok, _ := st.objs.Has(k); ok {
			retainedAfter++
		}
	}
	e.want("gc", "gc.run", "gc/fixed-point-retains-live", retainedAfter == len(live),
		fmt.Sprintf("%d of %d referenced objects survived the repeated cycles", retainedAfter, len(live)),
		fmt.Sprintf("%d", retainedAfter))

	// Wipe resets both stores under the cycle lock.
	err = st.col.Wipe(func() error {
		if werr := st.objs.Wipe(); werr != nil {
			return werr
		}
		return st.refs.Wipe()
	})
	has, _ := st.objs.Has(live[0])
	recs, _ := st.refs.All()
	e.want("gc", "gc.wipe", "gc/wipe-resets", err == nil && !has && len(recs) == 0,
		fmt.Sprintf("wipe left %v objects / %d refs (%v)", has, len(recs), err), "empty")

	// BeginWrite is Go-only; record that the gate opens and closes.
	done := st.col.BeginWrite()
	done()
	done()
	// BeginWrite is exported by the Go collector only, so this check has no
	// counterpart to be compared against.
	e.passLocal("gc", "gc.begin_write", "gc/begin-write-release-is-idempotent", "ok")
}

var _ = key.Size
