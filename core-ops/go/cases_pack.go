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
	// Encoding is swept over the whole (size, content) grid, because this
	// is where the compressor's behaviour is the measurement: random bytes
	// are stored, text shrinks, and duplicates shrink the same way as text
	// but with a warm dictionary.
	//
	// The denominator is the *input* payload, which is byte-identical in
	// both cores. The encoded size is not, and is reported separately
	// rather than divided by anything.
	for _, ps := range fx.Payloads {
		ps := ps
		lim := ps.limit(4 << 20)
		out = append(out, Case{
			Group: "amberpack", Op: "amberpack.encode_record", Workload: lim.Name, Threads: 1,
			Ops: len(lim.Items), Bytes: lim.Bytes, BytesKind: BytesPayload,
			Dims: lim.dims(),
			Setup: func(*Env) any {
				keys := make([]key.Key, len(lim.Items))
				for i, b := range lim.Items {
					keys[i] = mustV(key.New(key.Blob, uint64(len(b)), b))
				}
				return keys
			},
			Run: func(_ *Env, s any) uint64 {
				keys := s.([]key.Key)
				acc := newFold()
				for i, b := range lim.Items {
					rec, err := amberpack.EncodeRecord(keys[i], b)
					if err != nil {
						panic(err)
					}
					acc = sinkBytes(acc, rec)
				}
				return acc
			},
		})
	}

	// --- the decoding side, on byte-identical input --------------------
	// Every reader and decoder case runs once per producing core, over the
	// pack that core wrote. Both drivers read the same two files, so
	// `producer-go` means the same bytes in both documents and the
	// comparison isolates the decoder instead of comparing two encoders.
	for _, w := range fx.Wire {
		w := w
		suffix := "/producer-" + w.Producer
		recDims := Dims{Items: int64(len(w.Records)), Content: "structured", Objects: int64(w.Objects)}
		out = append(out,
			Case{
				Group: "amberpack", Op: "amberpack.parse_record", Workload: "mixed-records" + suffix, Threads: 1,
				Ops: len(w.Records), Bytes: w.Bytes, BytesKind: BytesEncoded,
				Dims: recDims, CrossChecksum: true,
				Setup: func(*Env) any { return w.Records },
				Run: func(_ *Env, s any) uint64 {
					acc := newFold()
					for _, rec := range s.([][]byte) {
						r, err := amberpack.ParseRecord(rec)
						if err != nil {
							panic(err)
						}
						// The whole parsed header: key, both lengths and
						// the flags. Fixed size, so this is constant work.
						acc = foldKey(acc, r.Key)
						acc = foldU64(acc, uint64(r.Slen))
						acc = foldU64(acc, uint64(r.Ulen))
						acc = foldU64(acc, uint64(r.Flags))
					}
					return acc
				},
			},
			Case{
				Group: "amberpack", Op: "amberpack.decode_payload", Workload: "mixed-records" + suffix, Threads: 1,
				Ops: len(w.Records), Bytes: w.Bytes, BytesKind: BytesEncoded,
				Dims: recDims, CrossChecksum: true,
				Setup: func(*Env) any {
					outp := make([]parsedRecord, 0, len(w.Records))
					for _, rec := range w.Records {
						outp = append(outp, parsedRecord{mustV(amberpack.ParseRecord(rec)), rec})
					}
					return outp
				},
				Run: func(_ *Env, s any) uint64 {
					acc := newFold()
					for _, pr := range s.([]parsedRecord) {
						b, err := amberpack.DecodePayload(pr.h.Flags, pr.h.Ulen,
							pr.rec[amberpack.RecHeaderSize:amberpack.RecHeaderSize+int(pr.h.Slen)])
						if err != nil {
							panic(err)
						}
						acc = sinkBytes(acc, b)
					}
					return acc
				},
			},
			Case{
				Group: "amberpack", Op: "amberpack.reader_all", Workload: "mixed-objects" + suffix, Threads: 1,
				Ops: w.Objects, Bytes: w.Bytes, BytesKind: BytesEncoded,
				Dims: recDims, CrossChecksum: true,
				Setup: func(*Env) any { return w.Data },
				Run: func(_ *Env, s any) uint64 {
					acc := newFold()
					r := amberpack.NewReader(bytes.NewReader(s.([]byte)))
					for o, err := range r.All() {
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
				Group: "amberpack", Op: "amberpack.reader_records", Workload: "mixed-objects" + suffix, Threads: 1,
				Ops: w.Objects, Bytes: w.Bytes, BytesKind: BytesEncoded,
				Dims: recDims, CrossChecksum: true,
				Setup: func(*Env) any { return w.Data },
				Run: func(_ *Env, s any) uint64 {
					acc := newFold()
					r := amberpack.NewReader(bytes.NewReader(s.([]byte)))
					for rec, err := range r.Records() {
						if err != nil {
							panic(err)
						}
						acc = foldKey(acc, rec.Key)
						acc = sinkBytes(acc, rec.Bytes)
					}
					return acc
				},
			},
			Case{
				Group: "amberpack", Op: "amberpack.writer_add_record",
				Workload: "pre-encoded-records" + suffix, Threads: 1,
				Ops: len(w.Records), Bytes: w.Bytes, BytesKind: BytesEncoded,
				Dims: recDims, CrossChecksum: true,
				Setup: func(*Env) any { return w.Records },
				Run: func(_ *Env, s any) uint64 {
					var c countingWriter
					wr := amberpack.NewWriter(&c)
					for _, rec := range s.([][]byte) {
						if err := wr.AddRecord(rec); err != nil {
							panic(err)
						}
					}
					if err := wr.Close(); err != nil {
						panic(err)
					}
					return foldI64(newFold(), c.n)
				},
			},
			Case{
				Group: "amberpack", Op: "amberpack.reader_all", Workload: "truncated-stream" + suffix, Threads: 1,
				Ops:           64,
				Dims:          Dims{Items: 64, ItemBytes: w.Bytes / 2, Content: "structured"},
				CrossChecksum: true,
				Setup:         func(*Env) any { return w.Data[:len(w.Data)/2] },
				Run: func(_ *Env, s any) uint64 {
					acc := newFold()
					for i := 0; i < 64; i++ {
						r := amberpack.NewReader(bytes.NewReader(s.([]byte)))
						for o, err := range r.All() {
							acc = foldBool(acc, err != nil)
							if err == nil {
								acc = foldKey(acc, o.Key)
							}
						}
					}
					return acc
				},
			},
			Case{
				Group: "amberpack", Op: "amberpack.parse_record", Workload: "corrupt-crc" + suffix, Threads: 1,
				Ops:           256,
				Dims:          Dims{Items: 256, Content: "structured"},
				CrossChecksum: true,
				Setup: func(*Env) any {
					bad := append([]byte(nil), w.Records[0]...)
					bad[amberpack.RecHeaderSize] ^= 0xFF
					return bad
				},
				Run: func(_ *Env, s any) uint64 {
					acc := newFold()
					for i := 0; i < 256; i++ {
						_, err := amberpack.ParseRecord(s.([]byte))
						acc = foldBool(acc, err != nil)
					}
					return acc
				},
			},
		)
	}

	// --- the encoding side, this core's own writer ---------------------
	out = append(out,
		Case{
			Group: "amberpack", Op: "amberpack.writer_add", Workload: "mixed-objects", Threads: 1,
			Ops: len(fx.PackObjects), Bytes: packBytes(fx.PackObjects), BytesKind: BytesPayload,
			Dims:  Dims{Items: int64(len(fx.PackObjects)), Objects: int64(len(fx.PackObjects)), Content: "structured"},
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
				return foldI64(newFold(), c.n)
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

	// Record what this core's encoder made of every point of the payload
	// grid. The two cores' numbers are printed side by side and never
	// divided by one another.
	for _, ps := range fx.Payloads {
		lim := ps.limit(4 << 20)
		var enc int64
		for i, b := range lim.Items {
			k := mustV(key.New(key.Blob, uint64(len(b)), b))
			rec := mustV(amberpack.EncodeRecord(k, b))
			enc += int64(len(rec))
			_ = i
		}
		e.encoded("amberpack.encode_record", lim.Name, lim.Bytes, enc, int64(len(lim.Items)))
	}
	e.encoded("amberpack.writer_add", "mixed-objects",
		packBytes(fx.PackObjects), int64(len(fx.WirePack)), int64(len(fx.PackObjects)))

	// Every wire pack this driver was handed, with its producer and hash.
	// Both drivers read the same files, so these rows are what proves a
	// decode comparison used identical bytes.
	for _, w := range fx.Wire {
		e.wireInputs = append(e.wireInputs, WireInput{
			Producer: w.Producer, SHA256: w.SHA256,
			Bytes: w.Bytes, Objects: int64(w.Objects),
		})
		// The hash of each shared input is a comparable statement: if the
		// two drivers somehow read different files, the run fails here
		// rather than publishing a decode comparison of unlike inputs.
		e.pass("amberpack", "amberpack.reader_all",
			"amberpack/wire-input-"+w.Producer, w.SHA256)
	}

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

	// Interoperability, which is the thing the decode comparison rests on:
	// this core reads *every* producer's pack and must recover exactly the
	// same objects from each. The digest is over the (key, payload) list,
	// so it is comparable across cores even though the packs are not.
	for _, w := range fx.Wire {
		var got []fstree.Object
		var rerr error
		r := amberpack.NewReader(bytes.NewReader(w.Data))
		for o, err := range r.All() {
			if err != nil {
				rerr = err
				break
			}
			got = append(got, o)
		}
		same := rerr == nil && len(got) == len(fx.PackObjects)
		if same {
			for i := range got {
				if got[i].Key != fx.PackObjects[i].Key ||
					!bytes.Equal(got[i].Bytes, fx.PackObjects[i].Bytes) {
					same = false
					break
				}
			}
		}
		e.want("amberpack", "amberpack.reader_all",
			"amberpack/reads-"+w.Producer+"-pack", same,
			fmt.Sprintf("reading the %s core's pack did not return the shared object population: %v",
				w.Producer, rerr),
			packContentDigest(got))
	}

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

	// Records() hands over the same records undecoded, and re-adding them
	// verbatim reproduces the stream — for every producer's pack, which is
	// what the writer_add_record workloads measure.
	for _, w := range fx.Wire {
		var raws [][]byte
		rr := amberpack.NewReader(bytes.NewReader(w.Data))
		for rec, err := range rr.Records() {
			must(err)
			raws = append(raws, append([]byte(nil), rec.Bytes...))
		}
		var rebuilt bytes.Buffer
		wr := amberpack.NewWriter(&rebuilt)
		for _, rec := range raws {
			must(wr.AddRecord(rec))
		}
		must(wr.Close())
		e.want("amberpack", "amberpack.writer_add_record",
			"amberpack/record-passthrough-"+w.Producer,
			bytes.Equal(rebuilt.Bytes(), w.Data),
			"re-adding read records did not reproduce the pack byte for byte",
			// The pack is the same file in both drivers, so this digest is
			// a comparable statement about a byte-identical artefact.
			w.SHA256)
	}

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
