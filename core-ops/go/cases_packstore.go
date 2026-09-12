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
			Group: "packstore", Op: "packstore.open", Workload: "empty", Threads: 1, Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return &openState{dir: workDir(en, "ps-open-empty")} },
			Run: func(en *Env, s any) uint64 {
				st := s.(*openState)
				var err error
				st.st, err = packstore.Open(st.dir, packstore.WithSegmentSize(en.Profile.SegmentBytes))
				if err != nil {
					panic(err)
				}
				return 1
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
			Group: "packstore", Op: "packstore.open", Workload: "populated-reopen", Threads: 1, Ops: 1, PerRep: true,
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
				return uint64(len(segs))
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
			Group: "packstore", Op: "packstore.close", Workload: "populated", Threads: 1, Ops: 1, PerRep: true,
			Setup: func(en *Env) any {
				dir := copiedDir(en, en.fx.StoreTemplate, "ps-close")
				st := mustV(packstore.Open(dir, packstore.WithSegmentSize(en.Profile.SegmentBytes)))
				return &openState{dir: dir, st: st}
			},
			Run: func(_ *Env, s any) uint64 {
				st := s.(*openState)
				if err := st.st.Close(); err != nil {
					panic(err)
				}
				st.st = nil
				return 1
			},
			Free: func(_ *Env, s any) { _ = os.RemoveAll(s.(*openState).dir) },
		},
	)

	// --- reads --------------------------------------------------------
	read := func(op, workload string, ops int, bytes int64, run func(*Env, *packstore.Store) uint64) Case {
		return Case{
			Group: "packstore", Op: op, Workload: workload, Threads: 1, Ops: ops, Bytes: bytes,
			Setup: func(en *Env) any { return en.fx.RO.st },
			Run:   func(en *Env, s any) uint64 { return run(en, s.(*packstore.Store)) },
		}
	}
	out = append(out,
		read("packstore.get", "hit", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			var acc uint64
			for _, k := range hitKeys {
				b, err := st.Get(k)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(b))
			}
			return acc
		}),
		read("packstore.get", "miss", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			var acc uint64
			for _, k := range missKeys {
				if _, err := st.Get(k); errors.Is(err, packstore.ErrNotFound) {
					acc++
				}
			}
			return acc
		}),
		read("packstore.get_record", "hit", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			var acc uint64
			for _, k := range hitKeys {
				b, err := st.GetRecord(k)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(b))
			}
			return acc
		}),
		read("packstore.has", "hit", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			var acc uint64
			for _, k := range hitKeys {
				ok, err := st.Has(k)
				if err != nil {
					panic(err)
				}
				if ok {
					acc++
				}
			}
			return acc
		}),
		read("packstore.has", "miss", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			var acc uint64
			for _, k := range missKeys {
				ok, err := st.Has(k)
				if err != nil {
					panic(err)
				}
				if !ok {
					acc++
				}
			}
			return acc
		}),
		read("packstore.stored_size", "hit", lookups, 0, func(_ *Env, st *packstore.Store) uint64 {
			var acc uint64
			for _, k := range hitKeys {
				n, ok, err := st.StoredSize(k)
				if err != nil {
					panic(err)
				}
				if ok {
					acc += n
				}
			}
			return acc
		}),
		read("packstore.missing", "half-present", len(mixed), 0, func(_ *Env, st *packstore.Store) uint64 {
			out, err := st.Missing(mixed)
			if err != nil {
				panic(err)
			}
			return uint64(len(out))
		}),
		read("packstore.sort_by_location", "scattered", len(mixed), 0, func(_ *Env, st *packstore.Store) uint64 {
			ks := make([]key.Key, len(mixed))
			copy(ks, mixed)
			st.SortByLocation(ks)
			return uint64(ks[0][0])
		}),
		read("packstore.segments", "list", 64, 0, func(_ *Env, st *packstore.Store) uint64 {
			var acc uint64
			for i := 0; i < 64; i++ {
				segs, err := st.Segments()
				if err != nil {
					panic(err)
				}
				acc += uint64(len(segs))
			}
			return acc
		}),
		read("packstore.scan_index", "one-segment", 1, 0, func(en *Env, st *packstore.Store) uint64 {
			var acc uint64
			err := st.ScanIndex(en.fx.ROSegID, func(k key.Key, off uint64, slen uint32) {
				acc += uint64(slen) + uint64(k[0])
			})
			if err != nil {
				panic(err)
			}
			return acc
		}),
		read("packstore.record", "by-location", minInt(lookups, 4096), 0, func(en *Env, st *packstore.Store) uint64 {
			var acc uint64
			locs := en.fx.ROLocs
			n := minInt(lookups, 4096)
			for i := 0; i < n; i++ {
				l := locs[(i*7919)%len(locs)]
				b, err := st.Record(l.ID, l.Off)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(b))
			}
			return acc
		}),
		read("packstore.has_outside", "sealed-segment", lookups, 0, func(en *Env, st *packstore.Store) uint64 {
			var acc uint64
			for _, k := range hitKeys {
				ok, err := st.HasOutside(en.fx.ROSegID, k)
				if err != nil {
					panic(err)
				}
				if ok {
					acc++
				}
			}
			return acc
		}),
		read("packstore.oldest_inflight_write", "idle", 4096, 0, func(_ *Env, st *packstore.Store) uint64 {
			var acc uint64
			for i := 0; i < 4096; i++ {
				if _, ok := st.OldestInflightWrite(); ok {
					acc++
				}
			}
			return acc
		}),
		read("packstore.new_mark_set", "snapshot", 16, 0, func(_ *Env, st *packstore.Store) uint64 {
			var acc uint64
			for i := 0; i < 16; i++ {
				acc += uint64(st.NewMarkSet().Marked() + 1)
			}
			return acc
		}),
		read("packstore.mark_set_mark", "all-keys", len(fx.StoreKeys), 0, func(en *Env, st *packstore.Store) uint64 {
			ms := st.NewMarkSet()
			var acc uint64
			for _, k := range en.fx.StoreKeys {
				if newly, present := ms.Mark(k); newly && present {
					acc++
				}
			}
			return acc + uint64(ms.Marked())
		}),
		read("packstore.mark_set_contains", "all-keys", len(fx.StoreKeys), 0, func(en *Env, st *packstore.Store) uint64 {
			ms := st.NewMarkSet()
			for _, k := range en.fx.StoreKeys {
				ms.Mark(k)
			}
			var acc uint64
			for _, k := range en.fx.StoreKeys {
				if ms.Contains(k) {
					acc++
				}
			}
			return acc
		}),
		read("packstore.liveness", "tenth-live", 1, 0, func(en *Env, st *packstore.Store) uint64 {
			live := keySet(en.fx.GarbageLive)
			ls, err := st.Liveness(func(k key.Key) bool { return live[k] })
			if err != nil {
				panic(err)
			}
			var acc uint64
			for _, l := range ls {
				acc += uint64(l.LiveKeys + l.DeadKeys)
			}
			return acc
		}),
		read("packstore.verify", "full-scrub", 1, 0, func(_ *Env, st *packstore.Store) uint64 {
			if err := st.Verify(context.Background()); err != nil {
				panic(err)
			}
			return 1
		}),
		read("packstore.barrier", "begin+observe+abort", len(fx.StoreKeys), 0,
			func(en *Env, st *packstore.Store) uint64 {
				st.BeginBarrier()
				st.ObserveKeys(en.fx.StoreKeys)
				st.AbortBarrier()
				return uint64(len(en.fx.StoreKeys))
			}),
	)

	// --- writes -------------------------------------------------------
	writeObjs := storeObjects(p, minInt(p.StoreObjects, 8000), 50000)
	tiny := storeObjectsOfSize(p, minInt(p.BatchOps, 4000), 128, 60000)
	large := storeObjectsOfSize(p, maxInt(minInt(p.BatchOps/16, 256), 16), 256<<10, 61000)

	putCase := func(workload string, objs []fstree.Object, sync bool) Case {
		return Case{
			Group: "packstore", Op: "packstore.put", Workload: workload, Threads: 1,
			Ops: len(objs), Bytes: packBytes(objs), PerRep: true,
			Setup: func(en *Env) any {
				if sync {
					return freshStoreSync(en, "ps-put")
				}
				return freshStore(en, "ps-put")
			},
			Run: func(_ *Env, s any) uint64 {
				st := s.(*storeHandle).st
				var acc uint64
				for _, o := range objs {
					if err := st.Put(o.Key, o.Bytes); err != nil {
						panic(err)
					}
					acc += uint64(o.Key[0])
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		}
	}
	out = append(out,
		putCase("tiny-128B/sync-off", tiny, false),
		putCase("tiny-128B/sync-on", tiny, true),
		putCase("large-256KiB/sync-off", large, false),
		Case{
			Group: "packstore", Op: "packstore.put", Workload: "duplicate/sync-off", Threads: 1,
			Ops: len(tiny), Bytes: packBytes(tiny), PerRep: true,
			Setup: func(en *Env) any {
				h := freshStore(en, "ps-put-dup")
				for _, o := range tiny {
					must(h.st.Put(o.Key, o.Bytes))
				}
				return h
			},
			Run: func(_ *Env, s any) uint64 {
				st := s.(*storeHandle).st
				var acc uint64
				for _, o := range tiny {
					if err := st.Put(o.Key, o.Bytes); err != nil {
						panic(err)
					}
					acc++
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
		Case{
			Group: "packstore", Op: "packstore.write_batch", Workload: "mixed-objects", Threads: 1,
			Ops: len(writeObjs), Bytes: packBytes(writeObjs), PerRep: true,
			Setup: func(en *Env) any { return freshStore(en, "ps-batch") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*storeHandle).st
				if err := st.WriteBatch(objectSeq(writeObjs)); err != nil {
					panic(err)
				}
				return uint64(len(writeObjs))
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
		Case{
			Group: "packstore", Op: "packstore.append_record", Workload: "pre-encoded+sync", Threads: 1,
			Ops: len(fx.WireRecords), PerRep: true,
			Setup: func(en *Env) any { return freshStore(en, "ps-append") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*storeHandle).st
				for i, rec := range en.fx.WireRecords {
					if err := st.AppendRecord(en.fx.PackObjects[i].Key, rec); err != nil {
						panic(err)
					}
				}
				if err := st.Sync(); err != nil {
					panic(err)
				}
				return uint64(len(en.fx.WireRecords))
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
	)
	for _, writers := range []int{p.ThreadsSingle, p.ThreadsMulti} {
		for _, verify := range []bool{false, true} {
			writers, verify := writers, verify
			name := fmt.Sprintf("mixed-objects/writers-%d/verify-%v", writers, verify)
			out = append(out, Case{
				Group: "packstore", Op: "packstore.write_parallel", Workload: name, Threads: writers,
				Ops: len(writeObjs), Bytes: packBytes(writeObjs), PerRep: true,
				Setup: func(en *Env) any { return freshStore(en, "ps-parallel") },
				Run: func(_ *Env, s any) uint64 {
					st := s.(*storeHandle).st
					stats, err := st.WriteParallel(objectSeq(writeObjs),
						packstore.WriteOpts{Writers: writers, Verify: verify})
					if err != nil {
						panic(err)
					}
					return uint64(stats.Stored)
				},
				Free: func(_ *Env, s any) { s.(*storeHandle).close() },
			})
		}
	}
	out = append(out, Case{
		Group: "packstore", Op: "packstore.write_parallel", Workload: "duplicate-stream/writers-8", Threads: p.ThreadsMulti,
		Ops: len(writeObjs), Bytes: packBytes(writeObjs), PerRep: true,
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
			return uint64(stats.Deduped)
		},
		Free: func(_ *Env, s any) { s.(*storeHandle).close() },
	})

	// --- destructive maintenance -------------------------------------
	out = append(out,
		Case{
			Group: "packstore", Op: "packstore.compact", Workload: "90-percent-dead", Threads: 1,
			Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return copiedStore(en, en.fx.GarbageTemplate, "ps-compact") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*storeHandle).st
				live := keySet(en.fx.GarbageLive)
				stats, err := st.Compact(func(k key.Key) bool { return live[k] },
					packstore.CompactOpts{MinDeadRatio: 0.5})
				if err != nil {
					panic(err)
				}
				return uint64(stats.RecordsCopied) + stats.BytesFreed
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
		Case{
			Group: "packstore", Op: "packstore.compact", Workload: "nothing-dead", Threads: 1,
			Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return copiedStore(en, en.fx.GarbageTemplate, "ps-compact-live") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*storeHandle).st
				stats, err := st.Compact(func(key.Key) bool { return true },
					packstore.CompactOpts{MinDeadRatio: 0.5})
				if err != nil {
					panic(err)
				}
				return uint64(stats.SegmentsScanned)
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
		Case{
			Group: "packstore", Op: "packstore.remove", Workload: "one-sealed-segment", Threads: 1,
			Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return copiedStore(en, en.fx.StoreTemplate, "ps-remove") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*storeHandle).st
				segs, err := st.Segments()
				if err != nil {
					panic(err)
				}
				if err := st.Remove(segs[0].ID); err != nil {
					panic(err)
				}
				return segs[0].ID + 1
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
		Case{
			Group: "packstore", Op: "packstore.wipe", Workload: "populated", Threads: 1,
			Ops: 1, PerRep: true,
			Setup: func(en *Env) any { return copiedStore(en, en.fx.StoreTemplate, "ps-wipe") },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*storeHandle).st
				if err := st.Wipe(); err != nil {
					panic(err)
				}
				return 1
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
		// PutVerified rewrites the segments holding a corrupt copy of the
		// key, so it runs on its own copy only.
		Case{
			Group: "packstore", Op: "packstore.put_verified", Workload: "already-intact", Threads: 1,
			Ops: minInt(p.BatchOps/8, 256), PerRep: true,
			Setup: func(en *Env) any { return copiedStore(en, en.fx.StoreTemplate, "ps-putverified") },
			Run: func(en *Env, s any) uint64 {
				st := s.(*storeHandle).st
				n := minInt(en.Profile.BatchOps/8, 256)
				var acc uint64
				for i := 0; i < n; i++ {
					o := en.fx.PackObjects[i%len(en.fx.PackObjects)]
					if err := st.PutVerified(o.Key, o.Bytes); err != nil {
						panic(err)
					}
					acc++
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
	)
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
	retained := true
	for _, k := range fx.GarbageLive {
		if _, gerr := cst.Get(k); gerr != nil {
			retained = false
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
	e.want("packstore", "packstore.compact", "packstore/compact-retains-live",
		err == nil && retained, fmt.Sprintf("a live object was lost by compaction (%v)", err),
		fmt.Sprintf("live=%d", len(fx.GarbageLive)))
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
