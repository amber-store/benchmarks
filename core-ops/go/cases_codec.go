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
	sets := []payloadSet{fx.Tiny, fx.Small, fx.Text, fx.Large, fx.Rand}
	var out []Case
	for _, ps := range sets {
		ps := ps
		out = append(out, Case{
			Group: "key", Op: "key.new", Workload: ps.Name, Threads: 1,
			Ops: len(ps.Items), Bytes: ps.Bytes,
			Setup: func(*Env) any { return ps },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, b := range s.(payloadSet).Items {
					k, err := key.New(key.Blob, uint64(len(b)), b)
					if err != nil {
						panic(err)
					}
					acc += uint64(k[0])
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
			Ops:   len(hashes),
			Setup: func(*Env) any { return hashes },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for i, h := range s.([][32]byte) {
					k, err := key.NewFromHash(key.Blob, uint64(i), h)
					if err != nil {
						panic(err)
					}
					acc += uint64(k[1])
				}
				return acc
			},
		},
		Case{
			Group: "key", Op: "key.parse", Workload: "canonical", Threads: 1,
			Ops:   len(fx.KeyBytes),
			Setup: func(*Env) any { return fx.KeyBytes },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, b := range s.([][]byte) {
					k, err := key.Parse(b)
					if err != nil {
						panic(err)
					}
					acc += uint64(k[2])
				}
				return acc
			},
		},
		Case{
			Group: "key", Op: "key.parse", Workload: "malformed", Threads: 1,
			Ops:   len(fx.BadKeyBytes) * 256,
			Setup: func(*Env) any { return fx.BadKeyBytes },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				bad := s.([][]byte)
				for i := 0; i < 256; i++ {
					for _, b := range bad {
						if _, err := key.Parse(b); err != nil {
							acc++
						}
					}
				}
				return acc
			},
		},
		Case{
			Group: "key", Op: "key.validate", Workload: "canonical", Threads: 1,
			Ops:   len(fx.Keys),
			Setup: func(*Env) any { return fx.Keys },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, k := range s.([]key.Key) {
					if k.Validate() == nil {
						acc++
					}
				}
				return acc
			},
		},
		// The header accessors are single field reads; timing each one
		// separately would measure the loop, not the core. They are batched
		// into one grouped case and the coverage matrix records that.
		Case{
			Group: "key", Op: "key.accessors", Workload: "type+length+length_size+hash", Threads: 1,
			Ops:   len(fx.Keys) * 4,
			Setup: func(*Env) any { return fx.Keys },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, k := range s.([]key.Key) {
					acc += uint64(k.Type()) + k.Length() + uint64(k.LengthSize()) + uint64(k.Hash()[0])
				}
				return acc
			},
		},
		Case{
			Group: "key", Op: "key.string", Workload: "hex", Threads: 1,
			Ops:   len(fx.Keys),
			Setup: func(*Env) any { return fx.Keys },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, k := range s.([]key.Key) {
					acc += uint64(len(k.String()))
				}
				return acc
			},
		},
		Case{
			Group: "key", Op: "key.type_string", Workload: "names", Threads: 1,
			Ops:   len(fx.Keys),
			Setup: func(*Env) any { return fx.Keys },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, k := range s.([]key.Key) {
					acc += uint64(len(k.Type().String()))
					if k.Type().IsValid() {
						acc++
					}
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
				Ops: reps, Bytes: int64(reps * len(enc)),
				Setup: func(*Env) any { return m },
				Run: func(_ *Env, s any) uint64 {
					var acc uint64
					for i := 0; i < reps; i++ {
						acc += uint64(len(encodeXattrs(s.(map[string][]byte))))
					}
					return acc
				},
			},
			{
				Group: "cbor", Op: "cbor.decode_xattrs", Workload: name, Threads: 1,
				Ops: reps, Bytes: int64(reps * len(enc)),
				Setup: func(*Env) any { return enc },
				Run: func(_ *Env, s any) uint64 {
					var acc uint64
					for i := 0; i < reps; i++ {
						m, err := decodeXattrs(s.([]byte))
						if err != nil {
							panic(err)
						}
						acc += uint64(len(m))
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
	corpora := []struct {
		name string
		data []byte
	}{
		{"random", fx.CorpusRandom},
		{"compressible", fx.CorpusText},
		{"tiny-64KiB", fx.CorpusText[:64<<10]},
	}
	for _, c := range corpora {
		c := c
		out = append(out, Case{
			Group: "chunkers", Op: "chunkers.split_bytes", Workload: c.name + "/default-sizes", Threads: 1,
			Ops: 1, Bytes: int64(len(c.data)),
			Setup: func(*Env) any { return c.data },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				err := chunkers.SplitBytes(bytes.NewReader(s.([]byte)), nil, func(chunk []byte) error {
					acc += uint64(len(chunk))
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
		Ops: 1, Bytes: int64(len(fx.CorpusText)),
		Setup: func(*Env) any { return fx.CorpusText },
		Run: func(_ *Env, s any) uint64 {
			var acc uint64
			err := chunkers.SplitBytes(bytes.NewReader(s.([]byte)), small, func(chunk []byte) error {
				acc += uint64(len(chunk))
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
			Ops:   len(fx.ItemEncodings),
			Setup: func(*Env) any { return chunkers.NewItemChunker(7) },
			Run: func(en *Env, s any) uint64 {
				ic := s.(chunkers.ItemChunker)
				var acc uint64
				run := 0
				for _, enc := range en.fx.ItemEncodings {
					run++
					if ic.IsBoundary(enc, run) {
						acc++
						run = 0
					}
				}
				return acc
			},
		},
		Case{
			Group: "chunkers", Op: "chunkers.new_item_chunker", Workload: "bits-4..12", Threads: 1,
			Ops:   9 * 256,
			Setup: func(*Env) any { return nil },
			Run: func(*Env, any) uint64 {
				var acc uint64
				for i := 0; i < 256; i++ {
					for bits := 4; bits <= 12; bits++ {
						ic := chunkers.NewItemChunker(bits)
						acc += uint64(ic.MinRun + ic.MaxRun)
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
			Ops: reps, Bytes: int64(reps * len(fx.RefRecordEnc)),
			Setup: func(*Env) any { return fx.RefRecord },
			Run: func(_ *Env, s any) uint64 {
				r := s.(reference.Reference)
				var acc uint64
				for i := 0; i < reps; i++ {
					b, err := r.Encode()
					if err != nil {
						panic(err)
					}
					acc += uint64(len(b))
				}
				return acc
			},
		},
		{
			Group: "reference", Op: "reference.decode", Workload: "signed", Threads: 1,
			Ops: reps, Bytes: int64(reps * len(fx.RefRecordEnc)),
			Setup: func(*Env) any { return fx.RefRecordEnc },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for i := 0; i < reps; i++ {
					r, err := reference.Decode(s.([]byte))
					if err != nil {
						panic(err)
					}
					acc += uint64(len(r.Name))
				}
				return acc
			},
		},
		{
			Group: "reference", Op: "reference.signature_payload", Workload: "signed", Threads: 1,
			Ops:   reps,
			Setup: func(*Env) any { return fx.RefRecord },
			Run: func(_ *Env, s any) uint64 {
				r := s.(reference.Reference)
				var acc uint64
				for i := 0; i < reps; i++ {
					b, err := r.SignaturePayload()
					if err != nil {
						panic(err)
					}
					acc += uint64(len(b))
				}
				return acc
			},
		},
		{
			Group: "reference", Op: "reference.validate_name", Workload: "valid", Threads: 1,
			Ops:   len(names),
			Setup: func(*Env) any { return names },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, n := range s.([]string) {
					if reference.ValidateName(n) == nil {
						acc++
					}
				}
				return acc
			},
		},
		{
			Group: "reference", Op: "reference.validate_user", Workload: "valid", Threads: 1,
			Ops:   len(names),
			Setup: func(*Env) any { return names },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, n := range s.([]string) {
					if reference.ValidateUser(n+"@example.org") == nil {
						acc++
					}
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

// sortedNames is a small helper the tree cases share.
func sortedNames(in [][]byte) [][]byte {
	out := make([][]byte, len(in))
	copy(out, in)
	sort.Slice(out, func(i, j int) bool { return bytes.Compare(out[i], out[j]) < 0 })
	return out
}

var _ = io.Discard
