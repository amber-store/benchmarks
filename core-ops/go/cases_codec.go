package main

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"reflect"
	"sort"
	"strings"

	"github.com/amber-store/core/chunkers"
	"github.com/amber-store/core/key"
	"github.com/amber-store/core/reference"
)

// ---------------------------------------------------------------------------
// key
// ---------------------------------------------------------------------------

func keyCases(e *Env) []Case {
	fx := e.fx
	var out []Case
	// key.New hashes the payload and assembles a header, so it is the
	// clearest place to sweep the whole (size, content) grid: the header
	// cost is constant and the hash cost is proportional, and the crossing
	// point is visible in the plot.
	for _, ps := range fx.Payloads {
		ps := ps
		out = append(out, Case{
			Group: "key", Op: "key.new", Workload: ps.Name, Threads: 1,
			Ops: len(ps.Items), Bytes: ps.Bytes, BytesKind: BytesPayload,
			Dims: ps.dims(), CrossChecksum: true,
			Setup: func(*Env) any { return ps },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for _, b := range s.(payloadSet).Items {
					k, err := key.New(key.Blob, uint64(len(b)), b)
					if err != nil {
						panic(err)
					}
					// The whole key, header and digest: 32 bytes, so the
					// hash bytes are observed without adding a second pass
					// over the payload.
					acc = foldKey(acc, k)
				}
				return acc
			},
		})
	}

	hashes := make([][32]byte, len(fx.Keys))
	for i := range fx.Keys {
		copy(hashes[i][:], fx.Keys[i].Hash())
	}
	out = append(out,
		Case{
			Group: "key", Op: "key.new_from_hash", Workload: "batch", Threads: 1,
			Ops:  len(hashes),
			Dims: Dims{Items: int64(len(hashes)), ItemBytes: 32, Content: "structured"},
			// Rust's key fixture derives from the same seed, so the
			// assembled keys are the same on both sides.
			CrossChecksum: true,
			Setup:         func(*Env) any { return hashes },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for i, h := range s.([][32]byte) {
					k, err := key.NewFromHash(key.Blob, uint64(i), h)
					if err != nil {
						panic(err)
					}
					acc = foldKey(acc, k)
				}
				return acc
			},
		},
		Case{
			Group: "key", Op: "key.parse", Workload: "canonical", Threads: 1,
			Ops:           len(fx.KeyBytes),
			Dims:          Dims{Items: int64(len(fx.KeyBytes)), ItemBytes: 32, Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return fx.KeyBytes },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for _, b := range s.([][]byte) {
					k, err := key.Parse(b)
					if err != nil {
						panic(err)
					}
					acc = foldKey(acc, k)
				}
				return acc
			},
		},
		Case{
			Group: "key", Op: "key.parse", Workload: "malformed", Threads: 1,
			Ops:           len(fx.BadKeyBytes) * 256,
			Dims:          Dims{Items: int64(len(fx.BadKeyBytes) * 256), ItemBytes: 32, Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return fx.BadKeyBytes },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				bad := s.([][]byte)
				for i := 0; i < 256; i++ {
					for _, b := range bad {
						_, err := key.Parse(b)
						acc = foldBool(acc, err != nil)
					}
				}
				return acc
			},
		},
		Case{
			Group: "key", Op: "key.validate", Workload: "canonical", Threads: 1,
			Ops:           len(fx.Keys),
			Dims:          Dims{Items: int64(len(fx.Keys)), ItemBytes: 32, Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return fx.Keys },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for _, k := range s.([]key.Key) {
					acc = foldBool(acc, k.Validate() == nil)
				}
				return acc
			},
		},
		// The header accessors are single field reads; timing each one
		// separately would measure the loop, not the core. They are batched
		// into one grouped case and the coverage matrix records that. Every
		// field, including the whole 31-byte digest, is consumed: reading
		// one byte of the hash would let a compiler keep only that byte.
		Case{
			Group: "key", Op: "key.accessors", Workload: "type+length+length_size+hash", Threads: 1,
			Ops:           len(fx.Keys) * 4,
			Dims:          Dims{Items: int64(len(fx.Keys)), ItemBytes: 32, Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return fx.Keys },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for _, k := range s.([]key.Key) {
					acc = foldU64(acc, uint64(k.Type()))
					acc = foldU64(acc, k.Length())
					acc = foldU64(acc, uint64(k.LengthSize()))
					acc = foldBytes(acc, k.Hash())
				}
				return acc
			},
		},
		Case{
			Group: "key", Op: "key.string", Workload: "hex", Threads: 1,
			Ops:           len(fx.Keys),
			Dims:          Dims{Items: int64(len(fx.Keys)), ItemBytes: 32, Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return fx.Keys },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for _, k := range s.([]key.Key) {
					// The rendering is short and fixed-length; folding it
					// whole is what makes the case a rendering measurement
					// rather than a length measurement.
					acc = foldStr(acc, k.String())
				}
				return acc
			},
		},
		Case{
			Group: "key", Op: "key.type_string", Workload: "names", Threads: 1,
			Ops:           len(fx.Keys),
			Dims:          Dims{Items: int64(len(fx.Keys)), ItemBytes: 32, Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return fx.Keys },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for _, k := range s.([]key.Key) {
					acc = foldStr(acc, k.Type().String())
					acc = foldBool(acc, k.Type().IsValid())
				}
				return acc
			},
		},
	)
	return out
}

func keyChecks(e *Env) {
	fx := e.fx
	// Construction is deterministic and the two cores must agree on every
	// bit of it: the digest of the keys built from a fixed payload set is
	// the cross-core contract.
	var built []key.Key
	for _, b := range fx.Small.Items {
		k, err := key.New(key.Blob, uint64(len(b)), b)
		if err != nil {
			e.fail("key", "key.new", "key/new-small", err.Error())
			return
		}
		built = append(built, k)
	}
	e.pass("key", "key.new", "key/new-small", digestKeys(built))

	var fromHash []key.Key
	for i, t := range []key.Type{key.Blob, key.FileNode, key.DirLeaf, key.DirNode, key.XattrSet} {
		var h [32]byte
		copy(h[:], randomBytes(e.Profile.Seed+12345+uint64(i), 32))
		k, err := key.NewFromHash(t, uint64(i)*777, h)
		if err != nil {
			e.fail("key", "key.new_from_hash", "key/new-from-hash", err.Error())
			return
		}
		fromHash = append(fromHash, k)
	}
	e.pass("key", "key.new_from_hash", "key/new-from-hash", digestKeys(fromHash))

	// Round trip: parse(bytes(k)) == k, and the accessors read back what
	// was put in.
	k0 := built[0]
	kb := k0
	rt, err := key.Parse(kb[:])
	e.want("key", "key.parse", "key/parse-roundtrip", err == nil && rt == k0,
		fmt.Sprintf("parse round trip failed: %v", err), rt.String())
	e.want("key", "key.accessors", "key/accessors",
		k0.Type() == key.Blob && k0.Length() == uint64(len(fx.Small.Items[0])) &&
			k0.LengthSize() >= 1 && len(k0.Hash()) == key.Size-1-k0.LengthSize(),
		"accessors disagree with the constructed key",
		fmt.Sprintf("%s/%d/%d", k0.Type(), k0.Length(), k0.LengthSize()))

	// The rejection paths are part of the contract; each malformed key must
	// fail with its own sentinel.
	wants := []error{key.ErrReservedBitSet, key.ErrReservedType, key.ErrNonCanonicalLength, key.ErrBadKeyLength}
	var details []string
	ok := true
	for i, b := range fx.BadKeyBytes {
		_, err := key.Parse(b)
		if err == nil || !errors.Is(err, wants[i]) {
			ok = false
			details = append(details, fmt.Sprintf("case %d: got %v, want %v", i, err, wants[i]))
		}
	}
	e.want("key", "key.parse", "key/parse-rejects", ok, strings.Join(details, "; "),
		digestStrings([]string{"ErrReservedBitSet", "ErrReservedType", "ErrNonCanonicalLength", "ErrBadKeyLength"}))

	e.want("key", "key.type_string", "key/type-names",
		key.Blob.String() == "Blob" && key.XattrSet.String() == "XattrSet" &&
			key.Type(7).IsValid() == false,
		"type names or validity disagree",
		digestStrings([]string{key.Blob.String(), key.FileNode.String(), key.DirLeaf.String(),
			key.DirNode.String(), key.XattrSet.String()}))
}

// ---------------------------------------------------------------------------
// cbor (Go: package cborx)
// ---------------------------------------------------------------------------

func cborCases(e *Env) []Case {
	fx := e.fx
	reps := 64
	mk := func(name string, m map[string][]byte, enc []byte) []Case {
		return []Case{
			{
				Group: "cbor", Op: "cbor.encode_xattrs", Workload: name, Threads: 1,
				Ops: reps, Bytes: int64(reps * len(enc)), BytesKind: BytesEncoded,
				Dims:          Dims{Items: int64(reps), Entries: int64(len(m)), ItemBytes: int64(len(enc)), Content: "structured"},
				CrossChecksum: true,
				Setup:         func(*Env) any { return m },
				Run: func(_ *Env, s any) uint64 {
					acc := newFold()
					for i := 0; i < reps; i++ {
						acc = sinkBytes(acc, encodeXattrs(s.(map[string][]byte)))
					}
					return acc
				},
			},
			{
				Group: "cbor", Op: "cbor.decode_xattrs", Workload: name, Threads: 1,
				Ops: reps, Bytes: int64(reps * len(enc)), BytesKind: BytesEncoded,
				Dims:          Dims{Items: int64(reps), Entries: int64(len(m)), ItemBytes: int64(len(enc)), Content: "structured"},
				CrossChecksum: true,
				Setup:         func(*Env) any { return enc },
				Run: func(_ *Env, s any) uint64 {
					acc := newFold()
					for i := 0; i < reps; i++ {
						got, err := decodeXattrs(s.([]byte))
						if err != nil {
							panic(err)
						}
						// Every decoded value is consumed at its ends, so
						// a decoder that returned empty buffers could not
						// reproduce this number.
						acc = foldU64(acc, uint64(len(got)))
						for _, k := range sortedKeys(got) {
							acc = foldStr(acc, k)
							acc = sinkBytes(acc, got[k])
						}
					}
					return acc
				},
			},
		}
	}
	out := mk("xattrs-3", fx.XattrsSmall, fx.XattrsSmallEnc)
	out = append(out, mk("xattrs-64", fx.XattrsLarge, fx.XattrsLargeEnc)...)
	// The Rust core additionally exports the CBOR head/bstr primitives
	// (`append_head`, `read_head`, `append_bstr`, `read_bstr`); the Go core
	// keeps them unexported inside cborx, so there is nothing to call here.
	// The grouped operation id the Rust driver measures them under is
	// declared too, so the report can bind its Rust-only case to a
	// declaration rather than infer one.
	e.skip("cbor.head_primitives", "", "Go keeps the CBOR head/byte-string primitives unexported inside package cborx; only EncodeXattrs/DecodeXattrs are public")
	e.skip("cbor.append_head", "", "Go keeps the CBOR head/byte-string primitives unexported inside package cborx; only EncodeXattrs/DecodeXattrs are public")
	e.skip("cbor.read_head", "", "Go keeps the CBOR head/byte-string primitives unexported inside package cborx; only EncodeXattrs/DecodeXattrs are public")
	e.skip("cbor.append_bstr", "", "Go keeps the CBOR head/byte-string primitives unexported inside package cborx; only EncodeXattrs/DecodeXattrs are public")
	e.skip("cbor.read_bstr", "", "Go keeps the CBOR head/byte-string primitives unexported inside package cborx; only EncodeXattrs/DecodeXattrs are public")
	e.skip("binaryfuse.new", "", "the Go core builds its pack filters with github.com/FastFilter/xorfilter, an external dependency; it exports no binaryfuse package of its own")
	e.skip("binaryfuse.contains", "", "the Go core builds its pack filters with github.com/FastFilter/xorfilter, an external dependency; it exports no binaryfuse package of its own")
	e.skip("binaryfuse.section_bytes", "", "the Go core keeps filter-section serialization unexported inside packstore")
	e.skip("binaryfuse.parse_section", "", "the Go core keeps filter-section parsing unexported inside packstore")
	return out
}

func cborChecks(e *Env) {
	fx := e.fx
	for name, m := range map[string]map[string][]byte{"xattrs-3": fx.XattrsSmall, "xattrs-64": fx.XattrsLarge} {
		enc := encodeXattrs(m)
		back, err := decodeXattrs(enc)
		e.want("cbor", "cbor.decode_xattrs", "cbor/roundtrip-"+name,
			err == nil && reflect.DeepEqual(back, m),
			fmt.Sprintf("xattr round trip failed: %v", err), digest(enc))
	}
	// Canonical order is part of the format: the encoding must be stable
	// regardless of Go's map iteration order.
	first := encodeXattrs(fx.XattrsLarge)
	stable := true
	for i := 0; i < 32; i++ {
		if !bytes.Equal(first, encodeXattrs(fx.XattrsLarge)) {
			stable = false
		}
	}
	e.want("cbor", "cbor.encode_xattrs", "cbor/canonical-stable", stable,
		"encoding is not stable across repeated calls", digest(first))
	// Trailing bytes must be rejected.
	_, err := decodeXattrs(append(append([]byte(nil), fx.XattrsSmallEnc...), 0x00))
	e.want("cbor", "cbor.decode_xattrs", "cbor/rejects-trailing", err != nil,
		"trailing bytes were accepted", "rejected")
}

// ---------------------------------------------------------------------------
// chunkers
// ---------------------------------------------------------------------------

func chunkerCases(e *Env) []Case {
	fx := e.fx
	var out []Case
	// The chunk count of each corpus is measured here, outside every timed
	// interval, so a per-chunk figure divides by the exact number of
	// boundaries the splitter will find rather than by one.
	corpora := []struct {
		name    string
		content string
		data    []byte
		chunks  int
	}{
		{"random", "random", fx.CorpusRandom, countChunks(fx.CorpusRandom, nil)},
		{"compressible", "text", fx.CorpusText, countChunks(fx.CorpusText, nil)},
		{"tiny-64KiB", "text", fx.CorpusText[:64<<10], countChunks(fx.CorpusText[:64<<10], nil)},
	}
	for _, c := range corpora {
		c := c
		out = append(out, Case{
			Group: "chunkers", Op: "chunkers.split_bytes", Workload: c.name + "/default-sizes", Threads: 1,
			Ops: c.chunks, Bytes: int64(len(c.data)), BytesKind: BytesPayload,
			Dims:          Dims{ItemBytes: int64(len(c.data)), Items: 1, Entries: int64(c.chunks), Content: c.content},
			CrossChecksum: true,
			Setup:         func(*Env) any { return c.data },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				err := chunkers.SplitBytes(bytes.NewReader(s.([]byte)), nil, func(chunk []byte) error {
					// The boundary list is the output; each chunk's length
					// is folded, which is O(chunks) rather than O(bytes).
					acc = foldU64(acc, uint64(len(chunk)))
					return nil
				})
				if err != nil {
					panic(err)
				}
				return acc
			},
		})
	}
	// A second size configuration, to show the boundary parameters are a
	// measured dimension rather than a constant.
	small := &chunkers.ByteOpts{MinSize: 4 << 10, NormalSize: 16 << 10, MaxSize: 64 << 10}
	out = append(out, Case{
		Group: "chunkers", Op: "chunkers.split_bytes", Workload: "compressible/4-16-64KiB", Threads: 1,
		Ops: countChunks(fx.CorpusText, small), Bytes: int64(len(fx.CorpusText)), BytesKind: BytesPayload,
		Dims: Dims{ItemBytes: int64(len(fx.CorpusText)), Items: 1,
			Entries: int64(countChunks(fx.CorpusText, small)), Content: "text"},
		CrossChecksum: true,
		Setup:         func(*Env) any { return fx.CorpusText },
		Run: func(_ *Env, s any) uint64 {
			acc := newFold()
			err := chunkers.SplitBytes(bytes.NewReader(s.([]byte)), small, func(chunk []byte) error {
				acc = foldU64(acc, uint64(len(chunk)))
				return nil
			})
			if err != nil {
				panic(err)
			}
			return acc
		},
	})
	out = append(out,
		Case{
			Group: "chunkers", Op: "chunkers.item_chunker", Workload: "is_boundary/bits-7", Threads: 1,
			Ops:           len(fx.ItemEncodings),
			Dims:          Dims{Items: int64(len(fx.ItemEncodings)), ItemBytes: 32, Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return chunkers.NewItemChunker(7) },
			Run: func(en *Env, s any) uint64 {
				ic := s.(chunkers.ItemChunker)
				acc := newFold()
				run := 0
				for _, enc := range en.fx.ItemEncodings {
					run++
					b := ic.IsBoundary(enc, run)
					acc = foldBool(acc, b)
					if b {
						run = 0
					}
				}
				return acc
			},
		},
		Case{
			Group: "chunkers", Op: "chunkers.new_item_chunker", Workload: "bits-4..12", Threads: 1,
			Ops:           9 * 256,
			Dims:          Dims{Items: 9 * 256, Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return nil },
			Run: func(*Env, any) uint64 {
				acc := newFold()
				for i := 0; i < 256; i++ {
					for bits := 4; bits <= 12; bits++ {
						ic := chunkers.NewItemChunker(bits)
						acc = foldU64(acc, uint64(ic.MinRun))
						acc = foldU64(acc, uint64(ic.MaxRun))
						blackBox(ic)
					}
				}
				return acc
			},
		},
	)
	return out
}

func chunkerChecks(e *Env) {
	fx := e.fx
	// Chunk boundaries define every file key, so the two cores must split
	// the same corpus into exactly the same chunks.
	for name, data := range map[string][]byte{"random": fx.CorpusRandom, "compressible": fx.CorpusText} {
		var chunks [][]byte
		var total int
		err := chunkers.SplitBytes(bytes.NewReader(data), nil, func(c []byte) error {
			chunks = append(chunks, append([]byte(nil), c...))
			total += len(c)
			return nil
		})
		ok := err == nil && total == len(data)
		e.want("chunkers", "chunkers.split_bytes", "chunkers/split-"+name, ok,
			fmt.Sprintf("split failed: %v (covered %d of %d bytes)", err, total, len(data)),
			digestList(chunks))
		if ok {
			sizes := true
			for i, c := range chunks {
				last := i == len(chunks)-1
				if len(c) > chunkers.DefaultMaxSize || (!last && len(c) < chunkers.DefaultMinSize) {
					sizes = false
				}
			}
			e.want("chunkers", "chunkers.split_bytes", "chunkers/split-bounds-"+name, sizes,
				"a chunk fell outside the configured min/max bounds",
				fmt.Sprintf("n=%d", len(chunks)))
		}
	}
	// An empty reader yields no chunks.
	n := 0
	err := chunkers.SplitBytes(bytes.NewReader(nil), nil, func([]byte) error { n++; return nil })
	e.want("chunkers", "chunkers.split_bytes", "chunkers/split-empty", err == nil && n == 0,
		fmt.Sprintf("empty reader produced %d chunks (%v)", n, err), "0")
	// A failing consumer aborts the split with its own error.
	sentinel := errors.New("consumer stop")
	err = chunkers.SplitBytes(bytes.NewReader(fx.CorpusText), nil, func([]byte) error { return sentinel })
	e.want("chunkers", "chunkers.split_bytes", "chunkers/split-propagates-error", errors.Is(err, sentinel),
		fmt.Sprintf("consumer error was not propagated: %v", err), "propagated")

	ic := chunkers.NewItemChunker(7)
	e.want("chunkers", "chunkers.new_item_chunker", "chunkers/item-bounds",
		ic.MinRun == 32 && ic.MaxRun == 512,
		fmt.Sprintf("item chunker bounds are %d/%d", ic.MinRun, ic.MaxRun),
		fmt.Sprintf("%d/%d", ic.MinRun, ic.MaxRun))
	var bounds []string
	run := 0
	for i, enc := range fx.ItemEncodings {
		run++
		if ic.IsBoundary(enc, run) {
			bounds = append(bounds, fmt.Sprintf("%d", i))
			run = 0
		}
	}
	e.pass("chunkers", "chunkers.item_chunker", "chunkers/item-boundaries", digestStrings(bounds))
}

// ---------------------------------------------------------------------------
// reference
// ---------------------------------------------------------------------------

func referenceCases(e *Env) []Case {
	fx := e.fx
	reps := 256
	names := make([]string, 0, 256)
	for i := 0; i < 256; i++ {
		names = append(names, fmt.Sprintf("bench/name/%d/%s", i, strings.Repeat("x", i%64)))
	}
	return []Case{
		{
			Group: "reference", Op: "reference.encode", Workload: "signed", Threads: 1,
			Ops: reps, Bytes: int64(reps * len(fx.RefRecordEnc)), BytesKind: BytesEncoded,
			Dims:          Dims{Items: int64(reps), ItemBytes: int64(len(fx.RefRecordEnc)), Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return fx.RefRecord },
			Run: func(_ *Env, s any) uint64 {
				r := s.(reference.Reference)
				acc := newFold()
				for i := 0; i < reps; i++ {
					b, err := r.Encode()
					if err != nil {
						panic(err)
					}
					acc = sinkBytes(acc, b)
				}
				return acc
			},
		},
		{
			Group: "reference", Op: "reference.decode", Workload: "signed", Threads: 1,
			Ops: reps, Bytes: int64(reps * len(fx.RefRecordEnc)), BytesKind: BytesEncoded,
			Dims:          Dims{Items: int64(reps), ItemBytes: int64(len(fx.RefRecordEnc)), Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return fx.RefRecordEnc },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for i := 0; i < reps; i++ {
					r, err := reference.Decode(s.([]byte))
					if err != nil {
						panic(err)
					}
					// Every decoded field, not just the name: the record is
					// small and fixed, so this stays constant-time.
					acc = foldStr(acc, r.Name)
					acc = foldStr(acc, r.User)
					acc = foldI64(acc, r.CreatedAt)
					acc = sinkBytes(acc, r.Key)
					acc = sinkBytes(acc, r.Signature)
					acc = sinkBytes(acc, r.PublicKey)
				}
				return acc
			},
		},
		{
			Group: "reference", Op: "reference.signature_payload", Workload: "signed", Threads: 1,
			Ops:           reps,
			Dims:          Dims{Items: int64(reps), Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return fx.RefRecord },
			Run: func(_ *Env, s any) uint64 {
				r := s.(reference.Reference)
				acc := newFold()
				for i := 0; i < reps; i++ {
					b, err := r.SignaturePayload()
					if err != nil {
						panic(err)
					}
					acc = sinkBytes(acc, b)
				}
				return acc
			},
		},
		{
			Group: "reference", Op: "reference.validate_name", Workload: "valid", Threads: 1,
			Ops:           len(names),
			Dims:          Dims{Items: int64(len(names)), Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return names },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for _, n := range s.([]string) {
					acc = foldBool(acc, reference.ValidateName(n) == nil)
				}
				return acc
			},
		},
		{
			Group: "reference", Op: "reference.validate_user", Workload: "valid", Threads: 1,
			Ops:           len(names),
			Dims:          Dims{Items: int64(len(names)), Content: "structured"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return names },
			Run: func(_ *Env, s any) uint64 {
				acc := newFold()
				for _, n := range s.([]string) {
					acc = foldBool(acc, reference.ValidateUser(n+"@example.org") == nil)
				}
				return acc
			},
		},
	}
}

func referenceChecks(e *Env) {
	fx := e.fx
	enc, err := fx.RefRecord.Encode()
	e.want("reference", "reference.encode", "reference/encode", err == nil,
		fmt.Sprintf("encode failed: %v", err), digest(enc))
	back, err := reference.Decode(enc)
	e.want("reference", "reference.decode", "reference/roundtrip",
		err == nil && back.Name == fx.RefRecord.Name && bytes.Equal(back.Key, fx.RefRecord.Key) &&
			back.User == fx.RefRecord.User && back.CreatedAt == fx.RefRecord.CreatedAt &&
			bytes.Equal(back.Signature, fx.RefRecord.Signature) &&
			bytes.Equal(back.PublicKey, fx.RefRecord.PublicKey),
		fmt.Sprintf("reference round trip failed: %v", err), digest(enc))
	sp, err := fx.RefRecord.SignaturePayload()
	e.want("reference", "reference.signature_payload", "reference/signature-payload", err == nil,
		fmt.Sprintf("signature payload failed: %v", err), digest(sp))
	// A signature payload must not carry the signature but must carry the
	// public key it was made with.
	e.want("reference", "reference.signature_payload", "reference/signature-payload-excludes-sig",
		err == nil && !bytes.Contains(sp, fx.RefRecord.Signature) && bytes.Contains(sp, fx.RefRecord.PublicKey),
		"signature payload does not bind exactly the expected fields", "ok")
	// Trailing bytes and non-canonical encodings are rejected.
	_, err = reference.Decode(append(append([]byte(nil), enc...), 0x00))
	e.want("reference", "reference.decode", "reference/rejects-trailing", err != nil,
		"trailing bytes were accepted", "rejected")
	bad := []string{"", "with@at", "with\x01control", strings.Repeat("n", reference.MaxNameLen+1)}
	allBad := true
	for _, n := range bad {
		if reference.ValidateName(n) == nil {
			allBad = false
		}
	}
	e.want("reference", "reference.validate_name", "reference/name-rules", allBad,
		"an invalid reference name was accepted", "rejected")
	e.want("reference", "reference.validate_user", "reference/user-rules",
		reference.ValidateUser("a@b.example") == nil && reference.ValidateUser("") != nil,
		"user validation disagrees with the documented rules", "ok")
}

// countChunks splits a corpus once, at fixture time, to learn how many
// chunks it really produces. Never called inside a measured interval.
func countChunks(data []byte, opts *chunkers.ByteOpts) int {
	n := 0
	must(chunkers.SplitBytes(bytes.NewReader(data), opts, func([]byte) error { n++; return nil }))
	if n == 0 {
		n = 1
	}
	return n
}

// sortedKeys returns a map's keys in ascending order, so a fold over a Go
// map does not depend on iteration order.
func sortedKeys(m map[string][]byte) []string {
	out := make([]string, 0, len(m))
	for k := range m {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// sortedNames is a small helper the tree cases share.
func sortedNames(in [][]byte) [][]byte {
	out := make([][]byte, len(in))
	copy(out, in)
	sort.Slice(out, func(i, j int) bool { return bytes.Compare(out[i], out[j]) < 0 })
	return out
}

var _ = io.Discard
