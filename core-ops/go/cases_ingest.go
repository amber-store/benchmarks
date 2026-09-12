package main

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/amber-store/core/amberignore"
	"github.com/amber-store/core/fstree"
	"github.com/amber-store/core/ingest"
	"github.com/amber-store/core/key"
	"github.com/amber-store/core/packstore"
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
			Setup: func(en *Env) any { return en.fx.IgnoreDir },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for i := 0; i < 64; i++ {
					m, err := amberignore.Root(s.(string))
					if err != nil {
						panic(err)
					}
					if m.Ignored("x.o", false) {
						acc++
					}
				}
				return acc
			},
		},
		{
			Group: "amberignore", Op: "amberignore.descend", Workload: "8-subdirs", Threads: 1, Ops: 64 * 8,
			Setup: func(en *Env) any { return mustV(amberignore.Root(en.fx.IgnoreDir)) },
			Run: func(en *Env, s any) uint64 {
				m := s.(*amberignore.Matcher)
				var acc uint64
				for i := 0; i < 64; i++ {
					for d := 0; d < 8; d++ {
						name := fmt.Sprintf("sub%02d", d)
						sub, err := m.Descend(filepath.Join(en.fx.IgnoreDir, name), name)
						if err != nil {
							panic(err)
						}
						if sub.Ignored(fmt.Sprintf("gen%02d-a", d), false) {
							acc++
						}
					}
				}
				return acc
			},
		},
		{
			Group: "amberignore", Op: "amberignore.ignored", Workload: "mixed-names", Threads: 1,
			Ops:   len(names),
			Setup: func(en *Env) any { return mustV(amberignore.Root(en.fx.IgnoreDir)) },
			Run: func(_ *Env, s any) uint64 {
				m := s.(*amberignore.Matcher)
				var acc uint64
				for _, n := range names {
					if m.Ignored(n, false) {
						acc++
					}
				}
				return acc
			},
		},
		{
			Group: "amberignore", Op: "amberignore.ignored", Workload: "nil-matcher", Threads: 1,
			Ops:   len(names),
			Setup: func(*Env) any { return (*amberignore.Matcher)(nil) },
			Run: func(_ *Env, s any) uint64 {
				m := s.(*amberignore.Matcher)
				var acc uint64
				for _, n := range names {
					if m.Ignored(n, false) {
						acc++
					}
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

	for _, jobs := range []int{p.ThreadsSingle, p.ThreadsMulti} {
		jobs := jobs
		out = append(out,
			Case{
				Group: "ingest", Op: "ingest.scan", Workload: fmt.Sprintf("tree/jobs-%d", jobs),
				Threads: jobs, Ops: 1, Bytes: fx.TreeBytes,
				Setup: func(en *Env) any { return en.fx.TreeV1 },
				Run: func(_ *Env, s any) uint64 {
					files, bytes, err := ingest.Scan(s.(string), false, jobs)
					if err != nil {
						panic(err)
					}
					return uint64(files) + uint64(bytes)
				},
			},
			Case{
				Group: "ingest", Op: "ingest.objects", Workload: fmt.Sprintf("tree/jobs-%d", jobs),
				Threads: jobs, Ops: 1, Bytes: fx.TreeBytes,
				Setup: func(en *Env) any { return en.fx.TreeV1 },
				Run: func(_ *Env, s any) uint64 {
					seq, root, err := ingest.Objects(s.(string), ingest.Opts{Jobs: jobs})
					if err != nil {
						panic(err)
					}
					var n uint64
					for o, err := range seq {
						if err != nil {
							panic(err)
						}
						n += uint64(len(o.Bytes))
					}
					return n + uint64((*root)[0])
				},
			},
			Case{
				Group: "ingest", Op: "ingest.dir", Workload: fmt.Sprintf("tree/fresh-store/jobs-%d", jobs),
				Threads: jobs, Ops: 1, Bytes: fx.TreeBytes, PerRep: true,
				Setup: func(en *Env) any { return freshStore(en, "ingest-dir") },
				Run: func(en *Env, s any) uint64 {
					h := s.(*storeHandle)
					root, stats, err := ingest.Dir(h.st, en.fx.TreeV1, ingest.Opts{Jobs: jobs})
					if err != nil {
						panic(err)
					}
					return uint64(stats.Stored) + uint64(root[0])
				},
				Free: func(_ *Env, s any) { s.(*storeHandle).close() },
			},
			Case{
				Group: "ingest", Op: "ingest.dir", Workload: fmt.Sprintf("tree/incremental-change/jobs-%d", jobs),
				Threads: jobs, Ops: 1, Bytes: fx.TreeBytes, PerRep: true,
				Setup: func(en *Env) any { return copiedStore(en, en.fx.IngestTemplate, "ingest-inc") },
				Run: func(en *Env, s any) uint64 {
					h := s.(*storeHandle)
					root, stats, err := ingest.Dir(h.st, en.fx.TreeV2, ingest.Opts{Jobs: jobs})
					if err != nil {
						panic(err)
					}
					return uint64(stats.Stored) + uint64(stats.Deduped) + uint64(root[0])
				},
				Free: func(_ *Env, s any) { s.(*storeHandle).close() },
			},
		)
	}
	out = append(out,
		Case{
			Group: "ingest", Op: "ingest.scan", Workload: "tree/no-ignore/jobs-1", Threads: 1,
			Ops: 1, Bytes: fx.TreeBytes,
			Setup: func(en *Env) any { return en.fx.TreeV1 },
			Run: func(_ *Env, s any) uint64 {
				files, bytes, err := ingest.Scan(s.(string), true, 1)
				if err != nil {
					panic(err)
				}
				return uint64(files) + uint64(bytes)
			},
		},
		Case{
			Group: "ingest", Op: "ingest.dir", Workload: "single-file/jobs-1", Threads: 1,
			Ops: 1, Bytes: e.Profile.CorpusBytes / 8, PerRep: true,
			Setup: func(en *Env) any { return freshStore(en, "ingest-file") },
			Run: func(en *Env, s any) uint64 {
				h := s.(*storeHandle)
				root, _, err := ingest.Dir(h.st, filepath.Join(en.fx.TreeV1, "big.bin"), ingest.Opts{Jobs: 1})
				if err != nil {
					panic(err)
				}
				return uint64(root[0])
			},
			Free: func(_ *Env, s any) { s.(*storeHandle).close() },
		},
	)
	return out
}

func ingestChecks(e *Env) {
	fx := e.fx
	p := e.Profile

	files, bytesTotal, err := ingest.Scan(fx.TreeV1, false, 1)
	e.want("ingest", "ingest.scan", "ingest/scan-filtered", err == nil && files > 0 && bytesTotal > 0,
		fmt.Sprintf("scan failed: %v", err), fmt.Sprintf("%d/%d", files, bytesTotal))
	filesAll, bytesAll, err2 := ingest.Scan(fx.TreeV1, true, 1)
	e.want("ingest", "ingest.scan", "ingest/scan-unfiltered", err2 == nil && filesAll > files && bytesAll > bytesTotal,
		fmt.Sprintf("ignoring .amberignore must see more files: %d/%d vs %d/%d",
			filesAll, bytesAll, files, bytesTotal),
		fmt.Sprintf("%d/%d", filesAll, bytesAll))
	// The worker count must not change the answer.
	filesN, bytesN, err3 := ingest.Scan(fx.TreeV1, false, p.ThreadsMulti)
	e.want("ingest", "ingest.scan", "ingest/scan-jobs-invariant",
		err3 == nil && filesN == files && bytesN == bytesTotal,
		fmt.Sprintf("jobs changed the scan totals: %d/%d vs %d/%d", filesN, bytesN, files, bytesTotal),
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
	_, stats2, err := ingest.Dir(st, fx.TreeV1, ingest.Opts{Jobs: p.ThreadsMulti})
	e.want("ingest", "ingest.dir", "ingest/dir-dedups",
		err == nil && stats2.Stored == 0 && stats2.Deduped == stats.Stored+stats.Deduped,
		fmt.Sprintf("a repeat ingest stored %d objects (first run stored %d)", stats2.Stored, stats.Stored),
		fmt.Sprintf("stored=%d deduped=%d", stats2.Stored, stats2.Deduped))
	// The incremental change must store strictly less than a fresh tree.
	_, stats3, err := ingest.Dir(st, fx.TreeV2, ingest.Opts{Jobs: p.ThreadsMulti})
	e.want("ingest", "ingest.dir", "ingest/dir-incremental",
		err == nil && stats3.Stored > 0 && stats3.Stored < stats.Stored,
		fmt.Sprintf("incremental ingest stored %d of the original %d objects", stats3.Stored, stats.Stored),
		fmt.Sprintf("stored=%d", stats3.Stored))
	must(st.Close())
	must(os.RemoveAll(dir))
}
