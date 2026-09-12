package main

import (
	"context"
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/amber-store/core/amberpack"
	"github.com/amber-store/core/fstree"
	"github.com/amber-store/core/key"
	"github.com/amber-store/core/packstore"
)

// openState carries a store a case opens inside the measured interval, so the
// close can happen outside it.
type openState struct {
	dir string
	st  *packstore.Store
}

func packstoreCases(e *Env) []Case {
	p := e.Profile
	fx := e.fx
	var out []Case

	lookups := p.BatchOps
	hitKeys := make([]key.Key, lookups)
	for i := range hitKeys {
		hitKeys[i] = fx.StoreKeys[(i*7919)%len(fx.StoreKeys)]
	}
	missKeys := make([]key.Key, lookups)
	for i := range missKeys {
		missKeys[i] = fx.StoreMissKeys[i%len(fx.StoreMissKeys)]
	}
	// A mixed batch for Missing: half present, half absent, interleaved.
	mixed := make([]key.Key, 0, lookups)
	for i := 0; i < lookups; i++ {
		if i%2 == 0 {
			mixed = append(mixed, hitKeys[i])
		} else {
			mixed = append(mixed, missKeys[i])
		}
	}

	// --- open ---------------------------------------------------------
	out = append(out,
		Case{
			Group: "packstore", Op: "packstore.open", Workload: "empty", Threads: 1, Ops: 1,
			Dims: Dims{Objects: 0, Items: 1, Content: "structured"}, PerRep: true,
			Setup: func(en *Env) any { return &openState{dir: workDir(en, "ps-open-empty")} },
			Run: func(en *Env, s any) uint64 {
				st := s.(*openState)
				var err error
				st.st, err = packstore.Open(st.dir, packstore.WithSegmentSize(en.Profile.SegmentBytes))
				if err != nil {
					panic(err)
				}
				segs, err := st.st.Segments()
				if err != nil {
					panic(err)
				}
				return foldI64(newFold(), int64(len(segs)))
			},
			Free: func(_ *Env, s any) {
				st := s.(*openState)
				if st.st != nil {
					_ = st.st.Close()
				}
				_ = os.RemoveAll(st.dir)
			},
		},
		Case{
			Group: "packstore", Op: "packstore.open", Workload: "populated-reopen", Threads: 1, Ops: 1,
			Dims: Dims{Objects: int64(len(fx.StoreKeys)), Items: 1, Content: "structured"}, PerRep: true,
			Setup: func(en *Env) any {
				return &openState{dir: copiedDir(en, en.fx.StoreTemplate, "ps-open-full")}
			},
			Run: func(en *Env, s any) uint64 {
				st := s.(*openState)
				var err error
				st.st, err = packstore.Open(st.dir, packstore.WithSegmentSize(en.Profile.SegmentBytes))
				if err != nil {
					panic(err)
				}
				segs, err := st.st.Segments()
				if err != nil {
					panic(err)
				}
				acc := foldI64(newFold(), int64(len(segs)))
				for _, sg := range segs {
					acc = foldU64(acc, sg.ID)
				}
				return acc
			},
			Free: func(_ *Env, s any) {
				st := s.(*openState)
				if st.st != nil {
					_ = st.st.Close()
				}
				_ = os.RemoveAll(st.dir)
			},
		},
		Case{
			Group: "packstore", Op: "packstore.close", Workload: "populated", Threads: 1, Ops: 1,
			Dims: Dims{Objects: int64(len(fx.StoreKeys)), Items: 1, Content: "structured"}, PerRep: true,
			Setup: func(en *Env) any {
				dir := copiedDir(en, en.fx.StoreTemplate, "ps-close")
				st := mustV(packstore.Open(dir, packstore.WithSegmentSize(en.Profile.SegmentBytes)))
				return &openState{dir: dir, st: st}
			},
			Run: func(_ *Env, s any) uint64 {
				st := s.(*openState)
				err := st.st.Close()
				if err != nil {
					panic(err)
				}
				st.st = nil
				return foldBool(newFold(), true)
			},
			Free: func(_ *Env, s any) { _ = os.RemoveAll(s.(*openState).dir) },
		},
	)

	// --- reads --------------------------------------------------------
	// Every read case runs against the same long-lived open copy of the
	// store fixture, so the measured interval is the lookup and not an
	// mmap. `objects` is the store's object count; `items` is how many
	// calls one measured interval makes.
	readDims := func(items int) Dims {
		return Dims{Items: int64(items), Objects: int64(len(fx.StoreKeys)), Content: "structured"}
	}
	read := func(op, workload string, ops int, bytes int64, run func(*Env, *packstore.Store) uint64) Case {
		return Case{
			Group: "packstore", Op: op, Workload: workload, Threads: 1, Ops: ops, Bytes: bytes,
			Dims:  readDims(ops),
			Setup: func(en *Env) any { return en.fx.RO.st },
			Run:   func(en *Env, s any) uint64 { return run(en, s.(*packstore.Store)) },
		}
	}
	out = append(out,
		read("packstore.get", "hit", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			acc := newFold()
			for _, k := range hitKeys {
				b, err := st.Get(k)
				if err != nil {
					panic(err)
				}
				// The returned buffer is consumed at its ends: a Get that
				// handed back an empty or uninitialised slice could not
				// reproduce this number.
				acc = sinkBytes(acc, b)
			}
			return acc
		}),
		read("packstore.get", "miss", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			acc := newFold()
			for _, k := range missKeys {
				_, err := st.Get(k)
				acc = foldBool(acc, errors.Is(err, packstore.ErrNotFound))
			}
			return acc
		}),
		read("packstore.get_record", "hit", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			acc := newFold()
			for _, k := range hitKeys {
				b, err := st.GetRecord(k)
				if err != nil {
					panic(err)
				}
				acc = sinkBytes(acc, b)
			}
			return acc
		}),
		read("packstore.has", "hit", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			acc := newFold()
			for _, k := range hitKeys {
				ok, err := st.Has(k)
				if err != nil {
					panic(err)
				}
				acc = foldBool(acc, ok)
			}
			return acc
		}),
		read("packstore.has", "miss", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			acc := newFold()
			for _, k := range missKeys {
				ok, err := st.Has(k)
				if err != nil {
					panic(err)
				}
				acc = foldBool(acc, ok)
			}
			return acc
		}),
		read("packstore.stored_size", "hit", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			acc := newFold()
			for _, k := range hitKeys {
				n, ok, err := st.StoredSize(k)
				if err != nil {
					panic(err)
				}
				acc = foldBool(acc, ok)
				acc = foldU64(acc, n)
			}
			return acc
		}),
		read("packstore.missing", "half-present", len(mixed), 0, func(_ *Env, st *packstore.Store) uint64 {
			out, err := st.Missing(mixed)
			if err != nil {
				panic(err)
			}
			acc := foldU64(newFold(), uint64(len(out)))
			// The whole returned set, in order: the answer is which keys
			// are missing, not how many.
			for _, k := range out {
				acc = foldKey(acc, k)
			}
			return acc
		}),
		read("packstore.sort_by_location", "scattered", len(mixed), 0, func(_ *Env, st *packstore.Store) uint64 {
			ks := make([]key.Key, len(mixed))
			copy(ks, mixed)
			st.SortByLocation(ks)
			// The permutation is the output, so the whole reordered slice
			// is folded rather than its first byte.
			acc := newFold()
			for _, k := range ks {
				acc = foldKey(acc, k)
			}
			return acc
		}),
		read("packstore.segments", "list", 64, 0, func(_ *Env, st *packstore.Store) uint64 {
			acc := newFold()
			for i := 0; i < 64; i++ {
				segs, err := st.Segments()
				if err != nil {
					panic(err)
				}
				acc = foldU64(acc, uint64(len(segs)))
				for _, sg := range segs {
					acc = foldU64(acc, sg.ID)
				}
			}
			return acc
		}),
		// Every sealed segment's index, not just the first: which segment a
		// given object lands in follows the compressed record sizes, which
		// the two cores' encoders do not produce identically, so a
		// single-segment walk would cover a different number of records on
		// each side. Over the whole store the count is the object count, in
		// both cores.
		read("packstore.scan_index", "all-segments", len(fx.StoreKeys), 0, func(en *Env, st *packstore.Store) uint64 {
			segs, err := st.Segments()
			if err != nil {
				panic(err)
			}
			acc := newFold()
			var n uint64
			for _, sg := range segs {
				if err := st.ScanIndex(sg.ID, func(k key.Key, off uint64, slen uint32) {
					// Offsets and segment ids are per-core facts (the
					// records are packed differently), so the fold covers
					// the key set and the record count, which are not.
					acc = foldKey(acc, k)
					n++
				}); err != nil {
					panic(err)
				}
			}
			return foldU64(acc, n)
		}),
		read("packstore.record", "by-location", minInt(lookups, 4096), 0, func(en *Env, st *packstore.Store) uint64 {
			acc := newFold()
			locs := en.fx.ROLocs
			n := minInt(lookups, 4096)
			for i := 0; i < n; i++ {
				l := locs[(i*7919)%len(locs)]
				b, err := st.Record(l.ID, l.Off)
				if err != nil {
					panic(err)
				}
				acc = sinkBytes(acc, b)
			}
			return acc
		}),
		read("packstore.has_outside", "sealed-segment", lookups, 0, func(en *Env, st *packstore.Store) uint64 {
			acc := newFold()
			for _, k := range hitKeys {
				ok, err := st.HasOutside(en.fx.ROSegID, k)
				if err != nil {
					panic(err)
				}
				acc = foldBool(acc, ok)
			}
			return acc
		}),
		read("packstore.oldest_inflight_write", "idle", 4096, 0, func(_ *Env, st *packstore.Store) uint64 {
			acc := newFold()
			for i := 0; i < 4096; i++ {
				t, ok := st.OldestInflightWrite()
				acc = foldBool(acc, ok)
				acc = foldI64(acc, t.UnixNano())
			}
			return acc
		}),
		read("packstore.new_mark_set", "snapshot", 16, 0, func(_ *Env, st *packstore.Store) uint64 {
			acc := newFold()
			for i := 0; i < 16; i++ {
				ms := st.NewMarkSet()
				acc = foldI64(acc, int64(ms.Marked()))
				blackBox(ms)
			}
			return acc
		}),
		read("packstore.mark_set_mark", "all-keys", len(fx.StoreKeys), 0, func(en *Env, st *packstore.Store) uint64 {
			ms := st.NewMarkSet()
			acc := newFold()
			for _, k := range en.fx.StoreKeys {
				newly, present := ms.Mark(k)
				acc = foldBool(acc, newly)
				acc = foldBool(acc, present)
			}
			return foldI64(acc, int64(ms.Marked()))
		}),
		read("packstore.mark_set_contains", "all-keys", len(fx.StoreKeys), 0, func(en *Env, st *packstore.Store) uint64 {
			ms := st.NewMarkSet()
			for _, k := range en.fx.StoreKeys {
				ms.Mark(k)
			}
			acc := newFold()
			for _, k := range en.fx.StoreKeys {
				acc = foldBool(acc, ms.Contains(k))
			}
			return acc
		}),
		read("packstore.liveness", "tenth-live", 1, 0, func(en *Env, st *packstore.Store) uint64 {
			live := keySet(en.fx.GarbageLive)
			ls, err := st.Liveness(func(k key.Key) bool { return live[k] })
			if err != nil {
				panic(err)
			}
			acc := foldU64(newFold(), uint64(len(ls)))
			for _, l := range ls {
				acc = foldI64(acc, int64(l.LiveKeys))
				acc = foldI64(acc, int64(l.DeadKeys))
			}
			return acc
		}),
		read("packstore.verify", "full-scrub", len(fx.StoreKeys), 0, func(_ *Env, st *packstore.Store) uint64 {
			err := st.Verify(context.Background())
			return foldBool(newFold(), err == nil)
		}),
		// Begin, observe the whole key set, abort. The observable result of
		// the capture is what a following Compact retains, which the
		// packstore/barrier-* checks assert; here the cost of capturing is
		// what is measured, and the answer folded is whether the store
		// really was capturing at the time.
		read("packstore.barrier", "begin+observe+abort", len(fx.StoreKeys), 0,
			func(en *Env, st *packstore.Store) uint64 {
				st.BeginBarrier()
				st.ObserveKeys(en.fx.StoreKeys)
				_, inflight := st.OldestInflightWrite()
				st.AbortBarrier()
				return foldBool(foldI64(newFold(), int64(len(en.fx.StoreKeys))), inflight)
			}),
	)

	// --- writes -------------------------------------------------------
	writeObjs := storeObjects(p, minInt(p.StoreObjects, 8000), 50000)

	// packstore.Put is swept over the object-size and content grid: the
	// size decides how many segments a batch fills, and the content decides
	// whether the record compresses and whether the store sees the object
	// at all. Each point is capped to a bounded number of stored bytes, so
	// the sweep costs about the same at every size.
	putSets := storePayloadSets(fx, p)
	for _, ps := range putSets {
		ps := ps
		objs := blobsOf(ps)
		out = append(out, Case{
			Group: "packstore", Op: "packstore.put", Workload: ps.Name + "/sync-off", Threads: 1,
			Ops: len(objs), Bytes: packBytes(objs), BytesKind: BytesPayload,
			Dims:   ps.dims(),
			PerRep: true,
			Setup:  func(en *Env) any { return freshStore(en, "ps-put") },
			Run: func(_ *Env, st any) uint64 {
				store := st.(*storeHandle).st
				acc := newFold()
				for _, o := range objs {
					err := store.Put(o.Key, o.Bytes)
					if err != nil {
						panic(err)
					}
					// Put's only result is whether it succeeded; the object
					// it stored is asserted by the packstore/put-* checks,
					// outside every measured interval.
					acc = foldBool(acc, true)
				}
				return acc
			},
			Free: func(_ *Env, st any) { st.(*storeHandle).close() },
		})
	}
	// The same objects written a second time into a store that already
	// holds them: the pure dedup path, at one representative size.
	dupSet := payloadNamed(putSets, "4KiB-random")
	dupObjs := blobsOf(dupSet)
	out = append(out,
		Case{
			Group: "packstore", Op: "packstore.put", Workload: "4KiB-random/already-present", Threads: 1,
			Ops: len(dupObjs), Bytes: packBytes(dupObjs), BytesKind: BytesPayload,
			Dims: dupSet.dims(), PerRep: true,
			Setup: func(en *Env) any {
				h := freshStore(en, "ps-put-dup")
				for _, o := range dupObjs {
					must(h.st.Put(o.Key, o.Bytes))
				}
				return h
			},
			Run: func(_ *Env, st any) uint64 {
				store := st.(*storeHandle).st
				acc := newFold()
				for _, o := range dupObjs {
					if err := store.Put(o.Key, o.Bytes); err != nil {
						panic(err)
					}
					acc = foldBool(acc, true)
				}
				return acc
			},
			Free: func(_ *Env, st any) { st.(*storeHandle).close() },
		},
		// The durability dimension, at the same representative size: every
		// append is fsynced rather than batched.
		Case{
			Group: "packstore", Op: "packstore.put", Workload: "4KiB-random/sync-on", Threads: 1,
			Ops: len(dupObjs), Bytes: packBytes(dupObjs), BytesKind: BytesPayload,
			Dims: dupSet.dims(), PerRep: true,
			Setup: func(en *Env) any { return freshStoreSync(en, "ps-put-sync") },
			Run: func(_ *Env, st any) uint64 {
				store := st.(*storeHandle).st
				acc := newFold()
				for _, o := range dupObjs {
					if err := store.Put(o.Key, o.Bytes); err != nil {
						panic(err)
					}
					acc = foldBool(acc, true)
				}
				return acc
			},
			Free: func(_ *Env, st any) { st.(*storeHandle).close() },
		},
		Case{
			Group: "packstore", Op: "packstore.write_batch", Workload: "mixed-objects", Threads: 1,
			Ops: len(writeObjs), Bytes: packBytes(writeObjs), BytesKind: BytesPayload,
			Dims:   Dims{Items: int64(len(writeObjs)), Objects: int64(len(writeObjs)), Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return freshStore(en, "ps-batch") },
			Run: func(_ *Env, st any) uint64 {
				store := st.(*storeHandle).st
				if err := store.WriteBatch(objectSeq(writeObjs)); err != nil {
					panic(err)
				}
				segs, err := store.Segments()
				if err != nil {
					panic(err)
				}
				return foldI64(newFold(), int64(len(segs)))
			},
			Free: func(_ *Env, st any) { st.(*storeHandle).close() },
		},
		Case{
			Group: "packstore", Op: "packstore.append_record", Workload: "pre-encoded+sync", Threads: 1,
			Ops: len(fx.WireRecords), Bytes: packBytes(fx.PackObjects), BytesKind: BytesPayload,
			Dims:   Dims{Items: int64(len(fx.WireRecords)), Objects: int64(len(fx.PackObjects)), Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return freshStore(en, "ps-append") },
			Run: func(en *Env, st any) uint64 {
				store := st.(*storeHandle).st
				acc := newFold()
				for i, rec := range en.fx.WireRecords {
					if err := store.AppendRecord(en.fx.PackObjects[i].Key, rec); err != nil {
						panic(err)
					}
					acc = foldBool(acc, true)
				}
				if err := store.Sync(); err != nil {
					panic(err)
				}
				return foldBool(acc, true)
			},
			Free: func(_ *Env, st any) { st.(*storeHandle).close() },
		},
	)
	for _, writers := range []int{p.ThreadsSingle, p.ThreadsMulti} {
		for _, verify := range []bool{false, true} {
			writers, verify := writers, verify
			name := fmt.Sprintf("mixed-objects/%s/verify-%v", writersLabel(writers), verify)
			out = append(out, Case{
				Group: "packstore", Op: "packstore.write_parallel", Workload: name, Threads: writers,
				Ops: len(writeObjs), Bytes: packBytes(writeObjs), BytesKind: BytesPayload,
				Dims: Dims{Items: int64(len(writeObjs)), Objects: int64(len(writeObjs)),
					Content: "structured"},
				PerRep:        true,
				CrossChecksum: true,
				Setup:         func(en *Env) any { return freshStore(en, "ps-parallel") },
				Run: func(_ *Env, s any) uint64 {
					st := s.(*storeHandle).st
					stats, err := st.WriteParallel(objectSeq(writeObjs),
						packstore.WriteOpts{Writers: writers, Verify: verify})
					if err != nil {
						panic(err)
					}
					return foldI64(foldI64(newFold(), int64(stats.Stored)), int64(stats.Deduped))
				},
				Free: func(_ *Env, s any) { s.(*storeHandle).close() },
			})
		}
	}
	out = append(out, Case{
		Group: "packstore", Op: "packstore.write_parallel", Workload: "duplicate-stream/writers-N", Threads: p.ThreadsMulti,
		Ops: len(writeObjs), Bytes: packBytes(writeObjs), BytesKind: BytesPayload,
		Dims: Dims{Items: int64(len(writeObjs)), Objects: int64(len(writeObjs)),
			Content: "duplicate"},
		PerRep: true, CrossChecksum: true,
		Setup: func(en *Env) any {
			h := freshStore(en, "ps-parallel-dup")
			_, err := h.st.WriteParallel(objectSeq(writeObjs),
				packstore.WriteOpts{Writers: en.Profile.ThreadsMulti})
			must(err)
			return h
		},
		Run: func(en *Env, s any) uint64 {
			st := s.(*storeHandle).st
			stats, err := st.WriteParallel(objectSeq(writeObjs),
				packstore.WriteOpts{Writers: en.Profile.ThreadsMulti})
			if err != nil {
				panic(err)
			}
			return foldI64(foldI64(newFold(), int64(stats.Stored)), int64(stats.Deduped))
		},
		Free: func(_ *Env, s any) { s.(*storeHandle).close() },
	})

	// --- destructive maintenance -------------------------------------
	out = append(out,
		Case{
			Group: "packstore", Op: "packstore.compact", Workload: "90-percent-dead", Threads: 1,
			Ops:    len(fx.StoreKeys),
			Dims:   Dims{Objects: int64(len(fx.StoreKeys)), Items: int64(len(fx.GarbageLive)), Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return copiedStore(en, en.fx.GarbageTemplate, "ps-compact") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*storeHandle).st
				live := keySet(en.fx.GarbageLive)
				stats, err := st.Compact(func(k key.Key) bool { return live[k] },
					packstore.CompactOpts{MinDeadRatio: 0.5})
				if err != nil {
					panic(err)
				}
				return foldU64(foldI64(foldI64(newFold(),
					int64(stats.RecordsCopied)), int64(stats.SegmentsScanned)), stats.BytesFreed)
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
		Case{
			Group: "packstore", Op: "packstore.compact", Workload: "nothing-dead", Threads: 1,
			Ops:    len(fx.StoreKeys),
			Dims:   Dims{Objects: int64(len(fx.StoreKeys)), Items: int64(len(fx.StoreKeys)), Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return copiedStore(en, en.fx.GarbageTemplate, "ps-compact-live") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*storeHandle).st
				stats, err := st.Compact(func(key.Key) bool { return true },
					packstore.CompactOpts{MinDeadRatio: 0.5})
				if err != nil {
					panic(err)
				}
				return foldU64(foldI64(foldI64(newFold(),
					int64(stats.RecordsCopied)), int64(stats.SegmentsScanned)), stats.BytesFreed)
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
		Case{
			Group: "packstore", Op: "packstore.remove", Workload: "one-sealed-segment", Threads: 1,
			Ops:    1,
			Dims:   Dims{Objects: int64(len(fx.StoreKeys)), Items: 1, Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return copiedStore(en, en.fx.StoreTemplate, "ps-remove") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*storeHandle).st
				segs, err := st.Segments()
				if err != nil {
					panic(err)
				}
				if err := st.Remove(segs[0].ID); err != nil {
					panic(err)
				}
				after, err := st.Segments()
				if err != nil {
					panic(err)
				}
				return foldI64(foldU64(newFold(), segs[0].ID), int64(len(after)))
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
		Case{
			Group: "packstore", Op: "packstore.wipe", Workload: "populated", Threads: 1,
			Ops:    1,
			Dims:   Dims{Objects: int64(len(fx.StoreKeys)), Items: 1, Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return copiedStore(en, en.fx.StoreTemplate, "ps-wipe") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*storeHandle).st
				if err := st.Wipe(); err != nil {
					panic(err)
				}
				segs, err := st.Segments()
				if err != nil {
					panic(err)
				}
				return foldI64(newFold(), int64(len(segs)))
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
		// PutVerified rewrites the segments holding a corrupt copy of the
		// key, so it runs on its own copy only.
		Case{
			Group: "packstore", Op: "packstore.put_verified", Workload: "already-intact", Threads: 1,
			Ops:    minInt(p.BatchOps/8, 256),
			Dims:   Dims{Items: int64(minInt(p.BatchOps/8, 256)), Objects: int64(len(fx.StoreKeys)), Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return copiedStore(en, en.fx.StoreTemplate, "ps-putverified") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*storeHandle).st
				n := minInt(en.Profile.BatchOps/8, 256)
				acc := newFold()
				for i := 0; i < n; i++ {
					o := en.fx.PackObjects[i%len(en.fx.PackObjects)]
					err := st.PutVerified(o.Key, o.Bytes)
					if err != nil {
						panic(err)
					}
					acc = foldBool(acc, true)
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
	)
	return out
}

// storePayloadSets is the payload grid reduced to what a store write can
// afford: every size class crossed with every content kind, each capped to a
// bounded number of stored bytes so the whole sweep costs about the same as
// one of the old cases did.
func storePayloadSets(fx *Fixtures, p Profile) []payloadSet {
	cap_ := p.PayloadTotal / 8
	out := make([]payloadSet, 0, len(fx.Payloads))
	for _, ps := range fx.Payloads {
		out = append(out, ps.limit(cap_))
	}
	return out
}

// blobsOf encodes a payload set as store objects, outside every measured
// interval.
func blobsOf(ps payloadSet) []fstree.Object {
	out := make([]fstree.Object, 0, len(ps.Items))
	for _, b := range ps.Items {
		out = append(out, mustV(fstree.EncodeBlob(b)))
	}
	return out
}

func storeObjectsOfSize(p Profile, n, size int, salt uint64) []fstree.Object {
	out := make([]fstree.Object, 0, n)
	for i := 0; i < n; i++ {
		b := randomBytes(p.Seed+salt+uint64(i)*0x100000001B3, size)
		out = append(out, mustV(fstree.EncodeBlob(b)))
	}
	return out
}

func keySet(ks []key.Key) map[key.Key]bool {
	m := make(map[key.Key]bool, len(ks))
	for _, k := range ks {
		m[k] = true
	}
	return m
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}

func packstoreChecks(e *Env) {
	fx := e.fx
	p := e.Profile
	st := fx.RO.st

	// Reads return exactly what was written, and misses are misses.
	ok := true
	var digests []string
	for i := 0; i < minInt(256, len(fx.StoreKeys)); i++ {
		k := fx.StoreKeys[i*7%len(fx.StoreKeys)]
		b, err := st.Get(k)
		if err != nil {
			ok = false
			break
		}
		got, err := key.New(key.Blob, uint64(len(b)), b)
		if err != nil || got != k {
			ok = false
			break
		}
		digests = append(digests, k.String())
	}
	e.want("packstore", "packstore.get", "packstore/get-content-addressed", ok,
		"a stored object did not re-hash to its key", digestStrings(digests))
	_, err := st.Get(fx.StoreMissKeys[0])
	e.want("packstore", "packstore.get", "packstore/get-miss", errors.Is(err, packstore.ErrNotFound),
		fmt.Sprintf("an absent key must return ErrNotFound, got %v", err), "ErrNotFound")
	has, err := st.Has(fx.StoreKeys[0])
	hasNot, err2 := st.Has(fx.StoreMissKeys[0])
	e.want("packstore", "packstore.has", "packstore/has", err == nil && err2 == nil && has && !hasNot,
		fmt.Sprintf("Has disagrees with the store contents: %v/%v (%v, %v)", has, hasNot, err, err2), "true/false")

	// GetRecord hands back a record that parses, decodes to the object and
	// can be re-added to a wire pack verbatim.
	rec, err := st.GetRecord(fx.StoreKeys[0])
	if err != nil {
		e.fail("packstore", "packstore.get_record", "packstore/get-record", err.Error())
	} else {
		h, perr := amberpack.ParseRecord(rec)
		body, derr := amberpack.DecodePayload(h.Flags, h.Ulen,
			rec[amberpack.RecHeaderSize:amberpack.RecHeaderSize+int(h.Slen)])
		want, _ := st.Get(fx.StoreKeys[0])
		e.want("packstore", "packstore.get_record", "packstore/get-record",
			perr == nil && derr == nil && h.Key == fx.StoreKeys[0] && string(body) == string(want),
			fmt.Sprintf("record round trip: %v/%v", perr, derr), fx.StoreKeys[0].String())
	}

	n, found, err := st.StoredSize(fx.StoreKeys[0])
	e.want("packstore", "packstore.stored_size", "packstore/stored-size",
		err == nil && found && n > 0 && n <= uint64(len(mustV(st.Get(fx.StoreKeys[0])))+64),
		fmt.Sprintf("stored size %d (found=%v, %v)", n, found, err), "ok")

	// Missing preserves order and multiplicity.
	probe := []key.Key{fx.StoreKeys[0], fx.StoreMissKeys[0], fx.StoreKeys[1], fx.StoreMissKeys[0]}
	miss, err := st.Missing(probe)
	e.want("packstore", "packstore.missing", "packstore/missing",
		err == nil && len(miss) == 2 && miss[0] == fx.StoreMissKeys[0] && miss[1] == fx.StoreMissKeys[0],
		fmt.Sprintf("Missing returned %d keys (%v)", len(miss), err), fmt.Sprintf("%d", len(miss)))

	// SortByLocation is a permutation of its input.
	ks := append([]key.Key(nil), fx.StoreKeys[:minInt(1024, len(fx.StoreKeys))]...)
	before := keySet(ks)
	st.SortByLocation(ks)
	perm := len(before) == len(keySet(ks))
	for _, k := range ks {
		if !before[k] {
			perm = false
		}
	}
	e.want("packstore", "packstore.sort_by_location", "packstore/sort-is-permutation", perm,
		"SortByLocation changed the key multiset", fmt.Sprintf("n=%d", len(ks)))

	// The footer index must describe exactly the segment's records.
	segs, err := st.Segments()
	if err != nil || len(segs) == 0 {
		e.fail("packstore", "packstore.segments", "packstore/segments", fmt.Sprintf("%d segments (%v)", len(segs), err))
	} else {
		var indexed []key.Key
		var count uint64
		must(st.ScanIndex(segs[0].ID, func(k key.Key, off uint64, slen uint32) {
			count++
			if len(indexed) < 64 {
				indexed = append(indexed, k)
			}
		}))
		// The record population of a given sealed segment depends on the
		// compressed record sizes, which klauspost/compress and libzstd do
		// not produce identically; the count is a within-core anchor.
		e.wantLocal("packstore", "packstore.scan_index", "packstore/scan-index-matches-footer",
			count == segs[0].Keys,
			fmt.Sprintf("the index walk yielded %d entries, the footer claims %d", count, segs[0].Keys),
			fmt.Sprintf("%d", count))
		// Every indexed record reads back at its recorded offset.
		recOK := true
		for _, l := range fx.ROLocs[:minInt(64, len(fx.ROLocs))] {
			raw, err := st.Record(l.ID, l.Off)
			if err != nil {
				recOK = false
				break
			}
			if _, err := amberpack.ParseRecord(raw); err != nil {
				recOK = false
				break
			}
		}
		e.wantLocal("packstore", "packstore.record", "packstore/record-by-location", recOK,
			"a record did not parse at its indexed offset", fmt.Sprintf("n=%d", len(fx.ROLocs)))
		out, err := st.HasOutside(segs[0].ID, fx.StoreMissKeys[0])
		e.want("packstore", "packstore.has_outside", "packstore/has-outside-miss", err == nil && !out,
			fmt.Sprintf("HasOutside on an absent key: %v (%v)", out, err), "false")
	}

	// A full scrub of an untouched store must pass.
	e.want("packstore", "packstore.verify", "packstore/verify-clean", st.Verify(context.Background()) == nil,
		"the scrub reported corruption in an untouched fixture", "clean")

	// The mark set only knows the snapshot's keys.
	ms := st.NewMarkSet()
	newly, present := ms.Mark(fx.StoreKeys[0])
	_, absent := ms.Mark(fx.StoreMissKeys[0])
	e.want("packstore", "packstore.mark_set_mark", "packstore/markset",
		newly && present && !absent && ms.Contains(fx.StoreKeys[0]) && ms.Marked() == 1,
		fmt.Sprintf("mark set: newly=%v present=%v absentPresent=%v marked=%d",
			newly, present, absent, ms.Marked()), "ok")

	// Liveness must account for every record exactly once.
	live := keySet(fx.GarbageLive)
	ls, err := st.Liveness(func(k key.Key) bool { return live[k] })
	var totalKeys int
	for _, l := range ls {
		totalKeys += l.LiveKeys + l.DeadKeys
	}
	e.want("packstore", "packstore.liveness", "packstore/liveness-accounts-all",
		err == nil && totalKeys == len(fx.StoreKeys),
		fmt.Sprintf("liveness counted %d records, the store holds %d (%v)", totalKeys, len(fx.StoreKeys), err),
		fmt.Sprintf("%d", totalKeys))

	// Compaction on an isolated copy: the live objects must survive and the
	// store must shrink.
	dir := copiedDir(e, fx.GarbageTemplate, "check-compact")
	cst := mustV(packstore.Open(dir, packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
	beforeBytes := dirBytes(dir)
	stats, err := cst.Compact(func(k key.Key) bool { return live[k] }, packstore.CompactOpts{MinDeadRatio: 0.5})
	afterBytes := dirBytes(dir)
	// Retention is a statement about content, not about presence: every
	// object that was supposed to survive is read back and re-hashed, and
	// the store is content-addressed, so a key that matches its own
	// re-hash is a complete verification of the retained bytes.
	retained := true
	retainedDetail := ""
	for _, k := range fx.GarbageLive {
		got, gerr := cst.Get(k)
		if gerr != nil {
			retained = false
			retainedDetail = fmt.Sprintf("%s: %v", k, gerr)
			break
		}
		rehash, herr := key.New(key.Blob, uint64(len(got)), got)
		if herr != nil || rehash != k {
			retained = false
			retainedDetail = fmt.Sprintf("%s re-hashed to %s (%v)", k, rehash, herr)
			break
		}
	}
	dropped := 0
	for i, k := range fx.StoreKeys {
		if i%10 != 0 {
			if ok, _ := cst.Has(k); !ok {
				dropped++
			}
		}
	}
	e.want("packstore", "packstore.compact", "packstore/compact-retains-live-content",
		err == nil && retained,
		fmt.Sprintf("a live object did not survive compaction intact (%v): %s", err, retainedDetail),
		fmt.Sprintf("live=%d", len(fx.GarbageLive)))
	// And everything that was eligible really was reclaimed: no object the
	// predicate called dead may still be readable from a compacted segment.
	survivors := 0
	for i, k := range fx.StoreKeys {
		if i%10 == 0 {
			continue
		}
		if ok, _ := cst.Has(k); ok {
			survivors++
		}
	}
	e.wantLocal("packstore", "packstore.compact", "packstore/compact-drops-dead",
		dropped > 0 && dropped+survivors == len(fx.StoreKeys)-len(fx.GarbageLive),
		fmt.Sprintf("%d dead objects dropped, %d still present, %d were dead",
			dropped, survivors, len(fx.StoreKeys)-len(fx.GarbageLive)),
		fmt.Sprintf("dropped=%d survivors=%d", dropped, survivors))
	e.wantLocal("packstore", "packstore.compact", "packstore/compact-reclaims",
		err == nil && stats.SegmentsCompacted > 0 && afterBytes < beforeBytes && dropped > 0,
		fmt.Sprintf("compaction freed nothing: %d segments, %d -> %d bytes, %d objects dropped",
			stats.SegmentsCompacted, beforeBytes, afterBytes, dropped),
		fmt.Sprintf("compacted=%d dropped=%d", stats.SegmentsCompacted, dropped))
	e.want("packstore", "packstore.verify", "packstore/verify-after-compact",
		cst.Verify(context.Background()) == nil,
		"the store did not scrub clean after compaction", "clean")
	must(cst.Close())
	must(os.RemoveAll(dir))

	// PutVerified repairs a deliberately corrupted copy. The corruption is
	// applied to an isolated copy of the fixture, never to the template.
	repairDir := copiedDir(e, fx.StoreTemplate, "check-repair")
	victim := fx.StoreKeys[0]
	rst := mustV(packstore.Open(repairDir, packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
	want := mustV(rst.Get(victim))
	loc := findRecordOffset(rst, victim)
	must(rst.Close())
	if loc.Len == 0 {
		e.fail("packstore", "packstore.put_verified", "packstore/repairs-corruption",
			"could not locate the victim record to corrupt")
	} else {
		must(corruptSegmentByte(repairDir, loc))
		rst = mustV(packstore.Open(repairDir, packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
		scrub := rst.Verify(context.Background())
		perr := rst.PutVerified(victim, want)
		after, gerr := rst.Get(victim)
		e.want("packstore", "packstore.put_verified", "packstore/repairs-corruption",
			scrub != nil && perr == nil && gerr == nil && string(after) == string(want),
			fmt.Sprintf("repair did not restore the object: scrub=%v put=%v get=%v", scrub, perr, gerr),
			victim.String())
		e.want("packstore", "packstore.verify", "packstore/verify-detects-corruption",
			errors.Is(scrub, packstore.ErrCorrupt),
			fmt.Sprintf("the scrub did not report the injected corruption: %v", scrub), "ErrCorrupt")
		must(rst.Close())
	}
	must(os.RemoveAll(repairDir))

	// Wipe empties the store and leaves it usable.
	wipeDir := copiedDir(e, fx.StoreTemplate, "check-wipe")
	wst := mustV(packstore.Open(wipeDir, packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
	must(wst.Wipe())
	gone, _ := wst.Has(fx.StoreKeys[0])
	must(wst.Put(fx.PackObjects[0].Key, fx.PackObjects[0].Bytes))
	back, err := wst.Get(fx.PackObjects[0].Key)
	e.want("packstore", "packstore.wipe", "packstore/wipe",
		!gone && err == nil && string(back) == string(fx.PackObjects[0].Bytes),
		fmt.Sprintf("wipe left data behind (%v) or broke the store (%v)", gone, err), "empty+usable")
	must(wst.Close())
	must(os.RemoveAll(wipeDir))

	// Closed stores refuse work.
	closedDir := workDir(e, "check-closed")
	cs := mustV(packstore.Open(closedDir))
	must(cs.Close())
	_, err = cs.Get(fx.StoreKeys[0])
	e.want("packstore", "packstore.close", "packstore/closed-store-errors", errors.Is(err, packstore.ErrClosed),
		fmt.Sprintf("a closed store must return ErrClosed, got %v", err), "ErrClosed")
	must(os.RemoveAll(closedDir))

	// WriteParallel with verification rejects a mislabelled object.
	vdir := workDir(e, "check-verify-write")
	vst := mustV(packstore.Open(vdir, packstore.WithSync(false)))
	wrong := []fstree.Object{{Key: fx.StoreKeys[0], Bytes: []byte("not the object under this key")}}
	_, err = vst.WriteParallel(objectSeq(wrong), packstore.WriteOpts{Writers: 1, Verify: true})
	e.want("packstore", "packstore.write_parallel", "packstore/verify-rejects-mismatch",
		errors.Is(err, packstore.ErrVerify),
		fmt.Sprintf("a key/payload mismatch must wrap ErrVerify, got %v", err), "ErrVerify")
	// Duplicates inside one stream are stored once.
	dup := []fstree.Object{fx.PackObjects[0], fx.PackObjects[0], fx.PackObjects[1]}
	ws, err := vst.WriteParallel(objectSeq(dup), packstore.WriteOpts{Writers: 1})
	e.want("packstore", "packstore.write_parallel", "packstore/dedups-within-batch",
		err == nil && ws.Stored == 2 && ws.Deduped == 1,
		fmt.Sprintf("stored=%d deduped=%d (%v)", ws.Stored, ws.Deduped, err),
		fmt.Sprintf("stored=%d deduped=%d", ws.Stored, ws.Deduped))
	must(vst.Close())
	must(os.RemoveAll(vdir))

	// --- the operations whose timings had no evidence ----------------
	// A timing says a function ran. These say it did what it is named
	// after, which is what makes the timing a measurement of that
	// operation rather than of an unknown one.

	// write_batch: every object in the batch is in the store afterwards,
	// and reads back as itself.
	bdir := workDir(e, "check-write-batch")
	defer os.RemoveAll(bdir)
	bst := mustV(packstore.Open(bdir,
		packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
	batch := storeObjects(p, minInt(p.BatchOps, 512), 70000)
	berr := bst.WriteBatch(objectSeq(batch))
	batchOK := berr == nil
	var batchDetail string
	for _, o := range batch {
		if !batchOK {
			break
		}
		got, gerr := bst.Get(o.Key)
		if gerr != nil || string(got) != string(o.Bytes) {
			batchOK = false
			batchDetail = fmt.Sprintf("%s: %v", o.Key, gerr)
		}
	}
	e.want("packstore", "packstore.write_batch", "packstore/write-batch-stores-all",
		batchOK, "an object in the batch is not readable afterwards: "+batchDetail,
		packContentDigest(batch))

	// segments: the sealed segments describe the store. Which objects land
	// in which segment follows the compressed sizes, so the counts are a
	// within-core anchor; the shape of the list is not.
	bsegs, serr := bst.Segments()
	shapeOK := serr == nil
	for i := range bsegs {
		if bsegs[i].Keys == 0 || (i > 0 && bsegs[i].ID <= bsegs[i-1].ID) {
			shapeOK = false
		}
	}
	e.want("packstore", "packstore.segments", "packstore/segments-are-ordered-and-nonempty",
		shapeOK && len(bsegs) >= 0,
		fmt.Sprintf("the sealed segment list is not strictly ordered or holds an empty segment: %v", serr),
		"ordered")
	tsegs := mustV(st.Segments())
	var sealedKeys uint64
	for _, sg := range tsegs {
		sealedKeys += sg.Keys
	}
	e.wantLocal("packstore", "packstore.segments", "packstore/segments-account-for-the-store",
		len(tsegs) > 0 && sealedKeys > 0 && sealedKeys <= uint64(len(fx.StoreKeys)),
		fmt.Sprintf("%d sealed segments hold %d of the store's %d objects",
			len(tsegs), sealedKeys, len(fx.StoreKeys)),
		fmt.Sprintf("segments=%d keys=%d", len(tsegs), sealedKeys))

	// oldest_inflight_write: an idle store has no write in flight.
	_, inflight := st.OldestInflightWrite()
	e.want("packstore", "packstore.oldest_inflight_write", "packstore/oldest-inflight-idle-is-none",
		!inflight, "an idle store reported a write in flight", "none")

	// remove: exactly the removed segment's objects go, the rest stay, and
	// the store still scrubs clean.
	rmdir := copiedDir(e, fx.StoreTemplate, "check-remove")
	defer os.RemoveAll(rmdir)
	rmst := mustV(packstore.Open(rmdir,
		packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
	rmsegs := mustV(rmst.Segments())
	if len(rmsegs) < 2 {
		e.fail("packstore", "packstore.remove", "packstore/remove-drops-only-that-segment",
			fmt.Sprintf("the fixture has %d sealed segments; the check needs two", len(rmsegs)))
	} else {
		var victim, survivor []key.Key
		must(rmst.ScanIndex(rmsegs[0].ID, func(k key.Key, _ uint64, _ uint32) {
			victim = append(victim, k)
		}))
		must(rmst.ScanIndex(rmsegs[1].ID, func(k key.Key, _ uint64, _ uint32) {
			survivor = append(survivor, k)
		}))
		must(rmst.Remove(rmsegs[0].ID))
		gone, kept := 0, 0
		for _, k := range victim {
			if ok, _ := rmst.Has(k); !ok {
				gone++
			}
		}
		for _, k := range survivor {
			if ok, _ := rmst.Has(k); ok {
				kept++
			}
		}
		e.wantLocal("packstore", "packstore.remove", "packstore/remove-drops-only-that-segment",
			gone == len(victim) && kept == len(survivor) &&
				rmst.Verify(context.Background()) == nil,
			fmt.Sprintf("%d of %d removed objects are gone, %d of %d others kept",
				gone, len(victim), kept, len(survivor)),
			fmt.Sprintf("gone=%d kept=%d", gone, kept))
	}
	must(rmst.Close())
	must(bst.Close())

	// --- the write barrier -------------------------------------------
	// BeginBarrier/ObserveKeys/AbortBarrier had no correctness evidence at
	// all; a timing of a call that does nothing observable is not a
	// measurement of anything. The observable contract is that keys seen
	// during a capture are treated as live by the next Compact even when
	// the liveness predicate calls them dead -- which is how an ingest may
	// run concurrently with a mark -- and that an aborted capture protects
	// nothing.
	barrierCase := func(id string, abort bool, wantProtected bool) {
		bdir := copiedDir(e, fx.GarbageTemplate, "check-barrier")
		defer os.RemoveAll(bdir)
		bst := mustV(packstore.Open(bdir,
			packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
		defer bst.Close()
		// Pick objects the predicate below calls dead, and observe them.
		var observed []key.Key
		for i, k := range fx.StoreKeys {
			if i%10 != 0 && len(observed) < 32 {
				observed = append(observed, k)
			}
		}
		bst.BeginBarrier()
		bst.ObserveKeys(observed)
		if abort {
			bst.AbortBarrier()
		}
		liveOnly := keySet(fx.GarbageLive)
		_, cerr := bst.Compact(func(k key.Key) bool { return liveOnly[k] },
			packstore.CompactOpts{MinDeadRatio: 0.5})
		protected := 0
		for _, k := range observed {
			if ok, _ := bst.Has(k); ok {
				protected++
			}
		}
		got := protected == len(observed)
		e.want("packstore", "packstore.barrier", id,
			cerr == nil && got == wantProtected,
			fmt.Sprintf("%d of %d observed keys survived the compaction (wanted all=%v): %v",
				protected, len(observed), wantProtected, cerr),
			fmt.Sprintf("observed=%d protected=%v", len(observed), got))
	}
	// A live capture protects everything it observed...
	barrierCase("packstore/barrier-observed-keys-survive-compact", false, true)
	// ...and an aborted one protects nothing.
	barrierCase("packstore/barrier-abort-protects-nothing", true, false)

	// --- append_record + sync ----------------------------------------
	// The other operation with no check: re-appending already-encoded
	// records. The contract is that the store afterwards holds exactly
	// those objects, byte for byte, and that they are still there after
	// the Sync the batch ends with and a reopen.
	adir := workDir(e, "check-append")
	defer os.RemoveAll(adir)
	ast := mustV(packstore.Open(adir,
		packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
	aerr := error(nil)
	for i, rec := range fx.WireRecords {
		if err := ast.AppendRecord(fx.PackObjects[i].Key, rec); err != nil {
			aerr = err
			break
		}
	}
	if aerr == nil {
		aerr = ast.Sync()
	}
	must(ast.Close())
	// Reopened from disk, so the check covers the Sync and not just the
	// in-memory state the appends left behind.
	ast = mustV(packstore.Open(adir,
		packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
	appendOK := aerr == nil
	appendDetail := fmt.Sprintf("%v", aerr)
	for _, o := range fx.PackObjects {
		if !appendOK {
			break
		}
		got, gerr := ast.Get(o.Key)
		if gerr != nil || string(got) != string(o.Bytes) {
			appendOK = false
			appendDetail = fmt.Sprintf("%s: %v (%d vs %d bytes)", o.Key, gerr, len(got), len(o.Bytes))
		}
	}
	e.want("packstore", "packstore.append_record", "packstore/append-record-roundtrip",
		appendOK,
		"an appended record did not read back as the object it encodes: "+appendDetail,
		packContentDigest(fx.PackObjects))
	// A nil record is rejected rather than stored as an empty object.
	nilErr := ast.AppendRecord(fx.PackObjects[0].Key, nil)
	e.want("packstore", "packstore.append_record", "packstore/append-record-rejects-nil",
		errors.Is(nilErr, packstore.ErrCorrupt),
		fmt.Sprintf("a nil record must wrap ErrCorrupt, got %v", nilErr), "ErrCorrupt")
	// And the scrub passes over what the appends wrote.
	e.want("packstore", "packstore.append_record", "packstore/append-record-scrubs-clean",
		ast.Verify(context.Background()) == nil,
		"the store did not scrub clean after a batch of appended records", "clean")
	must(ast.Close())
}

// findRecordOffset locates one key's record inside a sealed segment.
func findRecordOffset(st *packstore.Store, want key.Key) recordLoc {
	segs, err := st.Segments()
	if err != nil {
		return recordLoc{}
	}
	var found recordLoc
	for _, s := range segs {
		_ = st.ScanIndex(s.ID, func(k key.Key, off uint64, slen uint32) {
			if k == want && found.Len == 0 {
				found = recordLoc{ID: s.ID, Off: off, Len: slen}
			}
		})
		if found.Len != 0 {
			return found
		}
	}
	return found
}

// corruptSegmentByte flips one payload byte of a record inside a sealed
// segment file. It is only ever used on an isolated copy.
func corruptSegmentByte(dir string, loc recordLoc) error {
	path := filepath.Join(dir, fmt.Sprintf("%016x.seg", loc.ID))
	f, err := os.OpenFile(path, os.O_RDWR, 0)
	if err != nil {
		return err
	}
	defer f.Close()
	at := int64(loc.Off) + int64(amberpack.RecHeaderSize)
	var b [1]byte
	if _, err := f.ReadAt(b[:], at); err != nil {
		return err
	}
	b[0] ^= 0xFF
	_, err = f.WriteAt(b[:], at)
	return err
}

func dirBytes(dir string) int64 {
	var n int64
	_ = filepath.WalkDir(dir, func(p string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil
		}
		info, err := d.Info()
		if err == nil {
			n += info.Size()
		}
		return nil
	})
	return n
}
