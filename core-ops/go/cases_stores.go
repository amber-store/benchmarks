package main

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sync/atomic"

	"github.com/amber-store/core/amberpack"
	"github.com/amber-store/core/inbox"
	"github.com/amber-store/core/key"
	"github.com/amber-store/core/packstore"
	"github.com/amber-store/core/refstore"
)

// ---------------------------------------------------------------------------
// refstore
// ---------------------------------------------------------------------------

type refOpenState struct {
	dir string
	st  *refstore.Store
}

func refstoreCases(e *Env) []Case {
	fx := e.fx
	n := len(fx.RefBatch)
	hits := make([]string, minInt(e.Profile.BatchOps, n))
	for i := range hits {
		hits[i] = fx.RefNames[(i*7919)%n]
	}
	misses := make([]string, len(hits))
	for i := range misses {
		misses[i] = fmt.Sprintf("bench/absent/%06d", i)
	}

	var out []Case
	out = append(out,
		Case{
			Group: "refstore", Op: "refstore.open", Workload: "empty", Threads: 1, Ops: 1,
			Dims: Dims{Entries: 0, Items: 1, Content: "structured"}, PerRep: true,
			Setup: func(en *Env) any { return &refOpenState{dir: workDir(en, "rs-open-empty")} },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*refOpenState)
				var err error
				st.st, err = refstore.Open(st.dir, false)
				if err != nil {
					panic(err)
				}
				return 1
			},
			Free: func(_ *Env, s any) {
				st := s.(*refOpenState)
				if st.st != nil {
					_ = st.st.Close()
				}
				_ = os.RemoveAll(st.dir)
			},
		},
		Case{
			Group: "refstore", Op: "refstore.open", Workload: "populated-reopen", Threads: 1, Ops: 1,
			Dims: Dims{Entries: int64(n), Items: 1, Content: "structured"}, PerRep: true,
			Setup: func(en *Env) any {
				return &refOpenState{dir: copiedDir(en, en.fx.RefsTemplate, "rs-open-full")}
			},
			Run: func(_ *Env, s any) uint64 {
				st := s.(*refOpenState)
				var err error
				st.st, err = refstore.Open(st.dir, false)
				if err != nil {
					panic(err)
				}
				b, err := st.st.Get(fxRefProbe)
				if err != nil {
					panic(err)
				}
				return uint64(len(b))
			},
			Free: func(_ *Env, s any) {
				st := s.(*refOpenState)
				if st.st != nil {
					_ = st.st.Close()
				}
				_ = os.RemoveAll(st.dir)
			},
		},
		Case{
			Group: "refstore", Op: "refstore.close", Workload: "populated", Threads: 1, Ops: 1,
			Dims: Dims{Entries: int64(n), Items: 1, Content: "structured"}, PerRep: true,
			Setup: func(en *Env) any {
				dir := copiedDir(en, en.fx.RefsTemplate, "rs-close")
				return &refOpenState{dir: dir, st: mustV(refstore.Open(dir, false))}
			},
			Run: func(_ *Env, s any) uint64 {
				st := s.(*refOpenState)
				if err := st.st.Close(); err != nil {
					panic(err)
				}
				st.st = nil
				return 1
			},
			Free: func(_ *Env, s any) { _ = os.RemoveAll(s.(*refOpenState).dir) },
		},
	)

	for _, sync := range []bool{false, true} {
		sync := sync
		out = append(out, Case{
			Group: "refstore", Op: "refstore.put",
			Workload: fmt.Sprintf("records/sync-%v", sync), Threads: 1,
			Ops:    n,
			Dims:   Dims{Entries: int64(n), Items: int64(n), Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return freshRefs(en, "rs-put", sync) },
			Run: func(en *Env, s any) uint64 {
				st := s.(*refHandle).st
				acc := newFold()
				for _, r := range en.fx.RefBatch {
					err := st.Put(r.Name, r.Data)
					if err != nil {
						panic(err)
					}
					acc = foldBool(acc, true)
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*refHandle).close() },
		})
	}
	out = append(out,
		Case{
			Group: "refstore", Op: "refstore.put_batch",
			Workload: "records/sync-false", Threads: 1, Ops: n,
			Dims:   Dims{Entries: int64(n), Items: int64(n), Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return freshRefs(en, "rs-batch", false) },
			Run: func(en *Env, s any) uint64 {
				if err := s.(*refHandle).st.PutBatch(en.fx.RefBatch); err != nil {
					panic(err)
				}
				return foldI64(newFold(), int64(len(en.fx.RefBatch)))
			},
			Free: func(_ *Env, s any) { s.(*refHandle).close() },
		},
		Case{
			Group: "refstore", Op: "refstore.get", Workload: "hit", Threads: 1, Ops: len(hits),
			Dims:  Dims{Entries: int64(n), Items: int64(len(hits)), Content: "structured"},
			Setup: func(en *Env) any { return copiedRefs(en, en.fx.RefsTemplate, "rs-get", false) },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*refHandle).st
				acc := newFold()
				for _, name := range hits {
					b, err := st.Get(name)
					if err != nil {
						panic(err)
					}
					acc = sinkBytes(acc, b)
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*refHandle).close() },
		},
		Case{
			Group: "refstore", Op: "refstore.get", Workload: "miss", Threads: 1, Ops: len(misses),
			Dims:  Dims{Entries: int64(n), Items: int64(len(misses)), Content: "structured"},
			Setup: func(en *Env) any { return copiedRefs(en, en.fx.RefsTemplate, "rs-get-miss", false) },
			Run: func(_ *Env, s any) uint64 {
				st := s.(*refHandle).st
				acc := newFold()
				for _, name := range misses {
					_, err := st.Get(name)
					acc = foldBool(acc, errors.Is(err, refstore.ErrNotFound))
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*refHandle).close() },
		},
		Case{
			Group: "refstore", Op: "refstore.all", Workload: "records", Threads: 1,
			Ops:   n,
			Dims:  Dims{Entries: int64(n), Items: 1, Content: "structured"},
			Setup: func(en *Env) any { return copiedRefs(en, en.fx.RefsTemplate, "rs-all", false) },
			Run: func(_ *Env, s any) uint64 {
				recs, err := s.(*refHandle).st.All()
				if err != nil {
					panic(err)
				}
				acc := foldI64(newFold(), int64(len(recs)))
				for _, r := range recs {
					acc = foldStr(acc, r.Name)
					acc = sinkBytes(acc, r.Data)
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*refHandle).close() },
		},
		Case{
			Group: "refstore", Op: "refstore.delete", Workload: "records", Threads: 1,
			Ops:    n,
			Dims:   Dims{Entries: int64(n), Items: int64(n), Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return copiedRefs(en, en.fx.RefsTemplate, "rs-delete", false) },
			Run: func(en *Env, s any) uint64 {
				st := s.(*refHandle).st
				acc := newFold()
				for _, name := range en.fx.RefNames {
					err := st.Delete(name)
					if err != nil {
						panic(err)
					}
					acc = foldBool(acc, true)
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*refHandle).close() },
		},
		Case{
			Group: "refstore", Op: "refstore.wipe", Workload: "records", Threads: 1,
			Ops:    1,
			Dims:   Dims{Entries: int64(n), Items: 1, Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return copiedRefs(en, en.fx.RefsTemplate, "rs-wipe", false) },
			Run: func(_ *Env, s any) uint64 {
				if err := s.(*refHandle).st.Wipe(); err != nil {
					panic(err)
				}
				recs, err := s.(*refHandle).st.All()
				if err != nil {
					panic(err)
				}
				return foldI64(newFold(), int64(len(recs)))
			},
			Free: func(_ *Env, s any) { s.(*refHandle).close() },
		},
	)
	return out
}

// fxRefProbe is the reference name the reopen case looks up to prove the
// store is usable after Open returns.
const fxRefProbe = "bench/ref/000000"

func refstoreChecks(e *Env) {
	fx := e.fx

	// Open and close had no correctness evidence: a timing of "open a
	// store" says nothing unless the store is usable afterwards, and a
	// timing of "close a store" says nothing unless what was committed is
	// still there. Write, close, reopen, read back.
	rdir := workDir(e, "check-refs-reopen")
	defer os.RemoveAll(rdir)
	rs := mustV(refstore.Open(rdir, false))
	must(rs.PutBatch(fx.RefBatch[:minInt(64, len(fx.RefBatch))]))
	must(rs.Close())
	rs = mustV(refstore.Open(rdir, false))
	reopenOK := true
	var reopenDetail string
	for _, rec := range fx.RefBatch[:minInt(64, len(fx.RefBatch))] {
		got, gerr := rs.Get(rec.Name)
		if gerr != nil || !bytes.Equal(got, rec.Data) {
			reopenOK = false
			reopenDetail = fmt.Sprintf("%s: %v", rec.Name, gerr)
			break
		}
	}
	e.want("refstore", "refstore.close", "refstore/reopen-sees-committed-records", reopenOK,
		"a record committed before the close was not there after the reopen: "+reopenDetail,
		fmt.Sprintf("n=%d", minInt(64, len(fx.RefBatch))))
	must(rs.Close())

	h := copiedRefs(e, fx.RefsTemplate, "check-refs", false)
	defer h.close()
	st := h.st

	b, err := st.Get(fx.RefNames[0])
	e.want("refstore", "refstore.get", "refstore/get-verbatim",
		err == nil && bytes.Equal(b, fx.RefBatch[0].Data),
		fmt.Sprintf("stored record bytes differ from what was put: %v", err), digest(b))
	_, err = st.Get("bench/absent")
	e.want("refstore", "refstore.get", "refstore/get-miss", errors.Is(err, refstore.ErrNotFound),
		fmt.Sprintf("an absent name must return ErrNotFound, got %v", err), "ErrNotFound")

	all, err := st.All()
	ordered := err == nil && len(all) == len(fx.RefBatch)
	for i := 1; i < len(all) && ordered; i++ {
		if all[i-1].Name >= all[i].Name {
			ordered = false
		}
	}
	names := make([]string, 0, len(all))
	for _, r := range all {
		names = append(names, r.Name)
	}
	e.want("refstore", "refstore.all", "refstore/all-lexicographic", ordered,
		fmt.Sprintf("All returned %d records (%v), not in strict name order", len(all), err),
		digestStrings(names))

	// Put overwrites unconditionally; Delete is exact; Wipe empties.
	must(st.Put(fx.RefNames[0], []byte{0x01, 0x02}))
	b, err = st.Get(fx.RefNames[0])
	e.want("refstore", "refstore.put", "refstore/put-overwrites",
		err == nil && bytes.Equal(b, []byte{0x01, 0x02}),
		fmt.Sprintf("Put did not overwrite: %v", err), digest(b))
	must(st.Delete(fx.RefNames[0]))
	_, err = st.Get(fx.RefNames[0])
	e.want("refstore", "refstore.delete", "refstore/delete", errors.Is(err, refstore.ErrNotFound),
		fmt.Sprintf("the deleted name is still readable: %v", err), "ErrNotFound")
	err = st.Delete(fx.RefNames[0])
	e.want("refstore", "refstore.delete", "refstore/delete-absent", errors.Is(err, refstore.ErrNotFound),
		fmt.Sprintf("deleting an absent name must return ErrNotFound, got %v", err), "ErrNotFound")
	// PutBatch: last write wins for a repeated name.
	must(st.PutBatch([]refstore.Record{
		{Name: "bench/dup", Data: []byte("first")},
		{Name: "bench/dup", Data: []byte("second")},
	}))
	b, err = st.Get("bench/dup")
	e.want("refstore", "refstore.put_batch", "refstore/batch-last-wins",
		err == nil && string(b) == "second",
		fmt.Sprintf("repeated batch name resolved to %q (%v)", b, err), "second")
	must(st.Wipe())
	all, err = st.All()
	e.want("refstore", "refstore.wipe", "refstore/wipe", err == nil && len(all) == 0,
		fmt.Sprintf("wipe left %d records (%v)", len(all), err), "0")
}

// ---------------------------------------------------------------------------
// inbox
// ---------------------------------------------------------------------------

// inboxState is an open inbox over its own store, plus the staged files a
// case prepared outside the measured interval.
type inboxState struct {
	dir    string
	store  *storeHandle
	ib     *inbox.Inbox
	tmps   []string
	hashes [][]byte
}

func (s *inboxState) close() {
	if s.ib != nil {
		_ = s.ib.Close()
		s.ib = nil
	}
	if s.store != nil {
		s.store.close()
		s.store = nil
	}
	_ = os.RemoveAll(s.dir)
}

func newInbox(e *Env, name string, workers int) *inboxState {
	return newInboxWithGate(e, name, workers)
}

// newInboxWithGate is newInbox with the Go core's functional options. The
// Rust inbox has no options parameter, so only the Go-side gate check uses
// this; every measured case goes through newInbox and is therefore the same
// call on both sides.
func newInboxWithGate(e *Env, name string, workers int, opts ...inbox.Option) *inboxState {
	st := freshStore(e, name+"-store")
	dir := workDir(e, name)
	ib, err := inbox.Open(dir, st.st, workers, nil, opts...)
	must(err)
	return &inboxState{dir: dir, store: st, ib: ib}
}

func (s *inboxState) stageAll(fx *Fixtures) {
	for i, body := range fx.InboxPacks {
		meta := inbox.Meta{
			Ref:        fmt.Sprintf("bench/inbox/%03d", i),
			Root:       keyBytes(fx.InboxRoots[i]),
			ReceivedAt: int64(1_700_000_000_000_000_000 + int64(i)),
		}
		tmp, hash, _, err := s.ib.Stage(meta, bytes.NewReader(body))
		must(err)
		s.tmps = append(s.tmps, tmp)
		s.hashes = append(s.hashes, hash)
	}
}

func keyBytes(k key.Key) []byte {
	kk := k
	return append([]byte(nil), kk[:]...)
}

func inboxCases(e *Env) []Case {
	p := e.Profile
	fx := e.fx
	var out []Case

	out = append(out,
		Case{
			Group: "inbox", Op: "inbox.open", Workload: "empty", Threads: p.ThreadsMulti, Ops: 1,
			Dims: Dims{Items: 1, Content: "structured"}, PerRep: true,
			Setup: func(en *Env) any {
				st := freshStore(en, "inbox-open-store")
				return &inboxState{dir: workDir(en, "inbox-open"), store: st}
			},
			Run: func(en *Env, s any) uint64 {
				is := s.(*inboxState)
				ib, err := inbox.Open(is.dir, is.store.st, en.Profile.ThreadsMulti, nil)
				if err != nil {
					panic(err)
				}
				is.ib = ib
				return foldBool(newFold(), ib != nil)
			},
			Free: func(_ *Env, s any) { s.(*inboxState).close() },
		},
		Case{
			Group: "inbox", Op: "inbox.open", Workload: "sweeps-staged-tmp-files",
			Threads: p.ThreadsMulti, Ops: p.InboxPacks,
			Dims:   Dims{Items: int64(p.InboxPacks), Objects: int64(p.InboxPacks * 32), Content: "structured"},
			PerRep: true,
			Setup: func(en *Env) any {
				// Stage without committing, then close: the tmp files are
				// exactly what the next Open has to sweep.
				is := newInbox(en, "inbox-recover", en.Profile.ThreadsMulti)
				is.stageAll(en.fx)
				must(is.ib.Close())
				is.ib = nil
				return is
			},
			Run: func(en *Env, s any) uint64 {
				is := s.(*inboxState)
				ib, err := inbox.Open(is.dir, is.store.st, en.Profile.ThreadsMulti, nil)
				if err != nil {
					panic(err)
				}
				is.ib = ib
				return foldBool(newFold(), ib != nil)
			},
			Free: func(_ *Env, s any) { s.(*inboxState).close() },
		},
		Case{
			Group: "inbox", Op: "inbox.stage", Workload: "packs",
			Threads: p.ThreadsMulti, Ops: p.InboxPacks, Bytes: fx.InboxLogicalBytes, BytesKind: BytesPayload,
			Dims:   Dims{Items: int64(p.InboxPacks), Objects: int64(p.InboxPacks * 32), Content: "structured"},
			PerRep: true,
			Setup:  func(en *Env) any { return newInbox(en, "inbox-stage", en.Profile.ThreadsMulti) },
			Run: func(en *Env, s any) uint64 {
				is := s.(*inboxState)
				is.stageAll(en.fx)
				acc := foldI64(newFold(), int64(len(is.tmps)))
				for _, h := range is.hashes {
					acc = foldBytes(acc, h[:])
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*inboxState).close() },
		},
		Case{
			Group: "inbox", Op: "inbox.discard", Workload: "packs",
			Threads: p.ThreadsMulti, Ops: p.InboxPacks,
			Dims:   Dims{Items: int64(p.InboxPacks), Objects: int64(p.InboxPacks * 32), Content: "structured"},
			PerRep: true,
			Setup: func(en *Env) any {
				is := newInbox(en, "inbox-discard", en.Profile.ThreadsMulti)
				is.stageAll(en.fx)
				return is
			},
			Run: func(_ *Env, s any) uint64 {
				is := s.(*inboxState)
				acc := newFold()
				for _, tmp := range is.tmps {
					is.ib.Discard(tmp)
					acc = foldStr(acc, tmp)
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*inboxState).close() },
		},
		// Commit plus the wait is the end-to-end drain: the workers store
		// every pack's objects into the packstore.
		Case{
			Group: "inbox", Op: "inbox.drain", Workload: "packs/new",
			Threads: p.ThreadsMulti, Ops: p.InboxPacks, Bytes: fx.InboxLogicalBytes, BytesKind: BytesPayload,
			Dims:   Dims{Items: int64(p.InboxPacks), Objects: int64(p.InboxPacks * 32), Content: "structured"},
			PerRep: true,
			Setup: func(en *Env) any {
				is := newInbox(en, "inbox-drain", en.Profile.ThreadsMulti)
				is.stageAll(en.fx)
				return is
			},
			Run: func(en *Env, s any) uint64 {
				is := s.(*inboxState)
				acc := newFold()
				for i, tmp := range is.tmps {
					added, err := is.ib.Commit(tmp, is.hashes[i], en.fx.InboxRoots[i])
					if err != nil {
						panic(err)
					}
					acc = foldBool(acc, added)
				}
				for _, root := range en.fx.InboxRoots {
					is.ib.WaitFor(root)
					acc = foldKey(acc, root)
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*inboxState).close() },
		},
		Case{
			Group: "inbox", Op: "inbox.commit", Workload: "packs/duplicate",
			Threads: p.ThreadsMulti, Ops: p.InboxPacks,
			Dims:   Dims{Items: int64(p.InboxPacks), Objects: int64(p.InboxPacks * 32), Content: "structured"},
			PerRep: true,
			Setup: func(en *Env) any {
				// The idempotent path needs the first entry to still be
				// in the directory. Closing the inbox first retires the
				// workers, so the committed entries stay unprocessed and
				// the duplicate commit is deterministic in both cores
				// (Stage and Commit do not check the closed flag).
				is := newInbox(en, "inbox-commit-dup", en.Profile.ThreadsMulti)
				must(is.ib.Close())
				is.stageAll(en.fx)
				for i, tmp := range is.tmps {
					_, err := is.ib.Commit(tmp, is.hashes[i], en.fx.InboxRoots[i])
					must(err)
				}
				is.tmps, is.hashes = nil, nil
				is.stageAll(en.fx)
				return is
			},
			Run: func(en *Env, s any) uint64 {
				is := s.(*inboxState)
				acc := newFold()
				for i, tmp := range is.tmps {
					added, err := is.ib.Commit(tmp, is.hashes[i], en.fx.InboxRoots[i])
					if err != nil {
						panic(err)
					}
					acc = foldBool(acc, added)
				}
				return acc
			},
			Free: func(_ *Env, s any) { s.(*inboxState).close() },
		},
		Case{
			Group: "inbox", Op: "inbox.close", Workload: "drained", Threads: p.ThreadsMulti, Ops: 1,
			Dims:   Dims{Items: int64(p.InboxPacks), Objects: int64(p.InboxPacks * 32), Content: "structured"},
			PerRep: true,
			Setup: func(en *Env) any {
				is := newInbox(en, "inbox-close", en.Profile.ThreadsMulti)
				is.stageAll(en.fx)
				for i, tmp := range is.tmps {
					_, err := is.ib.Commit(tmp, is.hashes[i], en.fx.InboxRoots[i])
					must(err)
				}
				return is
			},
			Run: func(_ *Env, s any) uint64 {
				is := s.(*inboxState)
				if err := is.ib.Close(); err != nil {
					panic(err)
				}
				is.ib = nil
				return 1
			},
			Free: func(_ *Env, s any) { s.(*inboxState).close() },
		},
	)
	return out
}

func inboxBytes(fx *Fixtures) int64 {
	var n int64
	for _, b := range fx.InboxPacks {
		n += int64(len(b))
	}
	return n
}

func inboxChecks(e *Env) {
	fx := e.fx
	is := newInbox(e, "check-inbox", e.Profile.ThreadsMulti)
	defer is.close()

	// Staging hashes the body only, and the same body hashes the same way
	// in both cores — a wire-level contract.
	meta := inbox.Meta{Ref: "bench/inbox/check", Root: keyBytes(fx.InboxRoots[0]),
		ReceivedAt: 1_700_000_000_000_000_000}
	tmp, hash, n, err := is.ib.Stage(meta, bytes.NewReader(fx.InboxPacks[0]))
	e.want("inbox", "inbox.stage", "inbox/stage-hashes-body",
		err == nil && n == int64(len(fx.InboxPacks[0])) && len(hash) == 32,
		fmt.Sprintf("stage returned n=%d hash=%d (%v)", n, len(hash), err), "ok")
	// The body hash is the blake3 of the body, which is also what the Rust
	// core computes; comparing it is a genuine cross-core statement, but
	// the pack body itself embeds compressed payloads, so it is a
	// within-core anchor only.
	e.passLocal("inbox", "inbox.stage", "inbox/stage-body-hash", digestList([][]byte{hash}))

	added, err := is.ib.Commit(tmp, hash, fx.InboxRoots[0])
	e.want("inbox", "inbox.commit", "inbox/commit-adds", err == nil && added,
		fmt.Sprintf("commit reported added=%v (%v)", added, err), "true")
	is.ib.WaitFor(fx.InboxRoots[0])

	// The drained pack's objects are in the store.
	stored := 0
	r := readPackKeys(fx.InboxPacks[0])
	for _, k := range r {
		if ok, _ := is.store.st.Has(k); ok {
			stored++
		}
	}
	e.want("inbox", "inbox.drain", "inbox/drain-stores-objects", stored == len(r) && stored > 0,
		fmt.Sprintf("%d of %d objects reached the store", stored, len(r)),
		fmt.Sprintf("%d", stored))

	// Re-committing a body whose entry is still in the directory is
	// idempotent. A drained entry is deleted, so the check runs on a second
	// inbox whose workers have been retired: the first entry then stays
	// put and the duplicate is observable without racing a worker.
	idem := newInbox(e, "check-inbox-idem", e.Profile.ThreadsMulti)
	defer idem.close()
	must(idem.ib.Close())
	tmpA, hashA, _, err := idem.ib.Stage(meta, bytes.NewReader(fx.InboxPacks[0]))
	must(err)
	addedA, err := idem.ib.Commit(tmpA, hashA, fx.InboxRoots[0])
	tmpB, hashB, _, err2 := idem.ib.Stage(meta, bytes.NewReader(fx.InboxPacks[0]))
	must(err2)
	addedB, err3 := idem.ib.Commit(tmpB, hashB, fx.InboxRoots[0])
	_, statErr := os.Stat(tmpB)
	e.want("inbox", "inbox.commit", "inbox/commit-idempotent",
		err == nil && err3 == nil && addedA && !addedB && os.IsNotExist(statErr),
		fmt.Sprintf("first=%v second=%v (%v, %v), staged file still present: %v",
			addedA, addedB, err, err3, statErr == nil), "true/false")

	// Discard removes the staged file.
	tmp3, _, _, err := is.ib.Stage(meta, bytes.NewReader(fx.InboxPacks[1]))
	must(err)
	is.ib.Discard(tmp3)
	_, statErr3 := os.Stat(tmp3)
	e.want("inbox", "inbox.discard", "inbox/discard-removes-tmp", os.IsNotExist(statErr3),
		fmt.Sprintf("the discarded tmp file is still there: %v", statErr3), "removed")

	// WaitFor on an unknown root returns immediately.
	is.ib.WaitFor(fx.StoreMissKeys[0])
	e.pass("inbox", "inbox.wait_for", "inbox/wait-for-empty-group", "returns")

	// Open had no correctness evidence either. Its documented contract is
	// that a staged-but-uncommitted body left behind by a previous run is
	// swept when the inbox is opened again, so a crash between stage and
	// commit cannot accumulate garbage.
	sweep := newInbox(e, "check-inbox-sweep", e.Profile.ThreadsMulti)
	defer sweep.close()
	stale, _, _, serr := sweep.ib.Stage(meta, bytes.NewReader(fx.InboxPacks[0]))
	must(serr)
	must(sweep.ib.Close())
	sweep.ib = nil
	_, before := os.Stat(stale)
	reopened, oerr := inbox.Open(sweep.dir, sweep.store.st, e.Profile.ThreadsMulti, nil)
	must(oerr)
	sweep.ib = reopened
	_, after := os.Stat(stale)
	e.want("inbox", "inbox.open", "inbox/open-sweeps-staged-tmp-files",
		before == nil && os.IsNotExist(after),
		fmt.Sprintf("the staged body was %v before the reopen and %v after",
			before == nil, after == nil), "swept")

	// Close retires the workers after the entries they were given have been
	// drained: everything committed before the close is in the store, and
	// the inbox directory is left with nothing to sweep.
	fin := newInbox(e, "check-inbox-close", e.Profile.ThreadsMulti)
	defer fin.close()
	fin.stageAll(e.fx)
	for i, tmp := range fin.tmps {
		_, cerr := fin.ib.Commit(tmp, fin.hashes[i], fx.InboxRoots[i])
		must(cerr)
	}
	for _, root := range fx.InboxRoots {
		fin.ib.WaitFor(root)
	}
	must(fin.ib.Close())
	fin.ib = nil
	drained := 0
	for _, pack := range fx.InboxPacks {
		for _, k := range readPackKeys(pack) {
			if ok, _ := fin.store.st.Has(k); ok {
				drained++
			}
		}
	}
	total := 0
	for _, pack := range fx.InboxPacks {
		total += len(readPackKeys(pack))
	}
	leftovers := 0
	_ = filepath.Walk(fin.dir, func(p string, info os.FileInfo, err error) error {
		if err == nil && info != nil && !info.IsDir() {
			leftovers++
		}
		return nil
	})
	e.want("inbox", "inbox.close", "inbox/close-drains-then-retires",
		drained == total && total > 0 && leftovers == 0,
		fmt.Sprintf("%d of %d objects reached the store; %d files left in the inbox",
			drained, total, leftovers),
		fmt.Sprintf("%d", drained))

	// The Go-only gate option: WithGate brackets each entry's store write
	// with the collector's write gate. Timing an option constructor would
	// measure a closure allocation, so this is evidence rather than a case.
	var gateEntered atomic.Int64
	gated := newInboxWithGate(e, "check-inbox-gate", e.Profile.ThreadsMulti,
		inbox.WithGate(func() func() { gateEntered.Add(1); return func() {} }))
	defer gated.close()
	gated.stageAll(e.fx)
	for i, tmp := range gated.tmps {
		_, cerr := gated.ib.Commit(tmp, gated.hashes[i], fx.InboxRoots[i])
		must(cerr)
	}
	for _, root := range fx.InboxRoots {
		gated.ib.WaitFor(root)
	}
	e.wantLocal("inbox", "inbox.with_gate", "inbox/with-gate-brackets-writes",
		gateEntered.Load() > 0,
		fmt.Sprintf("the write gate was never entered while %d packs drained",
			len(fx.InboxPacks)),
		fmt.Sprintf("entered=%d", gateEntered.Load()))
}

// readPackKeys lists the keys a wire pack carries.
func readPackKeys(pack []byte) []key.Key {
	var out []key.Key
	r := amberpack.NewReader(bytes.NewReader(pack))
	for rec, err := range r.Records() {
		must(err)
		out = append(out, rec.Key)
	}
	return out
}

var _ = filepath.Join
var _ = packstore.ErrNotFound
