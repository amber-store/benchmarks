package main

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"

	"github.com/amber-store/core/amberignore"
	"github.com/amber-store/core/fstree"
	"github.com/amber-store/core/ingest"
	"github.com/amber-store/core/key"
	"github.com/amber-store/core/packstore"
	"github.com/amber-store/core/tarextract"
)

// ---------------------------------------------------------------------------
// amberignore
// ---------------------------------------------------------------------------

// ignoreNames is the deterministic name batch the matcher is asked about: a
// mix of ignored, negated and kept names, so the case is not dominated by one
// branch.
func ignoreNames() []string {
	var out []string
	for i := 0; i < 256; i++ {
		out = append(out,
			fmt.Sprintf("source%03d.c", i),
			fmt.Sprintf("object%03d.o", i),
			fmt.Sprintf("scratch%03d.tmp", i),
			"keep.tmp",
		)
	}
	return out
}

func ignoreCases(e *Env) []Case {
	names := ignoreNames()
	return []Case{
		{
			Group: "amberignore", Op: "amberignore.root", Workload: "load-root", Threads: 1, Ops: 64,
			Dims: Dims{Entries: 64, Content: "structured"}, CrossChecksum: true,
			Setup: func(en *Env) any { return en.fx.IgnoreDir },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for i := 0; i < 64; i++ {
					m, err := amberignore.Root(s.(string))
					if err != nil {
						panic(err)
					}
					// Probe the loaded matcher on both sides of each rule,
					// so the whole pattern set has to have been parsed.
					acc = foldBool(acc, m.Ignored("x.o", false))
					acc = foldBool(acc, m.Ignored("keep.tmp", false))
					acc = foldBool(acc, m.Ignored("vendor", true))
					blackBox(m)
				}
				return acc
			},
		},
		{
			Group: "amberignore", Op: "amberignore.descend", Workload: "8-subdirs", Threads: 1, Ops: 64 * 8,
			Dims: Dims{Entries: 64 * 8, Depth: 1, Content: "structured"}, CrossChecksum: true,
			Setup: func(en *Env) any { return mustV(amberignore.Root(en.fx.IgnoreDir)) },
			Run: func(en *Env, s any) uint64 {
				m := s.(*amberignore.Matcher)
				acc := newFold()
				for i := 0; i < 64; i++ {
					for d := 0; d < 8; d++ {
						name := fmt.Sprintf("sub%02d", d)
						sub, err := m.Descend(filepath.Join(en.fx.IgnoreDir, name), name)
						if err != nil {
							panic(err)
						}
						acc = foldBool(acc, sub.Ignored(fmt.Sprintf("gen%02d-a", d), false))
						acc = foldBool(acc, sub.Ignored(fmt.Sprintf("gen%02d-keep", d), false))
						blackBox(sub)
					}
				}
				return acc
			},
		},
		{
			Group: "amberignore", Op: "amberignore.ignored", Workload: "mixed-names", Threads: 1,
			Ops:  len(names),
			Dims: Dims{Entries: int64(len(names)), Content: "structured"}, CrossChecksum: true,
			Setup: func(en *Env) any { return mustV(amberignore.Root(en.fx.IgnoreDir)) },
			Run: func(_ *Env, s any) uint64 {
				m := s.(*amberignore.Matcher)
				acc := newFold()
				for _, n := range names {
					acc = foldBool(acc, m.Ignored(n, false))
				}
				return acc
			},
		},
		{
			Group: "amberignore", Op: "amberignore.ignored", Workload: "nil-matcher", Threads: 1,
			Ops:  len(names),
			Dims: Dims{Entries: int64(len(names)), Content: "structured"}, CrossChecksum: true,
			Setup: func(*Env) any { return (*amberignore.Matcher)(nil) },
			Run: func(_ *Env, s any) uint64 {
				m := s.(*amberignore.Matcher)
				acc := newFold()
				for _, n := range names {
					acc = foldBool(acc, m.Ignored(n, false))
				}
				return acc
			},
		},
	}
}

func ignoreChecks(e *Env) {
	fx := e.fx
	m, err := amberignore.Root(fx.IgnoreDir)
	if err != nil {
		e.fail("amberignore", "amberignore.root", "amberignore/root", err.Error())
		return
	}
	type probe struct {
		name  string
		isDir bool
		want  bool
	}
	probes := []probe{
		{"main.c", false, false},
		{"main.o", false, true},
		{"scratch.tmp", false, true},
		{"keep.tmp", false, false},
		{"vendor", true, true},
		{"target", true, true},
		{amberignore.FileName, false, false},
	}
	ok := true
	var got []string
	for _, p := range probes {
		g := m.Ignored(p.name, p.isDir)
		got = append(got, fmt.Sprintf("%s=%v", p.name, g))
		if g != p.want {
			ok = false
		}
	}
	e.want("amberignore", "amberignore.ignored", "amberignore/root-patterns", ok,
		fmt.Sprintf("root pattern results: %v", got), digestStrings(got))

	sub, err := m.Descend(filepath.Join(fx.IgnoreDir, "sub00"), "sub00")
	if err != nil {
		e.fail("amberignore", "amberignore.descend", "amberignore/descend", err.Error())
		return
	}
	var subGot []string
	subOK := sub.Ignored("gen00-a", false) &&
		!sub.Ignored("gen00-keep", false) &&
		sub.Ignored("x.o", false) // inherited from the root
	for _, n := range []string{"gen00-a", "gen00-keep", "x.o", "x.c"} {
		subGot = append(subGot, fmt.Sprintf("%s=%v", n, sub.Ignored(n, false)))
	}
	e.want("amberignore", "amberignore.descend", "amberignore/descend-composes", subOK,
		fmt.Sprintf("descended pattern results: %v", subGot), digestStrings(subGot))

	var nilm *amberignore.Matcher
	e.want("amberignore", "amberignore.ignored", "amberignore/nil-matcher-ignores-nothing",
		!nilm.Ignored("anything.o", false),
		"a nil matcher must ignore nothing", "false")
}

// ---------------------------------------------------------------------------
// ingest
// ---------------------------------------------------------------------------

func ingestCases(e *Env) []Case {
	p := e.Profile
	fx := e.fx
	var out []Case

	// Dimensions of an on-disk tree case. The counts come from the
	// fixture's scan, taken outside every measured interval.
	treeDims := func(c treeCounts) Dims {
		return Dims{Files: c.Files, ItemBytes: 0, Content: "tree", Entries: c.Files}
	}

	for _, jobs := range []int{p.ThreadsSingle, p.ThreadsMulti} {
		jobs := jobs
		out = append(out,
			// Scan walks directory entries and stats inodes; it never opens
			// a file body. Its byte figure is therefore the logical size of
			// the tree it covered, not bandwidth, and is labelled as such.
			Case{
				Group: "ingest", Op: "ingest.scan", Workload: fmt.Sprintf("filtered/jobs-%d", jobs),
				Threads: jobs, Ops: int(fx.V1Included.Files),
				Bytes: fx.V1Included.Bytes, BytesKind: BytesLogicalScanned,
				Dims:          treeDims(fx.V1Included),
				CrossChecksum: true,
				Setup:         func(en *Env) any { return en.fx.TreeV1 },
				Run: func(_ *Env, s any) uint64 {
					files, bytes, err := ingest.Scan(s.(string), false, jobs)
					if err != nil {
						panic(err)
					}
					return foldI64(foldI64(newFold(), int64(files)), int64(bytes))
				},
			},
			Case{
				Group: "ingest", Op: "ingest.objects", Workload: fmt.Sprintf("tree/jobs-%d", jobs),
				Threads: jobs, Ops: int(fx.V1Included.Files),
				Bytes: fx.V1Included.Bytes, BytesKind: BytesIncluded,
				Dims:          treeDims(fx.V1Included),
				CrossChecksum: true,
				Setup:         func(en *Env) any { return en.fx.TreeV1 },
				Run: func(_ *Env, s any) uint64 {
					seq, root, err := ingest.Objects(s.(string), ingest.Opts{Jobs: jobs})
					if err != nil {
						panic(err)
					}
					var n, count uint64
					for o, err := range seq {
						if err != nil {
							panic(err)
						}
						n += uint64(len(o.Bytes))
						count++
					}
					// The root key is the whole tree's identity: folding it
					// whole is 32 bytes, and it is the same in both cores.
					return foldKey(foldU64(foldU64(newFold(), count), n), *root)
				},
			},
			Case{
				Group: "ingest", Op: "ingest.dir", Workload: fmt.Sprintf("tree/fresh-store/jobs-%d", jobs),
				Threads: jobs, Ops: int(fx.V1Included.Files),
				Bytes: fx.V1Included.Bytes, BytesKind: BytesIncluded,
				Dims:          treeDims(fx.V1Included),
				PerRep:        true,
				CrossChecksum: true,
				Setup:         func(en *Env) any { return freshStore(en, "ingest-dir") },
				Run: func(en *Env, s any) uint64 {
					h := s.(*storeHandle)
					root, stats, err := ingest.Dir(h.st, en.fx.TreeV1, ingest.Opts{Jobs: jobs})
					if err != nil {
						panic(err)
					}
					return foldKey(foldI64(foldI64(newFold(),
						int64(stats.Stored)), int64(stats.Deduped)), root)
				},
				Free: func(_ *Env, s any) { s.(*storeHandle).close() },
			},
			// The successor tree covers different files and different
			// bytes, so it gets its own denominator rather than the first
			// tree's.
			Case{
				Group: "ingest", Op: "ingest.dir", Workload: fmt.Sprintf("tree/incremental-change/jobs-%d", jobs),
				Threads: jobs, Ops: int(fx.V2Included.Files),
				Bytes: fx.V2Included.Bytes, BytesKind: BytesIncluded,
				Dims:          treeDims(fx.V2Included),
				PerRep:        true,
				CrossChecksum: true,
				Setup:         func(en *Env) any { return copiedStore(en, en.fx.IngestTemplate, "ingest-inc") },
				Run: func(en *Env, s any) uint64 {
					h := s.(*storeHandle)
					root, stats, err := ingest.Dir(h.st, en.fx.TreeV2, ingest.Opts{Jobs: jobs})
					if err != nil {
						panic(err)
					}
					return foldKey(foldI64(foldI64(newFold(),
						int64(stats.Stored)), int64(stats.Deduped)), root)
				},
				Free: func(_ *Env, s any) { s.(*storeHandle).close() },
			},
			// Re-ingesting an unchanged tree is the common case in
			// practice and the pure dedup path: the same files are walked
			// and chunked, and nothing at all is stored.
			Case{
				Group: "ingest", Op: "ingest.dir", Workload: fmt.Sprintf("tree/unchanged-repeat/jobs-%d", jobs),
				Threads: jobs, Ops: int(fx.V1Included.Files),
				Bytes: fx.V1Included.Bytes, BytesKind: BytesIncluded,
				Dims:          treeDims(fx.V1Included),
				PerRep:        true,
				CrossChecksum: true,
				Setup:         func(en *Env) any { return copiedStore(en, en.fx.IngestTemplate, "ingest-same") },
				Run: func(en *Env, s any) uint64 {
					h := s.(*storeHandle)
					root, stats, err := ingest.Dir(h.st, en.fx.TreeV1, ingest.Opts{Jobs: jobs})
					if err != nil {
						panic(err)
					}
					if stats.Stored != 0 {
						panic(fmt.Sprintf("unchanged repeat stored %d objects", stats.Stored))
					}
					return foldKey(foldI64(foldI64(newFold(),
						int64(stats.Stored)), int64(stats.Deduped)), root)
				},
				Free: func(_ *Env, s any) { s.(*storeHandle).close() },
			},
		)
	}
	out = append(out,
		// The same walk with the ignore rules disabled covers strictly more
		// files and more bytes, and says so in its own denominator.
		Case{
			Group: "ingest", Op: "ingest.scan", Workload: "unfiltered/jobs-1", Threads: 1,
			Ops:   int(fx.V1Unfiltered.Files),
			Bytes: fx.V1Unfiltered.Bytes, BytesKind: BytesLogicalScanned,
			Dims:          treeDims(fx.V1Unfiltered),
			CrossChecksum: true,
			Setup:         func(en *Env) any { return en.fx.TreeV1 },
			Run: func(_ *Env, s any) uint64 {
				files, bytes, err := ingest.Scan(s.(string), true, 1)
				if err != nil {
					panic(err)
				}
				return foldI64(foldI64(newFold(), int64(files)), int64(bytes))
			},
		},
		Case{
			Group: "ingest", Op: "ingest.dir", Workload: "single-file/jobs-1", Threads: 1,
			Ops: 1, Bytes: e.Profile.CorpusBytes / 8, BytesKind: BytesIncluded,
			Dims:   Dims{Files: 1, ItemBytes: e.Profile.CorpusBytes / 8, Content: "text"},
			PerRep: true, CrossChecksum: true,
			Setup: func(en *Env) any { return freshStore(en, "ingest-file") },
			Run: func(en *Env, s any) uint64 {
				h := s.(*storeHandle)
				root, stats, err := ingest.Dir(h.st, filepath.Join(en.fx.TreeV1, "big.bin"), ingest.Opts{Jobs: 1})
				if err != nil {
					panic(err)
				}
				return foldKey(foldI64(newFold(), int64(stats.Stored)), root)
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
	)
	return out
}

func ingestChecks(e *Env) {
	fx := e.fx
	p := e.Profile

	// The counts the rate denominators use were taken at fixture-build
	// time; assert here that they are what they claim to be, and record
	// them as comparable digests so a difference between the two cores'
	// denominators fails the run instead of silently skewing a rate.
	e.pass("ingest", "ingest.scan", "ingest/scan-filtered",
		fmt.Sprintf("%d/%d", fx.V1Included.Files, fx.V1Included.Bytes))
	e.want("ingest", "ingest.scan", "ingest/scan-unfiltered",
		fx.V1Unfiltered.Files > fx.V1Included.Files && fx.V1Unfiltered.Bytes > fx.V1Included.Bytes,
		fmt.Sprintf("ignoring .amberignore must see more files: %d/%d vs %d/%d",
			fx.V1Unfiltered.Files, fx.V1Unfiltered.Bytes, fx.V1Included.Files, fx.V1Included.Bytes),
		fmt.Sprintf("%d/%d", fx.V1Unfiltered.Files, fx.V1Unfiltered.Bytes))
	e.pass("ingest", "ingest.scan", "ingest/scan-v2-included",
		fmt.Sprintf("%d/%d", fx.V2Included.Files, fx.V2Included.Bytes))
	// The worker count must not change the answer.
	filesN, bytesN, err3 := ingest.Scan(fx.TreeV1, false, p.ThreadsMulti)
	e.want("ingest", "ingest.scan", "ingest/scan-jobs-invariant",
		err3 == nil && int64(filesN) == fx.V1Included.Files && int64(bytesN) == fx.V1Included.Bytes,
		fmt.Sprintf("jobs changed the scan totals: %d/%d vs %d/%d",
			filesN, bytesN, fx.V1Included.Files, fx.V1Included.Bytes),
		fmt.Sprintf("%d/%d", filesN, bytesN))

	// The object stream and the store path must agree on the root, and the
	// root must not depend on the worker count.
	var streamed []key.Key
	seq, rootPtr, err := ingest.Objects(fx.TreeV1, ingest.Opts{Jobs: 1})
	if err != nil {
		e.fail("ingest", "ingest.objects", "ingest/objects", err.Error())
		return
	}
	mem := newMemStore()
	for o, err := range seq {
		if err != nil {
			e.fail("ingest", "ingest.objects", "ingest/objects", err.Error())
			return
		}
		streamed = append(streamed, o.Key)
		must(mem.put(o))
	}
	root1 := *rootPtr
	e.pass("ingest", "ingest.objects", "ingest/objects-root", root1.String())

	seq2, rootPtr2, err := ingest.Objects(fx.TreeV1, ingest.Opts{Jobs: p.ThreadsMulti})
	must(err)
	n2 := 0
	for _, err := range seq2 {
		must(err)
		n2++
	}
	e.want("ingest", "ingest.objects", "ingest/objects-jobs-invariant",
		*rootPtr2 == root1 && n2 == len(streamed),
		fmt.Sprintf("worker count changed the build: root %s vs %s, %d vs %d objects",
			rootPtr2.String(), root1.String(), n2, len(streamed)),
		rootPtr2.String())

	// The ingested tree must be complete and must read back as the source.
	vis, err := fstree.CheckComplete(root1, mem.get, mem.has, 1)
	e.want("ingest", "ingest.objects", "ingest/objects-complete", err == nil && len(vis) > 0,
		fmt.Sprintf("the streamed object set is not self-contained: %v", err),
		fmt.Sprintf("n=%d", len(vis)))

	// The whole restored tree, not a spot check. The archive is exported
	// from the ingested root, extracted into an empty directory, and the
	// complete listing of what comes out is compared against the listing
	// the harness derived from the source tree and the fixture's own ignore
	// rules. Nothing in this comparison is taken from either core.
	restored := workDir(e, "check-restore")
	defer os.RemoveAll(restored)
	var tarBuf bytes.Buffer
	if err := tarWrite(&tarBuf, root1, mem.get); err != nil {
		e.fail("ingest", "ingest.objects", "ingest/restored-tree-matches-source", err.Error())
	} else if err := tarextract.Extract(bytes.NewReader(tarBuf.Bytes()), restored); err != nil {
		e.fail("ingest", "ingest.objects", "ingest/restored-tree-matches-source", err.Error())
	} else {
		got, merr := manifestLines(restored, nil)
		want := fx.V1IncludedManifest
		ok := merr == nil && len(want) > 0 && len(got) == len(want)
		if ok {
			for i := range want {
				if want[i] != got[i] {
					ok = false
					break
				}
			}
		}
		e.want("ingest", "ingest.objects", "ingest/restored-tree-matches-source", ok,
			fmt.Sprintf("the restored tree is not the included source tree: %v; %s",
				merr, firstDifference(want, got)),
			digestStrings(got))
		// And the expectation itself is a comparable statement, so the two
		// cores are shown to have started from the same source tree.
		e.pass("ingest", "ingest.objects", "ingest/included-source-manifest", digestStrings(want))
	}

	ent, err := fstree.ResolveEntry(root1, "data/d000/f000.bin", mem.get)
	e.want("ingest", "ingest.objects", "ingest/objects-resolves-file",
		err == nil && ent != nil && ent.Mode&0o170000 == 0o100000,
		fmt.Sprintf("resolving an ingested file: %v", err), "ok")
	// The .amberignore rules really applied.
	_, err = fstree.ResolveEntry(root1, "build", mem.get)
	e.want("ingest", "ingest.objects", "ingest/objects-honours-ignore", err != nil,
		"the ignored build/ directory was ingested", "absent")
	ent, err = fstree.ResolveEntry(root1, "data/d000/keep.tmp", mem.get)
	e.want("ingest", "ingest.objects", "ingest/objects-honours-negation", err == nil && ent != nil,
		fmt.Sprintf("the negated keep.tmp was excluded: %v", err), "present")

	// ingest.Dir must reach the same root through the store path, and a
	// second ingest of the same tree must dedup completely.
	dir := filepath.Join(e.Scratch, "check-ingest-dir")
	must(os.RemoveAll(dir))
	must(os.MkdirAll(dir, 0o755))
	st := mustV(packstore.Open(dir, packstore.WithSync(false)))
	root2, stats, err := ingest.Dir(st, fx.TreeV1, ingest.Opts{Jobs: p.ThreadsMulti})
	e.want("ingest", "ingest.dir", "ingest/dir-root", err == nil && root2 == root1,
		fmt.Sprintf("ingest.Dir root %s differs from the stream root %s (%v)", root2, root1, err),
		root2.String())
	// This is the unchanged-repeat workload's contract: the same tree,
	// ingested again, stores nothing at all.
	_, stats2, err := ingest.Dir(st, fx.TreeV1, ingest.Opts{Jobs: p.ThreadsMulti})
	e.want("ingest", "ingest.dir", "ingest/dir-unchanged-repeat-stores-nothing",
		err == nil && stats2.Stored == 0 && stats2.Deduped == stats.Stored+stats.Deduped,
		fmt.Sprintf("a repeat ingest stored %d objects (first run stored %d)", stats2.Stored, stats.Stored),
		fmt.Sprintf("stored=%d deduped=%d", stats2.Stored, stats2.Deduped))
	// The incremental change must store strictly less than a fresh tree.
	rootV2, stats3, err := ingest.Dir(st, fx.TreeV2, ingest.Opts{Jobs: p.ThreadsMulti})
	e.want("ingest", "ingest.dir", "ingest/dir-incremental",
		err == nil && stats3.Stored > 0 && stats3.Stored < stats.Stored && rootV2 != root1,
		fmt.Sprintf("incremental ingest stored %d of the original %d objects", stats3.Stored, stats.Stored),
		fmt.Sprintf("stored=%d", stats3.Stored))
	e.pass("ingest", "ingest.dir", "ingest/dir-v2-root", rootV2.String())
	must(st.Close())
	must(os.RemoveAll(dir))
}
