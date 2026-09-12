package main

import (
	"bytes"
	"errors"
	"fmt"
	"io"

	"github.com/amber-store/core/amberpack"
	"github.com/amber-store/core/fstree"
	"github.com/amber-store/core/key"
)

// packContentDigest fingerprints what a pack carries rather than how it was
// encoded: the key and the logical payload length of every object, in stream
// order. Both cores must agree on this even though their compressed bytes
// differ.
func packContentDigest(objs []fstree.Object) string {
	items := make([][]byte, 0, len(objs)*2)
	for _, o := range objs {
		k := o.Key
		items = append(items, append([]byte(nil), k[:]...), o.Bytes)
	}
	return digestList(items)
}

func packCases(e *Env) []Case {
	fx := e.fx
	var out []Case

	// --- record codec -------------------------------------------------
	recSets := []payloadSet{fx.Tiny, fx.Small, fx.Text, fx.Large, fx.Rand}
	for _, ps := range recSets {
		ps := ps
		out = append(out, Case{
			Group: "amberpack", Op: "amberpack.encode_record", Workload: ps.Name, Threads: 1,
			Ops: len(ps.Items), Bytes: ps.Bytes,
			Setup: func(*Env) any {
				keys := make([]key.Key, len(ps.Items))
				for i, b := range ps.Items {
					keys[i] = mustV(key.New(key.Blob, uint64(len(b)), b))
				}
				return keys
			},
			Run: func(_ *Env, s any) uint64 {
				keys := s.([]key.Key)
				var acc uint64
				for i, b := range ps.Items {
					rec, err := amberpack.EncodeRecord(keys[i], b)
					if err != nil {
						panic(err)
					}
					acc += uint64(len(rec))
				}
				return acc
			},
		})
	}
	out = append(out,
		Case{
			Group: "amberpack", Op: "amberpack.parse_record", Workload: "mixed-records", Threads: 1,
			Ops:   len(fx.WireRecords),
			Setup: func(en *Env) any { return en.fx.WireRecords },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, rec := range s.([][]byte) {
					r, err := amberpack.ParseRecord(rec)
					if err != nil {
						panic(err)
					}
					acc += uint64(r.Slen)
				}
				return acc
			},
		},
		Case{
			Group: "amberpack", Op: "amberpack.parse_record", Workload: "corrupt-crc", Threads: 1,
			Ops: 256,
			Setup: func(en *Env) any {
				bad := append([]byte(nil), en.fx.WireRecords[0]...)
				bad[amberpack.RecHeaderSize] ^= 0xFF
				return bad
			},
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for i := 0; i < 256; i++ {
					if _, err := amberpack.ParseRecord(s.([]byte)); err != nil {
						acc++
					}
				}
				return acc
			},
		},
		Case{
			Group: "amberpack", Op: "amberpack.decode_payload", Workload: "mixed-records", Threads: 1,
			Ops: len(fx.WireRecords),
			Setup: func(en *Env) any {
				out := make([]parsedRecord, 0, len(en.fx.WireRecords))
				for _, rec := range en.fx.WireRecords {
					out = append(out, parsedRecord{mustV(amberpack.ParseRecord(rec)), rec})
				}
				return out
			},
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for _, p := range s.([]parsedRecord) {
					b, err := amberpack.DecodePayload(p.h.Flags, p.h.Ulen,
						p.rec[amberpack.RecHeaderSize:amberpack.RecHeaderSize+int(p.h.Slen)])
					if err != nil {
						panic(err)
					}
					acc += uint64(len(b))
				}
				return acc
			},
		},
		Case{
			Group: "amberpack", Op: "amberpack.writer_add", Workload: "mixed-objects", Threads: 1,
			Ops: len(fx.PackObjects), Bytes: packBytes(fx.PackObjects),
			Setup: func(en *Env) any { return en.fx.PackObjects },
			Run: func(_ *Env, s any) uint64 {
				var c countingWriter
				w := amberpack.NewWriter(&c)
				for _, o := range s.([]fstree.Object) {
					if err := w.Add(o); err != nil {
						panic(err)
					}
				}
				if err := w.Close(); err != nil {
					panic(err)
				}
				return uint64(c.n)
			},
		},
		Case{
			Group: "amberpack", Op: "amberpack.writer_add_record", Workload: "pre-encoded-records", Threads: 1,
			Ops: len(fx.WireRecords), Bytes: packBytes(fx.PackObjects),
			Setup: func(en *Env) any { return en.fx.WireRecords },
			Run: func(_ *Env, s any) uint64 {
				var c countingWriter
				w := amberpack.NewWriter(&c)
				for _, rec := range s.([][]byte) {
					if err := w.AddRecord(rec); err != nil {
						panic(err)
					}
				}
				if err := w.Close(); err != nil {
					panic(err)
				}
				return uint64(c.n)
			},
		},
		Case{
			Group: "amberpack", Op: "amberpack.reader_all", Workload: "mixed-objects", Threads: 1,
			Ops: len(fx.PackObjects), Bytes: packBytes(fx.PackObjects),
			Setup: func(en *Env) any { return en.fx.WirePack },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				r := amberpack.NewReader(bytes.NewReader(s.([]byte)))
				for o, err := range r.All() {
					if err != nil {
						panic(err)
					}
					acc += uint64(len(o.Bytes))
				}
				return acc
			},
		},
		Case{
			Group: "amberpack", Op: "amberpack.reader_records", Workload: "mixed-objects", Threads: 1,
			Ops:   len(fx.PackObjects),
			Setup: func(en *Env) any { return en.fx.WirePack },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				r := amberpack.NewReader(bytes.NewReader(s.([]byte)))
				for rec, err := range r.Records() {
					if err != nil {
						panic(err)
					}
					acc += uint64(len(rec.Bytes))
				}
				return acc
			},
		},
		Case{
			Group: "amberpack", Op: "amberpack.reader_all", Workload: "truncated-stream", Threads: 1,
			Ops:   64,
			Setup: func(en *Env) any { return en.fx.WirePack[:len(en.fx.WirePack)/2] },
			Run: func(_ *Env, s any) uint64 {
				var acc uint64
				for i := 0; i < 64; i++ {
					r := amberpack.NewReader(bytes.NewReader(s.([]byte)))
					for _, err := range r.All() {
						if err != nil {
							acc++
						}
					}
				}
				return acc
			},
		},
	)
	return out
}

// parsedRecord pairs a record's parsed header with its bytes, so the decode
// case measures only the payload decode.
type parsedRecord struct {
	h   amberpack.Record
	rec []byte
}

func packBytes(objs []fstree.Object) int64 {
	var n int64
	for _, o := range objs {
		n += int64(len(o.Bytes))
	}
	return n
}

func packChecks(e *Env) {
	fx := e.fx

	// A record round trips: encode, parse, decode, and the payload comes
	// back byte for byte. The keys and logical lengths are the cross-core
	// statement; the compressed bytes are not.
	var headers []string
	ok := true
	for i, o := range fx.PackObjects {
		rec, err := amberpack.EncodeRecord(o.Key, o.Bytes)
		if err != nil {
			ok = false
			break
		}
		h, err := amberpack.ParseRecord(rec)
		if err != nil || h.Key != o.Key || h.Ulen != uint32(len(o.Bytes)) {
			ok = false
			break
		}
		back, err := amberpack.DecodePayload(h.Flags, h.Ulen,
			rec[amberpack.RecHeaderSize:amberpack.RecHeaderSize+int(h.Slen)])
		if err != nil || !bytes.Equal(back, o.Bytes) {
			ok = false
			break
		}
		if i < 32 {
			headers = append(headers, fmt.Sprintf("%s/%d", h.Key, h.Ulen))
		}
	}
	e.want("amberpack", "amberpack.encode_record", "amberpack/record-roundtrip", ok,
		"a record did not round trip through encode/parse/decode", digestStrings(headers))
	e.passLocal("amberpack", "amberpack.encode_record", "amberpack/record-bytes",
		digestList(fx.WireRecords))

	// Compression is applied only when it strictly helps, so a random
	// payload stays stored uncompressed and a repetitive one shrinks.
	randRec := mustV(amberpack.EncodeRecord(
		mustV(key.New(key.Blob, uint64(len(fx.Rand.Items[0])), fx.Rand.Items[0])), fx.Rand.Items[0]))
	textRec := mustV(amberpack.EncodeRecord(
		mustV(key.New(key.Blob, uint64(len(fx.Large.Items[0])), fx.Large.Items[0])), fx.Large.Items[0]))
	rh, th := mustV(amberpack.ParseRecord(randRec)), mustV(amberpack.ParseRecord(textRec))
	e.want("amberpack", "amberpack.encode_record", "amberpack/compresses-only-when-smaller",
		rh.Slen == rh.Ulen && th.Slen < th.Ulen,
		fmt.Sprintf("random %d/%d, text %d/%d", rh.Slen, rh.Ulen, th.Slen, th.Ulen),
		fmt.Sprintf("random-stored=%v text-compressed=%v", rh.Slen == rh.Ulen, th.Slen < th.Ulen))

	// Corruption is detected rather than read through.
	bad := append([]byte(nil), fx.WireRecords[0]...)
	bad[amberpack.RecHeaderSize] ^= 0xFF
	_, err := amberpack.ParseRecord(bad)
	e.want("amberpack", "amberpack.parse_record", "amberpack/detects-crc-corruption",
		errors.Is(err, amberpack.ErrCorrupt),
		fmt.Sprintf("a flipped payload byte must wrap ErrCorrupt, got %v", err), "ErrCorrupt")
	badKey := append([]byte(nil), fx.WireRecords[0]...)
	badKey[1] |= 0x08 // set the key's reserved header bit
	_, err = amberpack.ParseRecord(badKey)
	e.want("amberpack", "amberpack.parse_record", "amberpack/rejects-non-canonical-key",
		errors.Is(err, amberpack.ErrCorrupt),
		fmt.Sprintf("a non-canonical key must wrap ErrCorrupt, got %v", err), "ErrCorrupt")

	// The reader returns exactly the objects the writer put in, in order.
	var readBack []fstree.Object
	r := amberpack.NewReader(bytes.NewReader(fx.WirePack))
	for o, err := range r.All() {
		if err != nil {
			e.fail("amberpack", "amberpack.reader_all", "amberpack/reader-roundtrip", err.Error())
			readBack = nil
			break
		}
		readBack = append(readBack, o)
	}
	if readBack != nil {
		same := len(readBack) == len(fx.PackObjects)
		for i := range readBack {
			if !same {
				break
			}
			if readBack[i].Key != fx.PackObjects[i].Key ||
				!bytes.Equal(readBack[i].Bytes, fx.PackObjects[i].Bytes) {
				same = false
			}
		}
		e.want("amberpack", "amberpack.reader_all", "amberpack/reader-roundtrip", same,
			"the reader did not return the objects the writer wrote",
			packContentDigest(readBack))
	}

	// Records() hands over the same records undecoded, and re-adding them
	// verbatim reproduces the stream.
	var raws [][]byte
	rr := amberpack.NewReader(bytes.NewReader(fx.WirePack))
	for rec, err := range rr.Records() {
		must(err)
		raws = append(raws, append([]byte(nil), rec.Bytes...))
	}
	var rebuilt bytes.Buffer
	w := amberpack.NewWriter(&rebuilt)
	for _, rec := range raws {
		must(w.AddRecord(rec))
	}
	must(w.Close())
	e.wantLocal("amberpack", "amberpack.writer_add_record", "amberpack/record-passthrough",
		bytes.Equal(rebuilt.Bytes(), fx.WirePack),
		"re-adding read records did not reproduce the pack byte for byte",
		digest(rebuilt.Bytes()))

	// A truncated stream is an error, not a clean end of pack.
	truncated := amberpack.NewReader(bytes.NewReader(fx.WirePack[:len(fx.WirePack)-1]))
	sawErr := false
	for _, err := range truncated.All() {
		if err != nil {
			sawErr = true
		}
	}
	e.want("amberpack", "amberpack.reader_all", "amberpack/rejects-truncated", sawErr,
		"a truncated pack was read as a clean stream", "rejected")
	// Legacy magics are rejected.
	legacy := append([]byte(nil), fx.WirePack...)
	legacy[7] = 0x01
	lr := amberpack.NewReader(bytes.NewReader(legacy))
	sawErr = false
	for _, err := range lr.All() {
		if err != nil && errors.Is(err, amberpack.ErrMalformed) {
			sawErr = true
		}
	}
	e.want("amberpack", "amberpack.reader_all", "amberpack/rejects-legacy-magic", sawErr,
		"a version-1 pack magic was accepted", "ErrMalformed")
	// An empty pack is still a well-formed stream.
	var empty bytes.Buffer
	ew := amberpack.NewWriter(&empty)
	must(ew.Close())
	n := 0
	er := amberpack.NewReader(bytes.NewReader(empty.Bytes()))
	for _, err := range er.All() {
		must(err)
		n++
	}
	e.want("amberpack", "amberpack.writer_add", "amberpack/empty-pack", n == 0 && empty.Len() == 9,
		fmt.Sprintf("an empty pack read back %d objects from %d bytes", n, empty.Len()),
		fmt.Sprintf("%d", empty.Len()))
}

var _ = io.Discard
