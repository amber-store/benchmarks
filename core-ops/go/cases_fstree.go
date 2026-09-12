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

	// --- encoders -----------------------------------------------------
	encBlob := func(name string, ps payloadSet) Case {
		return Case{
			Group: "fstree", Op: "fstree.encode_blob", Workload: name, Threads: 1,
			Ops: len(ps.Items), Bytes: ps.Bytes,
			Setup: func(*Env) any { return ps },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, b := range s.(payloadSet).Items {
					o, err := fstree.EncodeBlob(b)
					if err != nil {
						panic(err)
					}
					acc += uint64(o.Key[0])
				}
				return acc
			},
		}
	}
	out = append(out, encBlob("tiny-64B", fx.Tiny), encBlob("large-1MiB-text", fx.Large))

	encPair := func(op, name string, n int, run func(*Env) uint64, bytes int64) Case {
		return Case{
			Group: "fstree", Op: op, Workload: name, Threads: 1, Ops: n, Bytes: bytes,
			Setup: func(*Env) any { return nil },
			Run:   func(en *Env, _ any) uint64 { return run(en) },
		}
	}
	out = append(out,
		encPair("fstree.encode_dir_leaf", "entries-8", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				o, err := fstree.EncodeDirLeaf(en.fx.EntriesSmall)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(o.Bytes))
			}
			return acc
		}, int64(reps*len(fx.EncDirLeafSmall))),
		encPair("fstree.encode_dir_leaf", "entries-128-with-xattrs", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				o, err := fstree.EncodeDirLeaf(en.fx.EntriesLarge)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(o.Bytes))
			}
			return acc
		}, int64(reps*len(fx.EncDirLeafLarge))),
		encPair("fstree.encode_dir_node", "pairs-8", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				o, err := fstree.EncodeDirNode(en.fx.PairsSmall)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(o.Bytes))
			}
			return acc
		}, int64(reps*len(fx.EncDirNodeSmall))),
		encPair("fstree.encode_dir_node", "pairs-128", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				o, err := fstree.EncodeDirNode(en.fx.PairsLarge)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(o.Bytes))
			}
			return acc
		}, int64(reps*len(fx.EncDirNodeLarge))),
		encPair("fstree.encode_file_node", "children-8", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				o, err := fstree.EncodeFileNode(en.fx.ChildrenSmall)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(o.Bytes))
			}
			return acc
		}, int64(reps*len(fx.EncFileNodeSmall))),
		encPair("fstree.encode_file_node", "children-1024", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				o, err := fstree.EncodeFileNode(en.fx.ChildrenLarge)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(o.Bytes))
			}
			return acc
		}, int64(reps*len(fx.EncFileNodeLarge))),
		encPair("fstree.encode_xattr_set", "xattrs-64", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				o, err := fstree.EncodeXattrSet(en.fx.XattrsLarge)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(o.Bytes))
			}
			return acc
		}, 0),

		// --- decoders -------------------------------------------------
		encPair("fstree.decode_dir_leaf", "entries-8", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				es, err := fstree.DecodeDirLeaf(en.fx.EncDirLeafSmall)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(es))
			}
			return acc
		}, int64(reps*len(fx.EncDirLeafSmall))),
		encPair("fstree.decode_dir_leaf", "entries-128-with-xattrs", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				es, err := fstree.DecodeDirLeaf(en.fx.EncDirLeafLarge)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(es))
			}
			return acc
		}, int64(reps*len(fx.EncDirLeafLarge))),
		encPair("fstree.decode_dir_node", "pairs-128", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				ps, err := fstree.DecodeDirNode(en.fx.EncDirNodeLarge)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(ps))
			}
			return acc
		}, int64(reps*len(fx.EncDirNodeLarge))),
		encPair("fstree.decode_file_node", "children-1024", reps, func(en *Env) uint64 {
			var acc uint64
			for i := 0; i < reps; i++ {
				ks, err := fstree.DecodeFileNode(en.fx.EncFileNodeLarge)
				if err != nil {
					panic(err)
				}
				acc += uint64(len(ks))
			}
			return acc
		}, int64(reps*len(fx.EncFileNodeLarge))),
	)

	// --- child keys ---------------------------------------------------
	childInputs := []struct {
		name string
		k    func(*Env) key.Key
		b    func(*Env) []byte
	}{
		{"dir-leaf-128", func(en *Env) key.Key { return mustV(fstree.EncodeDirLeaf(en.fx.EntriesLarge)).Key },
			func(en *Env) []byte { return en.fx.EncDirLeafLarge }},
		{"dir-node-128", func(en *Env) key.Key { return mustV(fstree.EncodeDirNode(en.fx.PairsLarge)).Key },
			func(en *Env) []byte { return en.fx.EncDirNodeLarge }},
		{"file-node-1024", func(en *Env) key.Key { return mustV(fstree.EncodeFileNode(en.fx.ChildrenLarge)).Key },
			func(en *Env) []byte { return en.fx.EncFileNodeLarge }},
	}
	for _, ci := range childInputs {
		ci := ci
		out = append(out, Case{
			Group: "fstree", Op: "fstree.child_keys", Workload: ci.name, Threads: 1, Ops: reps,
			Setup: func(en *Env) any { return [2]any{ci.k(en), ci.b(en)} },
			Run: func(_ *Env, s any) uint64 {
				pair := s.([2]any)
				var acc uint64
				for i := 0; i < reps; i++ {
					ks, err := fstree.ChildKeys(pair[0].(key.Key), pair[1].([]byte))
					if err != nil {
						panic(err)
					}
					acc += uint64(len(ks))
				}
				return acc
			},
		})
	}

	// --- builders -----------------------------------------------------
	for _, n := range []int{128, e.Profile.SyntheticWide} {
		n := n
		out = append(out, Case{
			Group: "fstree", Op: "fstree.dir_builder", Workload: fmt.Sprintf("entries-%d", n), Threads: 1,
			Ops: n,
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
				return uint64(sink.n) + uint64(root[0])
			},
		})
	}
	for _, n := range []int{128, 65536} {
		n := n
		out = append(out, Case{
			Group: "fstree", Op: "fstree.index_builder_file", Workload: fmt.Sprintf("children-%d", n), Threads: 1,
			Ops: n,
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
				return uint64(sink.n) + uint64(root[0])
			},
		})
	}

	// --- read paths ---------------------------------------------------
	lookups := e.Profile.BatchOps
	out = append(out,
		Case{
			Group: "fstree", Op: "fstree.lookup_entry", Workload: "wide/hit", Threads: 1, Ops: lookups,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var acc uint64
				names := en.fx.WideNames
				for i := 0; i < lookups; i++ {
					ent, err := fstree.LookupEntry(en.fx.WideRoot, names[(i*7919)%len(names)], en.fx.Mem.get)
					if err != nil {
						panic(err)
					}
					acc += uint64(len(ent.Name))
				}
				return acc
			},
		},
		Case{
			Group: "fstree", Op: "fstree.lookup_entry", Workload: "wide/miss", Threads: 1, Ops: lookups,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var acc uint64
				for i := 0; i < lookups; i++ {
					_, err := fstree.LookupEntry(en.fx.WideRoot, en.fx.MissName, en.fx.Mem.get)
					if errors.Is(err, fstree.ErrNotFound) {
						acc++
					}
				}
				return acc
			},
		},
		Case{
			Group: "fstree", Op: "fstree.lookup_entry", Workload: "shallow/hit", Threads: 1, Ops: lookups,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var acc uint64
				for i := 0; i < lookups; i++ {
					ent, err := fstree.LookupEntry(en.fx.ShallowRoot,
						[]byte(fmt.Sprintf("entry-%08d", i%16)), en.fx.Mem.get)
					if err != nil {
						panic(err)
					}
					acc += uint64(len(ent.Name))
				}
				return acc
			},
		},
		Case{
			Group: "fstree", Op: "fstree.list_entries", Workload: "wide/first-page-100", Threads: 1, Ops: 64,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var acc uint64
				for i := 0; i < 64; i++ {
					es, more, err := fstree.ListEntries(en.fx.WideRoot, nil, 100, en.fx.Mem.get)
					if err != nil {
						panic(err)
					}
					acc += uint64(len(es))
					if more {
						acc++
					}
				}
				return acc
			},
		},
		Case{
			Group: "fstree", Op: "fstree.list_entries", Workload: "wide/full-paging-100", Threads: 1,
			Ops:   1,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var acc uint64
				var after []byte
				for {
					es, more, err := fstree.ListEntries(en.fx.WideRoot, after, 100, en.fx.Mem.get)
					if err != nil {
						panic(err)
					}
					acc += uint64(len(es))
					if !more || len(es) == 0 {
						break
					}
					after = es[len(es)-1].Name
				}
				return acc
			},
		},
		Case{
			Group: "fstree", Op: "fstree.collect_entries", Workload: "wide", Threads: 1, Ops: 1,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				es, err := fstree.CollectEntries(en.fx.WideRoot, en.fx.Mem.get)
				if err != nil {
					panic(err)
				}
				return uint64(len(es))
			},
		},
		Case{
			Group: "fstree", Op: "fstree.resolve_path", Workload: "deep", Threads: 1, Ops: 256,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var acc uint64
				for i := 0; i < 256; i++ {
					k, err := fstree.ResolvePath(en.fx.DeepRoot, en.fx.DeepPath, en.fx.Mem.get)
					if err != nil {
						panic(err)
					}
					acc += uint64(k[0])
				}
				return acc
			},
		},
		Case{
			Group: "fstree", Op: "fstree.resolve_entry", Workload: "deep", Threads: 1, Ops: 256,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var acc uint64
				for i := 0; i < 256; i++ {
					ent, err := fstree.ResolveEntry(en.fx.DeepRoot, en.fx.DeepPath+"/entry-00000000", en.fx.Mem.get)
					if err != nil {
						panic(err)
					}
					acc += uint64(len(ent.Name))
				}
				return acc
			},
		},
		Case{
			Group: "fstree", Op: "fstree.write_content", Workload: "file-corpus", Threads: 1,
			Ops: 1, Bytes: fx.FileBytes,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var c countingWriter
				if err := fstree.WriteContent(&c, en.fx.FileRoot, en.fx.Mem.get); err != nil {
					panic(err)
				}
				return uint64(c.n)
			},
		},
		// ReachableKeys and CheckComplete choose their own parallelism
		// (GOMAXPROCS here, available_parallelism in Rust). Threads 0 marks
		// that; the driver runs under a fixed CPU set so both cores see the
		// same bound.
		Case{
			Group: "fstree", Op: "fstree.reachable_keys", Workload: "wide", Threads: 0, Ops: 1,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				ks, err := fstree.ReachableKeys(en.fx.WideRoot, en.fx.Mem.get)
				if err != nil {
					panic(err)
				}
				return uint64(len(ks))
			},
		},
		Case{
			Group: "fstree", Op: "fstree.reachable_keys", Workload: "deep", Threads: 0, Ops: 1,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				ks, err := fstree.ReachableKeys(en.fx.DeepRoot, en.fx.Mem.get)
				if err != nil {
					panic(err)
				}
				return uint64(len(ks))
			},
		},
		Case{
			Group: "fstree", Op: "fstree.reachable_keys", Workload: "file-corpus", Threads: 0, Ops: 1,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				ks, err := fstree.ReachableKeys(en.fx.FileRoot, en.fx.Mem.get)
				if err != nil {
					panic(err)
				}
				return uint64(len(ks))
			},
		},
	)
	for _, jobs := range []int{e.Profile.ThreadsSingle, e.Profile.ThreadsMulti} {
		jobs := jobs
		out = append(out, Case{
			Group: "fstree", Op: "fstree.check_complete",
			Workload: fmt.Sprintf("wide/jobs-%d", jobs), Threads: jobs, Ops: 1,
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				ks, err := fstree.CheckComplete(en.fx.WideRoot, en.fx.Mem.get, en.fx.Mem.has, jobs)
				if err != nil {
					panic(err)
				}
				return uint64(len(ks))
			},
		})
	}
	out = append(out, Case{
		Group: "fstree", Op: "fstree.check_complete", Workload: "incomplete/jobs-1", Threads: 1, Ops: 1,
		Setup: func(*Env) any { return nil },
		Run: func(en *Env, _ any) uint64 {
			_, err := fstree.CheckComplete(en.fx.IncompleteRoot, en.fx.IncompleteStore.get,
				en.fx.IncompleteStore.has, 1)
			var moe *fstree.MissingObjectError
			if errors.As(err, &moe) {
				return 1
			}
			panic(fmt.Sprintf("expected a missing-object error, got %v", err))
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
