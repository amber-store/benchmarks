package main

import (
	"bytes"
	"errors"
	"fmt"
	"io"

	"github.com/amber-store/core/chunkers"
	"github.com/amber-store/core/fstree"
	"github.com/amber-store/core/ingest"
	"github.com/amber-store/core/key"
)

// countingSink counts built objects without retaining them, so a builder case
// measures the builder rather than a map insert.
type countingSink struct {
	n     int
	bytes int64
}

func (c *countingSink) emit(o fstree.Object) error {
	c.n++
	c.bytes += int64(len(o.Bytes))
	return nil
}

func fstreeCases(e *Env) []Case {
	fx := e.fx
	reps := 64
	var out []Case

	// --- encoders: the payload-size sweep -------------------------------
	for _, ps := range fx.Payloads {
		ps := ps
		lim := ps.limit(4 << 20)
		out = append(out, Case{
			Group: "fstree", Op: "fstree.encode_blob", Workload: lim.Name, Threads: 1,
			Ops: len(lim.Items), Bytes: lim.Bytes, BytesKind: BytesPayload,
			Dims: lim.dims(), CrossChecksum: true,
			Setup: func(*Env) any { return lim },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for _, b := range s.(payloadSet).Items {
					o, err := fstree.EncodeBlob(b)
					if err != nil {
						panic(err)
					}
					// The whole content key, so the hash is observed, and
					// the encoded object's ends, without a second pass.
					acc = foldKey(acc, o.Key)
					acc = sinkBytes(acc, o.Bytes)
				}
				return acc
			},
		})
	}

	// --- node codecs: the entry-count sweep -----------------------------
	// Every point differs from its neighbours in one number only, so the
	// per-entry cost of an encode or a decode can be read off the row.
	for _, es := range fx.EntrySets {
		es := es
		dims := Dims{Entries: int64(len(es.Entries)), Items: int64(reps),
			ItemBytes: int64(len(es.Enc)), Content: es.Content, Shape: "leaf"}
		out = append(out,
			Case{
				Group: "fstree", Op: "fstree.encode_dir_leaf", Workload: es.Name, Threads: 1,
				Ops: reps * len(es.Entries), Bytes: int64(reps * len(es.Enc)), BytesKind: BytesEncoded,
				Dims: dims, CrossChecksum: true,
				Setup: func(*Env) any { return es.Entries },
				Run: func(_ *Env, st any) uint64 {
					acc := newFold()
					for i := 0; i < reps; i++ {
						o, err := fstree.EncodeDirLeaf(st.([]fstree.Entry))
						if err != nil {
							panic(err)
						}
						acc = foldKey(acc, o.Key)
						acc = sinkBytes(acc, o.Bytes)
					}
					return acc
				},
			},
			Case{
				Group: "fstree", Op: "fstree.decode_dir_leaf", Workload: es.Name, Threads: 1,
				Ops: reps * len(es.Entries), Bytes: int64(reps * len(es.Enc)), BytesKind: BytesEncoded,
				Dims: dims, CrossChecksum: true,
				Setup: func(*Env) any { return es.Enc },
				Run: func(_ *Env, st any) uint64 {
					acc := newFold()
					for i := 0; i < reps; i++ {
						ents, err := fstree.DecodeDirLeaf(st.([]byte))
						if err != nil {
							panic(err)
						}
						acc = foldU64(acc, uint64(len(ents)))
						for _, en := range ents {
							// Each decoded entry's fields: names and keys
							// are short, so this stays proportional to the
							// entry count rather than to the payload.
							acc = sinkBytes(acc, en.Name)
							acc = sinkBytes(acc, en.ContentKey)
							acc = foldU64(acc, uint64(en.Mode))
							acc = foldI64(acc, en.Mtime)
						}
					}
					return acc
				},
			},
		)
	}
	for _, pset := range fx.PairSets {
		pset := pset
		dims := Dims{Entries: int64(len(pset.Pairs)), Items: int64(reps),
			ItemBytes: int64(len(pset.Enc)), Content: "structured", Shape: "index"}
		out = append(out,
			Case{
				Group: "fstree", Op: "fstree.encode_dir_node", Workload: pset.Name, Threads: 1,
				Ops: reps * len(pset.Pairs), Bytes: int64(reps * len(pset.Enc)), BytesKind: BytesEncoded,
				Dims: dims, CrossChecksum: true,
				Setup: func(*Env) any { return pset.Pairs },
				Run: func(_ *Env, st any) uint64 {
					acc := newFold()
					for i := 0; i < reps; i++ {
						o, err := fstree.EncodeDirNode(st.([]fstree.DirPair))
						if err != nil {
							panic(err)
						}
						acc = foldKey(acc, o.Key)
						acc = sinkBytes(acc, o.Bytes)
					}
					return acc
				},
			},
			Case{
				Group: "fstree", Op: "fstree.decode_dir_node", Workload: pset.Name, Threads: 1,
				Ops: reps * len(pset.Pairs), Bytes: int64(reps * len(pset.Enc)), BytesKind: BytesEncoded,
				Dims: dims, CrossChecksum: true,
				Setup: func(*Env) any { return pset.Enc },
				Run: func(_ *Env, st any) uint64 {
					acc := newFold()
					for i := 0; i < reps; i++ {
						prs, err := fstree.DecodeDirNode(st.([]byte))
						if err != nil {
							panic(err)
						}
						acc = foldU64(acc, uint64(len(prs)))
						for _, pr := range prs {
							acc = sinkBytes(acc, pr.SepName)
							acc = sinkBytes(acc, pr.ChildKey)
						}
					}
					return acc
				},
			},
		)
	}
	for _, cs := range fx.ChildSets {
		cs := cs
		dims := Dims{Entries: int64(len(cs.Keys)), Items: int64(reps),
			ItemBytes: int64(len(cs.Enc)), Content: "structured", Shape: "file-index"}
		out = append(out,
			Case{
				Group: "fstree", Op: "fstree.encode_file_node", Workload: cs.Name, Threads: 1,
				Ops: reps * len(cs.Keys), Bytes: int64(reps * len(cs.Enc)), BytesKind: BytesEncoded,
				Dims: dims, CrossChecksum: true,
				Setup: func(*Env) any { return cs.Keys },
				Run: func(_ *Env, st any) uint64 {
					acc := newFold()
					for i := 0; i < reps; i++ {
						o, err := fstree.EncodeFileNode(st.([]key.Key))
						if err != nil {
							panic(err)
						}
						acc = foldKey(acc, o.Key)
						acc = sinkBytes(acc, o.Bytes)
					}
					return acc
				},
			},
			Case{
				Group: "fstree", Op: "fstree.decode_file_node", Workload: cs.Name, Threads: 1,
				Ops: reps * len(cs.Keys), Bytes: int64(reps * len(cs.Enc)), BytesKind: BytesEncoded,
				Dims: dims, CrossChecksum: true,
				Setup: func(*Env) any { return cs.Enc },
				Run: func(_ *Env, st any) uint64 {
					acc := newFold()
					for i := 0; i < reps; i++ {
						ks, err := fstree.DecodeFileNode(st.([]byte))
						if err != nil {
							panic(err)
						}
						acc = foldU64(acc, uint64(len(ks)))
						for _, k := range ks {
							acc = foldKey(acc, k)
						}
					}
					return acc
				},
			},
		)
	}
	out = append(out, Case{
		Group: "fstree", Op: "fstree.encode_xattr_set", Workload: "xattrs-64", Threads: 1,
		Ops:           reps * len(fx.XattrsLarge),
		Dims:          Dims{Entries: int64(len(fx.XattrsLarge)), Items: int64(reps), Content: "structured"},
		CrossChecksum: true,
		Setup:         func(*Env) any { return nil },
		Run: func(en *Env, _ any) uint64 {
			acc := newFold()
			for i := 0; i < reps; i++ {
				o, err := fstree.EncodeXattrSet(en.fx.XattrsLarge)
				if err != nil {
					panic(err)
				}
				acc = foldKey(acc, o.Key)
				acc = sinkBytes(acc, o.Bytes)
			}
			return acc
		},
	})

	// --- child keys ---------------------------------------------------
	childInputs := []struct {
		name    string
		entries int64
		k       func(*Env) key.Key
		b       func(*Env) []byte
	}{
		{"dir-leaf-128", 128, func(en *Env) key.Key { return mustV(fstree.EncodeDirLeaf(en.fx.EntriesLarge)).Key },
			func(en *Env) []byte { return en.fx.EncDirLeafLarge }},
		{"dir-node-128", 128, func(en *Env) key.Key { return mustV(fstree.EncodeDirNode(en.fx.PairsLarge)).Key },
			func(en *Env) []byte { return en.fx.EncDirNodeLarge }},
		{"file-node-1024", 1024, func(en *Env) key.Key { return mustV(fstree.EncodeFileNode(en.fx.ChildrenLarge)).Key },
			func(en *Env) []byte { return en.fx.EncFileNodeLarge }},
	}
	for _, ci := range childInputs {
		ci := ci
		out = append(out, Case{
			Group: "fstree", Op: "fstree.child_keys", Workload: ci.name, Threads: 1, Ops: reps,
			Dims:          Dims{Entries: ci.entries, Items: int64(reps), Content: "structured"},
			CrossChecksum: true,
			Setup:         func(en *Env) any { return [2]any{ci.k(en), ci.b(en)} },
			Run: func(_ *Env, s any) uint64 {
				pair := s.([2]any)
				acc := newFold()
				for i := 0; i < reps; i++ {
					ks, err := fstree.ChildKeys(pair[0].(key.Key), pair[1].([]byte))
					if err != nil {
						panic(err)
					}
					acc = foldU64(acc, uint64(len(ks)))
					for _, k := range ks {
						acc = foldKey(acc, k)
					}
				}
				return acc
			},
		})
	}

	// --- builders: the entry-count sweep --------------------------------
	for _, n := range e.Profile.TreeWidths {
		n := n
		out = append(out, Case{
			Group: "fstree", Op: "fstree.dir_builder", Workload: fmt.Sprintf("entries-%d", n), Threads: 1,
			Ops:           n,
			Dims:          Dims{Entries: int64(n), Width: int64(n), Depth: 1, Shape: "wide", Content: "structured"},
			CrossChecksum: true,
			Setup: func(en *Env) any {
				entries := make([]fstree.Entry, 0, n)
				for i := 0; i < n; i++ {
					entries = append(entries, entryFor(i, en.Profile.Seed+70, nil))
				}
				return entries
			},
			Run: func(en *Env, s any) uint64 {
				sink := &countingSink{}
				db := fstree.NewDirBuilder(chunkers.NewItemChunker(ingest.DefaultItemBits))
				for _, en2 := range s.([]fstree.Entry) {
					if err := db.AddEntry(sink.emit, en2); err != nil {
						panic(err)
					}
				}
				root, err := db.Finish(sink.emit)
				if err != nil {
					panic(err)
				}
				return foldKey(foldI64(foldI64(newFold(), int64(sink.n)), sink.bytes), root)
			},
		})
	}
	for _, n := range e.Profile.FanOuts {
		n := n
		out = append(out, Case{
			Group: "fstree", Op: "fstree.index_builder_file", Workload: fmt.Sprintf("children-%d", n), Threads: 1,
			Ops:           n,
			Dims:          Dims{Entries: int64(n), Width: int64(n), Shape: "file-index", Content: "structured"},
			CrossChecksum: true,
			Setup: func(en *Env) any {
				ks := make([]key.Key, 0, n)
				for i := 0; i < n; i++ {
					var h [32]byte
					copy(h[:], randomBytes(en.Profile.Seed+90+uint64(i), 32))
					ks = append(ks, mustV(key.NewFromHash(key.Blob, 65536, h)))
				}
				return ks
			},
			Run: func(en *Env, s any) uint64 {
				sink := &countingSink{}
				ib := fstree.NewFileIndexBuilder(chunkers.NewItemChunker(ingest.DefaultItemBits))
				for _, k := range s.([]key.Key) {
					if err := ib.AddChild(sink.emit, k, nil); err != nil {
						panic(err)
					}
				}
				root, err := ib.Finish(sink.emit)
				if err != nil {
					panic(err)
				}
				return foldKey(foldI64(foldI64(newFold(), int64(sink.n)), sink.bytes), root)
			},
		})
	}

	// --- read paths: the directory-width sweep --------------------------
	lookups := e.Profile.BatchOps
	for _, d := range fx.Dirs {
		d := d
		wdims := Dims{Entries: int64(d.Entries), Width: int64(d.Entries), Depth: 1,
			Shape: "wide", Content: "structured", Objects: d.Objects}
		out = append(out,
			Case{
				Group: "fstree", Op: "fstree.lookup_entry",
				Workload: fmt.Sprintf("width-%d/hit", d.Entries), Threads: 1, Ops: lookups,
				Dims: wdims, CrossChecksum: true,
				Setup: func(*Env) any { return nil },
				Run: func(en *Env, _ any) uint64 {
					acc := newFold()
					for i := 0; i < lookups; i++ {
						ent, err := fstree.LookupEntry(d.Root, d.Names[(i*7919)%len(d.Names)], en.fx.Mem.get)
						if err != nil {
							panic(err)
						}
						acc = sinkBytes(acc, ent.Name)
						acc = sinkBytes(acc, ent.ContentKey)
					}
					return acc
				},
			},
			Case{
				Group: "fstree", Op: "fstree.lookup_entry",
				Workload: fmt.Sprintf("width-%d/miss", d.Entries), Threads: 1, Ops: lookups,
				Dims: wdims, CrossChecksum: true,
				Setup: func(*Env) any { return nil },
				Run: func(en *Env, _ any) uint64 {
					acc := newFold()
					for i := 0; i < lookups; i++ {
						_, err := fstree.LookupEntry(d.Root, d.MissName, en.fx.Mem.get)
						acc = foldBool(acc, errors.Is(err, fstree.ErrNotFound))
					}
					return acc
				},
			},
			Case{
				Group: "fstree", Op: "fstree.collect_entries",
				Workload: fmt.Sprintf("width-%d", d.Entries), Threads: 1, Ops: d.Entries,
				Dims: wdims, CrossChecksum: true,
				Setup: func(*Env) any { return nil },
				Run: func(en *Env, _ any) uint64 {
					es, err := fstree.CollectEntries(d.Root, en.fx.Mem.get)
					if err != nil {
						panic(err)
					}
					acc := foldU64(newFold(), uint64(len(es)))
					for _, ent := range es {
						acc = sinkBytes(acc, ent.Name)
						acc = sinkBytes(acc, ent.ContentKey)
					}
					return acc
				},
			},
			Case{
				Group: "fstree", Op: "fstree.list_entries",
				Workload: fmt.Sprintf("width-%d/full-paging-100", d.Entries), Threads: 1, Ops: d.Entries,
				Dims: wdims, CrossChecksum: true,
				Setup: func(*Env) any { return nil },
				Run: func(en *Env, _ any) uint64 {
					acc := newFold()
					var after []byte
					for {
						es, more, err := fstree.ListEntries(d.Root, after, 100, en.fx.Mem.get)
						if err != nil {
							panic(err)
						}
						acc = foldU64(acc, uint64(len(es)))
						acc = foldBool(acc, more)
						if !more || len(es) == 0 {
							break
						}
						after = es[len(es)-1].Name
					}
					return acc
				},
			},
			Case{
				Group: "fstree", Op: "fstree.reachable_keys",
				Workload: fmt.Sprintf("width-%d", d.Entries), Threads: 0, Ops: int(d.Objects),
				Dims: wdims, CrossChecksum: true,
				Setup: func(*Env) any { return nil },
				Run: func(en *Env, _ any) uint64 {
					ks, err := fstree.ReachableKeys(d.Root, en.fx.Mem.get)
					if err != nil {
						panic(err)
					}
					return foldU64(newFold(), uint64(len(ks)))
				},
			},
		)
	}
	// The first page alone, at the widest directory: paging cost without
	// the walk over everything behind it.
	widest := fx.Dirs[len(fx.Dirs)-1]
	out = append(out, Case{
		Group: "fstree", Op: "fstree.list_entries", Workload: "widest/first-page-100", Threads: 1, Ops: 64 * 100,
		Dims: Dims{Entries: int64(widest.Entries), Width: int64(widest.Entries), Depth: 1,
			Shape: "wide", Content: "structured", Items: 64},
		CrossChecksum: true,
		Setup:         func(*Env) any { return nil },
		Run: func(en *Env, _ any) uint64 {
			acc := newFold()
			for i := 0; i < 64; i++ {
				es, more, err := fstree.ListEntries(widest.Root, nil, 100, en.fx.Mem.get)
				if err != nil {
					panic(err)
				}
				acc = foldU64(acc, uint64(len(es)))
				acc = foldBool(acc, more)
			}
			return acc
		},
	})

	// --- read paths: the depth sweep ------------------------------------
	for _, ch := range fx.Chains {
		ch := ch
		ddims := Dims{Depth: int64(ch.Depth), Shape: "deep", Content: "structured",
			Width: int64(fx.Dirs[0].Entries), Items: 256}
		out = append(out,
			Case{
				Group: "fstree", Op: "fstree.resolve_path",
				Workload: fmt.Sprintf("depth-%d", ch.Depth), Threads: 1, Ops: 256,
				Dims: ddims, CrossChecksum: true,
				Setup: func(*Env) any { return nil },
				Run: func(en *Env, _ any) uint64 {
					acc := newFold()
					for i := 0; i < 256; i++ {
						k, err := fstree.ResolvePath(ch.Root, ch.Path, en.fx.Mem.get)
						if err != nil {
							panic(err)
						}
						acc = foldKey(acc, k)
					}
					return acc
				},
			},
			Case{
				Group: "fstree", Op: "fstree.resolve_entry",
				Workload: fmt.Sprintf("depth-%d", ch.Depth), Threads: 1, Ops: 256,
				Dims: ddims, CrossChecksum: true,
				Setup: func(*Env) any { return nil },
				Run: func(en *Env, _ any) uint64 {
					acc := newFold()
					for i := 0; i < 256; i++ {
						ent, err := fstree.ResolveEntry(ch.Root, ch.Path+"/entry-00000000", en.fx.Mem.get)
						if err != nil {
							panic(err)
						}
						acc = sinkBytes(acc, ent.Name)
						acc = sinkBytes(acc, ent.ContentKey)
					}
					return acc
				},
			},
		)
	}

	out = append(out,
		Case{
			Group: "fstree", Op: "fstree.write_content", Workload: "file-corpus", Threads: 1,
			Ops: int(fx.FileChunks), Bytes: fx.FileBytes, BytesKind: BytesPayload,
			Dims: Dims{ItemBytes: fx.FileBytes, Entries: fx.FileChunks, Shape: "file-index",
				Content: "text"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var c countingWriter
				if err := fstree.WriteContent(&c, en.fx.FileRoot, en.fx.Mem.get); err != nil {
					panic(err)
				}
				return foldI64(newFold(), c.n)
			},
		},
		// ReachableKeys and CheckComplete choose their own parallelism
		// (GOMAXPROCS here, available_parallelism in Rust). Threads 0 marks
		// that; the driver runs under a fixed CPU set so both cores see the
		// same bound, and the effective width is recorded in the document's
		// environment block.
		Case{
			Group: "fstree", Op: "fstree.reachable_keys", Workload: "deep", Threads: 0,
			Ops:           fx.DeepDepth,
			Dims:          Dims{Depth: int64(fx.DeepDepth), Shape: "deep", Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				ks, err := fstree.ReachableKeys(en.fx.DeepRoot, en.fx.Mem.get)
				if err != nil {
					panic(err)
				}
				return foldU64(newFold(), uint64(len(ks)))
			},
		},
		Case{
			Group: "fstree", Op: "fstree.reachable_keys", Workload: "file-corpus", Threads: 0,
			Ops:           int(fx.FileChunks),
			Dims:          Dims{Entries: fx.FileChunks, Shape: "file-index", Content: "text"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				ks, err := fstree.ReachableKeys(en.fx.FileRoot, en.fx.Mem.get)
				if err != nil {
					panic(err)
				}
				return foldU64(newFold(), uint64(len(ks)))
			},
		},
	)
	// Worker scaling, at the widest directory: the same walk asked for one
	// worker and for the profile's concurrent count, so the row pair is a
	// scaling measurement rather than two unrelated numbers.
	for _, jobs := range []int{e.Profile.ThreadsSingle, e.Profile.ThreadsMulti} {
		jobs := jobs
		out = append(out, Case{
			Group: "fstree", Op: "fstree.check_complete",
			Workload: "widest/" + jobsLabel(jobs), Threads: jobs, Ops: int(widest.Objects),
			Dims: Dims{Entries: int64(widest.Entries), Width: int64(widest.Entries),
				Objects: widest.Objects, Shape: "wide", Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				ks, err := fstree.CheckComplete(widest.Root, en.fx.Mem.get, en.fx.Mem.has, jobs)
				if err != nil {
					panic(err)
				}
				return foldU64(newFold(), uint64(len(ks)))
			},
		})
	}
	// The same walk across the width sweep, at one worker.
	for _, d := range fx.Dirs {
		d := d
		out = append(out, Case{
			Group: "fstree", Op: "fstree.check_complete",
			Workload: fmt.Sprintf("width-%d/jobs-1", d.Entries), Threads: 1, Ops: int(d.Objects),
			Dims: Dims{Entries: int64(d.Entries), Width: int64(d.Entries), Objects: d.Objects,
				Shape: "wide", Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				ks, err := fstree.CheckComplete(d.Root, en.fx.Mem.get, en.fx.Mem.has, 1)
				if err != nil {
					panic(err)
				}
				return foldU64(newFold(), uint64(len(ks)))
			},
		})
	}
	out = append(out, Case{
		Group: "fstree", Op: "fstree.check_complete", Workload: "incomplete/jobs-1", Threads: 1,
		Ops:           1,
		Dims:          Dims{Entries: int64(fx.Dirs[0].Entries), Shape: "wide", Content: "structured"},
		CrossChecksum: true,
		Setup:         func(*Env) any { return nil },
		Run: func(en *Env, _ any) uint64 {
			_, err := fstree.CheckComplete(en.fx.IncompleteRoot, en.fx.IncompleteStore.get,
				en.fx.IncompleteStore.has, 1)
			var moe *fstree.MissingObjectError
			if !errors.As(err, &moe) {
				panic(fmt.Sprintf("expected a missing-object error, got %v", err))
			}
			// The key the walk found missing is the output; folding it
			// whole is what makes this a measurement of the walk.
			return foldKey(newFold(), moe.Key)
		},
	})
	return out
}

type countingWriter struct{ n int64 }

func (c *countingWriter) Write(p []byte) (int, error) { c.n += int64(len(p)); return len(p), nil }

func fstreeChecks(e *Env) {
	fx := e.fx

	// Encoders: the object bytes are the format, so their digests are the
	// cross-core contract.
	enc := map[string][]byte{
		"dir-leaf-8":     fx.EncDirLeafSmall,
		"dir-leaf-128":   fx.EncDirLeafLarge,
		"dir-node-8":     fx.EncDirNodeSmall,
		"dir-node-128":   fx.EncDirNodeLarge,
		"file-node-8":    fx.EncFileNodeSmall,
		"file-node-1024": fx.EncFileNodeLarge,
	}
	for name, b := range enc {
		e.pass("fstree", "fstree.encode_"+encoderOf(name), "fstree/encode-"+name, digest(b))
	}
	xo := mustV(fstree.EncodeXattrSet(fx.XattrsLarge))
	e.pass("fstree", "fstree.encode_xattr_set", "fstree/encode-xattr-set-64", digest(xo.Bytes))
	bo := mustV(fstree.EncodeBlob(fx.Small.Items[0]))
	e.want("fstree", "fstree.encode_blob", "fstree/encode-blob",
		bytes.Equal(bo.Bytes, fx.Small.Items[0]) && bo.Key.Type() == key.Blob,
		"a Blob object must be the raw content bytes", bo.Key.String())

	// Decoders round trip back to the inputs.
	es, err := fstree.DecodeDirLeaf(fx.EncDirLeafLarge)
	e.want("fstree", "fstree.decode_dir_leaf", "fstree/decode-dir-leaf",
		err == nil && len(es) == len(fx.EntriesLarge) && bytes.Equal(es[0].Name, fx.EntriesLarge[0].Name) &&
			bytes.Equal(es[8].XattrsIn, fx.EntriesLarge[8].XattrsIn),
		fmt.Sprintf("dir leaf round trip failed: %v", err), digest(mustV(fstree.EncodeDirLeaf(es)).Bytes))
	ps, err := fstree.DecodeDirNode(fx.EncDirNodeLarge)
	e.want("fstree", "fstree.decode_dir_node", "fstree/decode-dir-node",
		err == nil && len(ps) == len(fx.PairsLarge) && bytes.Equal(ps[3].SepName, fx.PairsLarge[3].SepName),
		fmt.Sprintf("dir node round trip failed: %v", err), digest(mustV(fstree.EncodeDirNode(ps)).Bytes))
	ks, err := fstree.DecodeFileNode(fx.EncFileNodeLarge)
	e.want("fstree", "fstree.decode_file_node", "fstree/decode-file-node",
		err == nil && len(ks) == len(fx.ChildrenLarge) && ks[5] == fx.ChildrenLarge[5],
		fmt.Sprintf("file node round trip failed: %v", err), digestKeys(ks))
	// Truncated bodies must be rejected rather than silently short-decoded.
	_, err = fstree.DecodeDirLeaf(fx.EncDirLeafLarge[:len(fx.EncDirLeafLarge)/2])
	e.want("fstree", "fstree.decode_dir_leaf", "fstree/decode-rejects-truncated", err != nil,
		"a truncated DirLeaf body was accepted", "rejected")

	leafKey := mustV(fstree.EncodeDirLeaf(fx.EntriesLarge)).Key
	ck, err := fstree.ChildKeys(leafKey, fx.EncDirLeafLarge)
	e.want("fstree", "fstree.child_keys", "fstree/child-keys-dir-leaf", err == nil && len(ck) > 0,
		fmt.Sprintf("child keys of a DirLeaf: %v", err), digestKeys(ck))
	blobKey := mustV(fstree.EncodeBlob(fx.Small.Items[0])).Key
	ck, err = fstree.ChildKeys(blobKey, fx.Small.Items[0])
	e.want("fstree", "fstree.child_keys", "fstree/child-keys-blob-is-leaf", err == nil && len(ck) == 0,
		fmt.Sprintf("a Blob must have no children: %d, %v", len(ck), err), "0")

	// Builders: the root key of a directory of a fixed entry list is the
	// deepest cross-core equality there is — it folds in the item chunker,
	// the encoders and the key construction.
	e.pass("fstree", "fstree.dir_builder", "fstree/dir-builder-root", fx.WideRoot.String())
	e.pass("fstree", "fstree.index_builder_file", "fstree/file-index-root", fx.FileRoot.String())

	// Read paths.
	ent, err := fstree.LookupEntry(fx.WideRoot, fx.WideNames[len(fx.WideNames)/2], fx.Mem.get)
	e.want("fstree", "fstree.lookup_entry", "fstree/lookup-hit",
		err == nil && bytes.Equal(ent.Name, fx.WideNames[len(fx.WideNames)/2]),
		fmt.Sprintf("lookup failed: %v", err), digest(ent.Name))
	_, err = fstree.LookupEntry(fx.WideRoot, fx.MissName, fx.Mem.get)
	e.want("fstree", "fstree.lookup_entry", "fstree/lookup-miss", errors.Is(err, fstree.ErrNotFound),
		fmt.Sprintf("a missing name must wrap ErrNotFound, got %v", err), "ErrNotFound")

	all, err := fstree.CollectEntries(fx.WideRoot, fx.Mem.get)
	e.want("fstree", "fstree.collect_entries", "fstree/collect-entries",
		err == nil && len(all) == len(fx.WideNames),
		fmt.Sprintf("collected %d of %d entries: %v", len(all), len(fx.WideNames), err),
		digestList(func() [][]byte {
			out := make([][]byte, 0, len(all))
			for _, a := range all {
				out = append(out, a.Name)
			}
			return out
		}()))

	// Paging must see every entry exactly once, in name order.
	var paged [][]byte
	var after []byte
	for {
		page, more, err := fstree.ListEntries(fx.WideRoot, after, 100, fx.Mem.get)
		if err != nil {
			e.fail("fstree", "fstree.list_entries", "fstree/list-paging", err.Error())
			paged = nil
			break
		}
		for _, p := range page {
			paged = append(paged, p.Name)
		}
		if !more || len(page) == 0 {
			break
		}
		after = page[len(page)-1].Name
	}
	if paged != nil {
		e.want("fstree", "fstree.list_entries", "fstree/list-paging",
			len(paged) == len(fx.WideNames) && bytes.Equal(paged[0], sortedNames(fx.WideNames)[0]),
			fmt.Sprintf("paging returned %d of %d entries", len(paged), len(fx.WideNames)),
			digestList(paged))
	}

	k, err := fstree.ResolvePath(fx.DeepRoot, fx.DeepPath, fx.Mem.get)
	e.want("fstree", "fstree.resolve_path", "fstree/resolve-path", err == nil && k == fx.ShallowRoot,
		fmt.Sprintf("resolve path: %v", err), k.String())
	_, err = fstree.ResolvePath(fx.DeepRoot, fx.DeepPath+"/nope", fx.Mem.get)
	e.want("fstree", "fstree.resolve_path", "fstree/resolve-path-missing", errors.Is(err, fstree.ErrNotFound),
		fmt.Sprintf("a missing component must wrap ErrNotFound, got %v", err), "ErrNotFound")
	_, err = fstree.ResolvePath(fx.DeepRoot, "..", fx.Mem.get)
	e.want("fstree", "fstree.resolve_path", "fstree/resolve-path-rejects-dotdot", err != nil,
		"'..' must be rejected", "rejected")
	re, err := fstree.ResolveEntry(fx.DeepRoot, "", fx.Mem.get)
	e.want("fstree", "fstree.resolve_entry", "fstree/resolve-entry-root-is-nil", err == nil && re == nil,
		fmt.Sprintf("the root is not an entry: %v, %v", re, err), "nil")

	var buf bytes.Buffer
	err = fstree.WriteContent(&buf, fx.FileRoot, fx.Mem.get)
	e.want("fstree", "fstree.write_content", "fstree/write-content",
		err == nil && bytes.Equal(buf.Bytes(), fx.CorpusText),
		fmt.Sprintf("reassembled content differs from the source corpus: %v", err),
		digest(buf.Bytes()))

	rk, err := fstree.ReachableKeys(fx.WideRoot, fx.Mem.get)
	e.want("fstree", "fstree.reachable_keys", "fstree/reachable-root-first",
		err == nil && len(rk) > 0 && rk[0] == fx.WideRoot,
		fmt.Sprintf("reachable keys: %v", err), fmt.Sprintf("n=%d", len(rk)))
	// The walk must report each key once even though entries repeat keys.
	seen := map[key.Key]bool{}
	dup := false
	for _, kk := range rk {
		if seen[kk] {
			dup = true
		}
		seen[kk] = true
	}
	e.want("fstree", "fstree.reachable_keys", "fstree/reachable-distinct", err == nil && !dup,
		"a key was reported twice", fmt.Sprintf("n=%d", len(rk)))

	vis, err := fstree.CheckComplete(fx.WideRoot, fx.Mem.get, fx.Mem.has, 1)
	e.want("fstree", "fstree.check_complete", "fstree/check-complete",
		err == nil && len(vis) == len(rk) && vis[0] == fx.WideRoot,
		fmt.Sprintf("check complete: %v", err), fmt.Sprintf("n=%d", len(vis)))
	_, err = fstree.CheckComplete(fx.IncompleteRoot, fx.IncompleteStore.get, fx.IncompleteStore.has, 1)
	var moe *fstree.MissingObjectError
	e.want("fstree", "fstree.check_complete", "fstree/check-complete-missing", errors.As(err, &moe),
		fmt.Sprintf("an absent leaf must surface as MissingObjectError, got %v", err), "MissingObjectError")
}

func encoderOf(name string) string {
	switch {
	case len(name) > 8 && name[:8] == "dir-leaf":
		return "dir_leaf"
	case len(name) > 8 && name[:8] == "dir-node":
		return "dir_node"
	default:
		return "file_node"
	}
}

var _ io.Writer = (*countingWriter)(nil)
