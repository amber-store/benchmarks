package main

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"sync"

	"github.com/amber-store/core/amberpack"
	"github.com/amber-store/core/chunkers"
	"github.com/amber-store/core/fstree"
	"github.com/amber-store/core/ingest"
	"github.com/amber-store/core/key"
	"github.com/amber-store/core/packstore"
	"github.com/amber-store/core/reference"
	"github.com/amber-store/core/refstore"
)

// payloadSet is a batch of byte strings a short operation is run over. Short
// operations are always measured over a whole set: one call is far below the
// clock's resolution, a set of a few thousand is not.
type payloadSet struct {
	Name  string
	Items [][]byte
	Bytes int64
}

func newPayloadSet(name string, count, size int, seed uint64, text bool) payloadSet {
	ps := payloadSet{Name: name, Items: make([][]byte, count)}
	for i := range ps.Items {
		s := seed + uint64(i)*0x100000001B3
		if text {
			ps.Items[i] = compressibleBytes(s, size)
		} else {
			ps.Items[i] = randomBytes(s, size)
		}
		ps.Bytes += int64(size)
	}
	return ps
}

// memStore is the in-memory object bag the pure tree cases read through. It
// is concurrency-safe because the walks (ReachableKeys, CheckComplete) call
// get from several goroutines.
type memStore struct {
	mu sync.RWMutex
	m  map[key.Key][]byte
}

func newMemStore() *memStore { return &memStore{m: map[key.Key][]byte{}} }

func (s *memStore) put(o fstree.Object) error {
	s.mu.Lock()
	defer s.mu.Unlock()
	if _, ok := s.m[o.Key]; !ok {
		s.m[o.Key] = o.Bytes
	}
	return nil
}

func (s *memStore) get(k key.Key) ([]byte, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	b, ok := s.m[k]
	if !ok {
		return nil, fmt.Errorf("memstore: %s not found", k)
	}
	return b, nil
}

func (s *memStore) has(k key.Key) (bool, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	_, ok := s.m[k]
	return ok, nil
}

func (s *memStore) drop(k key.Key) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.m, k)
}

func (s *memStore) len() int {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return len(s.m)
}

// Fixtures holds everything the cases read. It is built once, before any
// measurement, and never mutated by a measured operation.
type Fixtures struct {
	// Byte corpora.
	CorpusRandom []byte
	CorpusText   []byte

	// Batched payloads for the short codec operations.
	Tiny  payloadSet // 64 B, random
	Small payloadSet // 4 KiB, random
	Text  payloadSet // 4 KiB, compressible
	Large payloadSet // 1 MiB, compressible
	Rand  payloadSet // 1 MiB, random

	// Keys: canonical ones to parse and validate, and malformed ones whose
	// rejection is part of the measured contract.
	Keys        []key.Key
	KeyBytes    [][]byte
	BadKeyBytes [][]byte

	// Extended-attribute maps, small and large.
	XattrsSmall    map[string][]byte
	XattrsLarge    map[string][]byte
	XattrsSmallEnc []byte
	XattrsLargeEnc []byte

	// fstree codec inputs and their encodings.
	EntriesSmall     []fstree.Entry
	EntriesLarge     []fstree.Entry
	PairsSmall       []fstree.DirPair
	PairsLarge       []fstree.DirPair
	ChildrenSmall    []key.Key
	ChildrenLarge    []key.Key
	EncDirLeafSmall  []byte
	EncDirLeafLarge  []byte
	EncDirNodeSmall  []byte
	EncDirNodeLarge  []byte
	EncFileNodeSmall []byte
	EncFileNodeLarge []byte
	ItemEncodings    [][]byte

	// In-memory trees.
	Mem             *memStore
	WideRoot        key.Key
	WideNames       [][]byte // present names, ascending
	MissName        []byte   // a name that is not in the wide directory
	DeepRoot        key.Key
	DeepPath        string
	ShallowRoot     key.Key
	FileRoot        key.Key
	FileBytes       int64
	IncompleteRoot  key.Key // a wide tree with one Blob leaf removed
	IncompleteStore *memStore

	// On-disk trees.
	TreeV1 string
	TreeV2 string
	// IngestTemplate is a packstore that already holds the V1 tree, so the
	// incremental-change case measures a re-ingest against real dedup hits.
	IngestTemplate string
	TreeBytes      int64
	TreeFiles      int64
	IgnoreDir      string

	// Pack fixtures.
	PackObjects []fstree.Object
	WirePack    []byte
	WireRecords [][]byte

	// Store templates: copied per repetition by the destructive cases.
	StoreTemplate   string // packstore with StoreObjects objects over several segments
	StoreKeys       []key.Key
	StoreMissKeys   []key.Key
	StoreSegments   []packstore.SegmentInfo
	GarbageTemplate string // packstore whose sealed segments are mostly dead
	GarbageLive     []key.Key
	RefsTemplate    string // refstore with RefRecords records
	RefNames        []string
	GCTemplate      string // objects+refs+gc dirs with real reclaimable packs
	GCLiveRoot      key.Key
	InboxTemplate   string // inbox directory holding InboxPacks staged entries
	InboxPacks      [][]byte
	InboxRoots      []key.Key

	// Reference records.
	RefRecord    reference.Reference
	RefRecordEnc []byte
	RefBatch     []refstore.Record

	// Tar: the ingested fixture tree in memory, plus the archive exported
	// from it.
	TreeMem  *memStore
	TreeRoot key.Key
	TarBytes []byte

	// A single open copy of the store template, shared by every read-only
	// packstore case so the measured interval is the lookup and not an
	// mmap of the whole fixture.
	RO      *storeHandle
	ROLocs  []recordLoc
	ROSegID uint64
}

// recordLoc is one record's position: the sealed segment it lives in and its
// offset inside that segment's body.
type recordLoc struct {
	ID  uint64
	Off uint64
	Len uint32
}

const (
	modeReg = 0o100644
	modeDir = 0o040755
	modeLnk = 0o120777
)

func buildFixtures(e *Env) *Fixtures {
	p := e.Profile
	fx := &Fixtures{}
	seed := p.Seed

	fx.CorpusRandom = randomBytes(seed+1, int(p.CorpusBytes))
	fx.CorpusText = compressibleBytes(seed+2, int(p.CorpusBytes))

	batch := p.BatchOps
	fx.Tiny = newPayloadSet("tiny-64B-random", batch, 64, seed+10, false)
	fx.Small = newPayloadSet("small-4KiB-random", maxInt(batch/8, 32), 4<<10, seed+11, false)
	fx.Text = newPayloadSet("small-4KiB-text", maxInt(batch/8, 32), 4<<10, seed+12, true)
	fx.Large = newPayloadSet("large-1MiB-text", maxInt(batch/256, 4), 1<<20, seed+13, true)
	fx.Rand = newPayloadSet("large-1MiB-random", maxInt(batch/256, 4), 1<<20, seed+14, false)

	buildKeyFixtures(fx, p)
	buildXattrFixtures(fx, p)
	buildCodecFixtures(fx, p)
	buildMemTrees(e, fx)
	buildDiskTrees(e, fx)
	buildPackFixtures(e, fx)
	buildStoreTemplates(e, fx)
	buildRefFixtures(e, fx)
	buildInboxTemplate(e, fx)
	buildGCTemplate(e, fx)
	buildTarFixture(e, fx)
	buildIngestTemplate(e, fx)
	buildReadStore(e, fx)
	return fx
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func buildKeyFixtures(fx *Fixtures, p Profile) {
	r := newRNG(p.Seed + 20)
	types := []key.Type{key.Blob, key.FileNode, key.DirLeaf, key.DirNode, key.XattrSet}
	for i := 0; i < p.BatchOps; i++ {
		var h [32]byte
		copy(h[:], randomBytes(p.Seed+20+uint64(i), 32))
		length := uint64(r.next() % (1 << 40))
		k, err := key.NewFromHash(types[i%len(types)], length, h)
		must(err)
		fx.Keys = append(fx.Keys, k)
		kk := k
		fx.KeyBytes = append(fx.KeyBytes, append([]byte(nil), kk[:]...))
	}
	// Malformed keys: the reserved header bit, a reserved object type, a
	// non-canonical (zero-padded) length and a short buffer. Their rejection
	// is a measured path, not just a check.
	base := append([]byte(nil), fx.KeyBytes[0]...)
	reserved := append([]byte(nil), base...)
	reserved[0] |= 0x08
	badType := append([]byte(nil), base...)
	badType[0] = (badType[0] & 0x0F) | 0x50
	padded, err := key.NewFromHash(key.Blob, 1<<16, [32]byte{})
	must(err)
	pad := padded
	nonCanon := append([]byte(nil), pad[:]...)
	nonCanon[0] = (nonCanon[0] & 0xF0) | 0x04 // claim 4 length bytes
	nonCanon[1] = 0x00                        // leading zero: non-canonical
	short := base[:31]
	fx.BadKeyBytes = [][]byte{reserved, badType, nonCanon, short}
}

func buildXattrFixtures(fx *Fixtures, p Profile) {
	fx.XattrsSmall = map[string][]byte{
		"user.amber.a": []byte("1"),
		"user.amber.b": []byte("value"),
		"user.mime":    []byte("application/octet-stream"),
	}
	fx.XattrsLarge = map[string][]byte{}
	for i := 0; i < 64; i++ {
		fx.XattrsLarge[fmt.Sprintf("user.amber.attr%03d", i)] =
			randomBytes(p.Seed+30+uint64(i), 48)
	}
	fx.XattrsSmallEnc = encodeXattrs(fx.XattrsSmall)
	fx.XattrsLargeEnc = encodeXattrs(fx.XattrsLarge)
}

// entryFor builds one deterministic directory entry.
func entryFor(i int, seed uint64, withXattrs []byte) fstree.Entry {
	h := randomBytes(seed+uint64(i), 32)
	var arr [32]byte
	copy(arr[:], h)
	ck, err := key.NewFromHash(key.Blob, uint64(1024+i), arr)
	must(err)
	ckb := ck
	e := fstree.Entry{
		Name:       []byte(fmt.Sprintf("entry-%08d", i)),
		Mode:       modeReg,
		UID:        1000,
		GID:        1000,
		Mtime:      int64(1_700_000_000_000_000_000 + int64(i)*1_000_000),
		ContentKey: append([]byte(nil), ckb[:]...),
	}
	if withXattrs != nil {
		e.XattrsIn = withXattrs
	}
	return e
}

func buildCodecFixtures(fx *Fixtures, p Profile) {
	for i := 0; i < 8; i++ {
		fx.EntriesSmall = append(fx.EntriesSmall, entryFor(i, p.Seed+40, nil))
	}
	for i := 0; i < 128; i++ {
		var xa []byte
		if i%8 == 0 {
			xa = fx.XattrsSmallEnc
		}
		fx.EntriesLarge = append(fx.EntriesLarge, entryFor(i, p.Seed+40, xa))
	}
	mkPairs := func(n int) []fstree.DirPair {
		out := make([]fstree.DirPair, 0, n)
		for i := 0; i < n; i++ {
			var arr [32]byte
			copy(arr[:], randomBytes(p.Seed+50+uint64(i), 32))
			k, err := key.NewFromHash(key.DirLeaf, uint64(4096+i), arr)
			must(err)
			kk := k
			out = append(out, fstree.DirPair{
				SepName:  []byte(fmt.Sprintf("entry-%08d", i*7)),
				ChildKey: append([]byte(nil), kk[:]...),
			})
		}
		return out
	}
	fx.PairsSmall, fx.PairsLarge = mkPairs(8), mkPairs(128)

	mkChildren := func(n int) []key.Key {
		out := make([]key.Key, 0, n)
		for i := 0; i < n; i++ {
			var arr [32]byte
			copy(arr[:], randomBytes(p.Seed+60+uint64(i), 32))
			k, err := key.NewFromHash(key.Blob, uint64(65536), arr)
			must(err)
			out = append(out, k)
		}
		return out
	}
	fx.ChildrenSmall, fx.ChildrenLarge = mkChildren(8), mkChildren(1024)

	fx.EncDirLeafSmall = mustV(fstree.EncodeDirLeaf(fx.EntriesSmall)).Bytes
	fx.EncDirLeafLarge = mustV(fstree.EncodeDirLeaf(fx.EntriesLarge)).Bytes
	fx.EncDirNodeSmall = mustV(fstree.EncodeDirNode(fx.PairsSmall)).Bytes
	fx.EncDirNodeLarge = mustV(fstree.EncodeDirNode(fx.PairsLarge)).Bytes
	fx.EncFileNodeSmall = mustV(fstree.EncodeFileNode(fx.ChildrenSmall)).Bytes
	fx.EncFileNodeLarge = mustV(fstree.EncodeFileNode(fx.ChildrenLarge)).Bytes

	// The item chunker decides boundaries on the canonical encoding of one
	// item; feed it the same encodings the builders would.
	for i := 0; i < p.BatchOps; i++ {
		k := fx.Keys[i%len(fx.Keys)]
		kk := k
		fx.ItemEncodings = append(fx.ItemEncodings, append([]byte(nil), kk[:]...))
	}
}

// buildMemTrees builds the in-memory directory and file trees the read paths
// walk: one wide directory (a real prolly tree with index levels), one deep
// chain of directories, one shallow directory, and one multi-level file.
func buildMemTrees(e *Env, fx *Fixtures) {
	p := e.Profile
	fx.Mem = newMemStore()
	ic := chunkers.NewItemChunker(ingest.DefaultItemBits)

	// Every entry points at a Blob that really is in the store, so the
	// completeness walk has something to find and the missing-leaf case is
	// a deliberate single removal rather than an empty store.
	storedEntry := func(i int, seed uint64) fstree.Entry {
		body := []byte(fmt.Sprintf("amber-core-ops leaf %016x/%08d", seed, i))
		o := mustV(fstree.EncodeBlob(body))
		must(fx.Mem.put(o))
		en := entryFor(i, seed, nil)
		en.ContentKey = keyBytes(o.Key)
		return en
	}

	build := func(n int, seed uint64) (key.Key, [][]byte) {
		db := fstree.NewDirBuilder(ic)
		names := make([][]byte, 0, n)
		for i := 0; i < n; i++ {
			en := storedEntry(i, seed)
			names = append(names, en.Name)
			must(db.AddEntry(fx.Mem.put, en))
		}
		root, err := db.Finish(fx.Mem.put)
		must(err)
		return root, names
	}

	fx.WideRoot, fx.WideNames = build(p.SyntheticWide, p.Seed+70)
	fx.MissName = []byte("entry-zzzzzzzz")
	fx.ShallowRoot, _ = build(16, p.Seed+71)

	// A chain of directories, each holding the next one plus a little noise,
	// so path resolution really descends TreeDepth levels.
	child := fx.ShallowRoot
	var comps []string
	for d := p.TreeDepth - 1; d >= 0; d-- {
		db := fstree.NewDirBuilder(ic)
		name := fmt.Sprintf("level%03d", d)
		ck := child
		// Entries must be added in bytewise name order: "entry-..." sorts
		// before "level...".
		must(db.AddEntry(fx.Mem.put, storedEntry(d, p.Seed+72)))
		must(db.AddEntry(fx.Mem.put, fstree.Entry{
			Name: []byte(name), Mode: modeDir, UID: 1000, GID: 1000,
			Mtime: 1_700_000_000_000_000_000, ContentKey: append([]byte(nil), ck[:]...),
		}))
		root, err := db.Finish(fx.Mem.put)
		must(err)
		child = root
		comps = append([]string{name}, comps...)
	}
	fx.DeepRoot = child
	fx.DeepPath = filepath.Join(comps...)

	// One file built the way ingest builds files: content-defined chunks
	// under FileNode index levels.
	fb := fstree.NewFileIndexBuilder(ic)
	nblobs := 0
	must(chunkers.SplitBytes(bytes.NewReader(fx.CorpusText), nil, func(chunk []byte) error {
		o, err := fstree.EncodeBlob(chunk)
		if err != nil {
			return err
		}
		if err := fx.Mem.put(o); err != nil {
			return err
		}
		nblobs++
		return fb.AddChild(fx.Mem.put, o.Key, nil)
	}))
	fx.FileRoot = mustV(fb.Finish(fx.Mem.put))
	fx.FileBytes = int64(len(fx.CorpusText))

	// A copy of the shallow tree with one leaf missing, for the
	// completeness check's failure path.
	fx.IncompleteStore = newMemStore()
	fx.Mem.mu.RLock()
	for k, v := range fx.Mem.m {
		fx.IncompleteStore.m[k] = v
	}
	fx.Mem.mu.RUnlock()
	fx.IncompleteRoot = fx.ShallowRoot
	entries := mustV(fstree.CollectEntries(fx.ShallowRoot, fx.IncompleteStore.get))
	missing := mustV(key.Parse(entries[0].ContentKey))
	fx.IncompleteStore.drop(missing)
}

// buildDiskTrees materializes the on-disk fixture tree and its incrementally
// modified successor.
func buildDiskTrees(e *Env, fx *Fixtures) {
	p := e.Profile
	fx.TreeV1 = filepath.Join(e.Scratch, "tree-v1")
	fx.TreeV2 = filepath.Join(e.Scratch, "tree-v2")
	fx.IgnoreDir = filepath.Join(e.Scratch, "ignore-fixture")
	if err := os.RemoveAll(fx.TreeV1); err != nil {
		panic(err)
	}
	must(os.RemoveAll(fx.TreeV2))
	must(os.RemoveAll(fx.IgnoreDir))

	files, bytesTotal := writeFixtureTree(fx.TreeV1, p, e.Xattrs)
	fx.TreeFiles, fx.TreeBytes = files, bytesTotal
	must(stampTree(fx.TreeV1))
	must(copyTree(fx.TreeV1, fx.TreeV2))
	mutateFixtureTree(fx.TreeV2, p)
	must(stampTree(fx.TreeV2))
	writeIgnoreFixture(fx.IgnoreDir, p)
	must(stampTree(fx.IgnoreDir))
}

// writeFixtureTree lays out the deterministic source tree. Both drivers build
// it from the same spec; the manifest digest check proves they agreed.
func writeFixtureTree(root string, p Profile, xattrs bool) (files, total int64) {
	must(os.MkdirAll(root, 0o755))
	write := func(rel string, b []byte) {
		full := filepath.Join(root, rel)
		must(os.MkdirAll(filepath.Dir(full), 0o755))
		must(os.WriteFile(full, b, 0o644))
		files++
		total += int64(len(b))
	}
	must(os.WriteFile(filepath.Join(root, ".amberignore"),
		[]byte("*.tmp\n!keep.tmp\n/build/\n"), 0o644))
	files++

	r := newRNG(p.Seed + 80)
	dirs := maxInt(p.TreeFiles/10, 4)
	for d := 0; d < dirs; d++ {
		for f := 0; f < 10; f++ {
			idx := d*10 + f
			size := 1 << uint(6+r.n(9)) // 64 B .. 16 KiB
			if idx%97 == 0 {
				size = 3 << 20 // a few multi-chunk files
			}
			var b []byte
			if idx%3 == 0 {
				b = randomBytes(p.Seed+1000+uint64(idx), size)
			} else {
				b = compressibleBytes(p.Seed+1000+uint64(idx), size)
			}
			write(fmt.Sprintf("data/d%03d/f%03d.bin", d, f), b)
		}
		write(fmt.Sprintf("data/d%03d/scratch.tmp", d), []byte("ignored\n"))
		write(fmt.Sprintf("data/d%03d/keep.tmp", d), []byte("kept by negation\n"))
	}
	for i := 0; i < 8; i++ {
		write(fmt.Sprintf("build/gen%d.bin", i), compressibleBytes(p.Seed+2000+uint64(i), 4096))
	}
	for i := 0; i < p.TreeWide; i++ {
		write(fmt.Sprintf("wide/w%06d.bin", i), randomBytes(p.Seed+3000+uint64(i), 64+i%448))
	}
	deep := "deep"
	for d := 0; d < p.TreeDepth; d++ {
		deep = filepath.Join(deep, fmt.Sprintf("l%02d", d))
	}
	write(filepath.Join(deep, "leaf.bin"), compressibleBytes(p.Seed+4000, 8192))
	write("big.bin", compressibleBytes(p.Seed+5000, int(p.CorpusBytes/8)))

	must(os.MkdirAll(filepath.Join(root, "links"), 0o755))
	for i := 0; i < 8; i++ {
		must(os.Symlink(fmt.Sprintf("../data/d000/f%03d.bin", i),
			filepath.Join(root, "links", fmt.Sprintf("ln%03d", i))))
	}
	if xattrs {
		must(setXattr(filepath.Join(root, "data/d000/f000.bin"), "user.amber.small", []byte("v1")))
		must(setXattr(filepath.Join(root, "data/d000/f001.bin"), "user.amber.large",
			randomBytes(p.Seed+6000, 512)))
	}
	return files, total
}

// mutateFixtureTree applies the deterministic incremental change: a few files
// grow, a few appear and a few vanish. Re-ingesting the result is the
// incremental-change workload.
func mutateFixtureTree(root string, p Profile) {
	dirs := maxInt(p.TreeFiles/10, 4)
	for d := 0; d < dirs; d += 7 {
		path := filepath.Join(root, fmt.Sprintf("data/d%03d/f000.bin", d))
		b := mustV(os.ReadFile(path))
		b = append(b, compressibleBytes(p.Seed+7000+uint64(d), 4096)...)
		must(os.WriteFile(path, b, 0o644))
	}
	for i := 0; i < 16; i++ {
		must(os.WriteFile(filepath.Join(root, fmt.Sprintf("data/new%03d.bin", i)),
			compressibleBytes(p.Seed+8000+uint64(i), 2048), 0o644))
	}
	for d := 1; d < dirs && d < 1+8; d++ {
		must(os.Remove(filepath.Join(root, fmt.Sprintf("data/d%03d/f009.bin", d))))
	}
}

// writeIgnoreFixture builds a directory whose .amberignore files stack, so
// Descend and Ignored are measured against a real pattern chain.
func writeIgnoreFixture(root string, p Profile) {
	must(os.MkdirAll(root, 0o755))
	must(os.WriteFile(filepath.Join(root, ".amberignore"),
		[]byte("*.o\n*.tmp\n!keep.tmp\n/vendor/\n**/target/\n"), 0o644))
	for d := 0; d < 8; d++ {
		sub := filepath.Join(root, fmt.Sprintf("sub%02d", d))
		must(os.MkdirAll(sub, 0o755))
		if d%2 == 0 {
			must(os.WriteFile(filepath.Join(sub, ".amberignore"),
				[]byte(fmt.Sprintf("gen%02d-*\n!gen%02d-keep\n", d, d)), 0o644))
		}
	}
}

func buildPackFixtures(e *Env, fx *Fixtures) {
	p := e.Profile
	n := maxInt(p.BatchOps/4, 64)
	for i := 0; i < n; i++ {
		var b []byte
		switch i % 4 {
		case 0:
			b = randomBytes(p.Seed+9000+uint64(i), 512)
		case 1:
			b = compressibleBytes(p.Seed+9000+uint64(i), 4096)
		case 2:
			b = randomBytes(p.Seed+9000+uint64(i), 64<<10)
		default:
			b = compressibleBytes(p.Seed+9000+uint64(i), 64<<10)
		}
		o, err := fstree.EncodeBlob(b)
		must(err)
		fx.PackObjects = append(fx.PackObjects, o)
	}
	var buf bytes.Buffer
	w := amberpack.NewWriter(&buf)
	for _, o := range fx.PackObjects {
		must(w.Add(o))
	}
	must(w.Close())
	fx.WirePack = buf.Bytes()
	for _, o := range fx.PackObjects {
		rec, err := amberpack.EncodeRecord(o.Key, o.Bytes)
		must(err)
		fx.WireRecords = append(fx.WireRecords, rec)
	}
}

// storeObjects is the deterministic object population of the packstore
// fixtures: a mix of tiny and large, compressible and random payloads.
func storeObjects(p Profile, n int, salt uint64) []fstree.Object {
	out := make([]fstree.Object, 0, n)
	for i := 0; i < n; i++ {
		s := p.Seed + salt + uint64(i)*0x100000001B3
		var b []byte
		switch i % 5 {
		case 0:
			b = randomBytes(s, 128)
		case 1:
			b = compressibleBytes(s, 1024)
		case 2:
			b = randomBytes(s, 8<<10)
		case 3:
			b = compressibleBytes(s, 32<<10)
		default:
			b = randomBytes(s, 2<<10)
		}
		o, err := fstree.EncodeBlob(b)
		must(err)
		out = append(out, o)
	}
	return out
}

func objectSeq(objs []fstree.Object) func(func(packstore.Object, error) bool) {
	return func(yield func(packstore.Object, error) bool) {
		for _, o := range objs {
			if !yield(packstore.Object{Key: o.Key, Data: o.Bytes}, nil) {
				return
			}
		}
	}
}

func buildStoreTemplates(e *Env, fx *Fixtures) {
	p := e.Profile
	fx.StoreTemplate = filepath.Join(e.Scratch, "store-template")
	must(os.RemoveAll(fx.StoreTemplate))
	must(os.MkdirAll(fx.StoreTemplate, 0o755))
	objs := storeObjects(p, p.StoreObjects, 10000)
	st := mustV(packstore.Open(fx.StoreTemplate,
		packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
	_, err := st.WriteParallel(objectSeq(objs), packstore.WriteOpts{Writers: p.ThreadsMulti})
	must(err)
	must(st.Sync())
	for _, o := range objs {
		fx.StoreKeys = append(fx.StoreKeys, o.Key)
	}
	fx.StoreSegments = mustV(st.Segments())
	must(st.Close())

	// Keys that are definitely absent, for the miss paths.
	for _, o := range storeObjects(p, maxInt(p.BatchOps, 1024), 20000) {
		fx.StoreMissKeys = append(fx.StoreMissKeys, o.Key)
	}

	// A store whose sealed segments are mostly garbage: only every tenth
	// object stays live, so compaction and the sweep have real work and the
	// retained objects can be verified afterwards.
	fx.GarbageTemplate = filepath.Join(e.Scratch, "store-garbage")
	must(os.RemoveAll(fx.GarbageTemplate))
	must(os.MkdirAll(fx.GarbageTemplate, 0o755))
	gst := mustV(packstore.Open(fx.GarbageTemplate,
		packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
	_, err = gst.WriteParallel(objectSeq(objs), packstore.WriteOpts{Writers: p.ThreadsMulti})
	must(err)
	must(gst.Sync())
	must(gst.Close())
	for i, o := range objs {
		if i%10 == 0 {
			fx.GarbageLive = append(fx.GarbageLive, o.Key)
		}
	}
}

// buildReadStore opens one long-lived copy of the store template and records
// where its records live, for the index and raw-record cases.
func buildReadStore(e *Env, fx *Fixtures) {
	dir := filepath.Join(e.Scratch, "store-readonly")
	must(os.RemoveAll(dir))
	must(copyTree(fx.StoreTemplate, dir))
	st := mustV(packstore.Open(dir,
		packstore.WithSegmentSize(e.Profile.SegmentBytes), packstore.WithSync(false)))
	fx.RO = &storeHandle{dir: dir, st: st}
	segs := mustV(st.Segments())
	if len(segs) == 0 {
		panic("the store fixture has no sealed segment; raise StoreObjects or lower SegmentBytes")
	}
	fx.ROSegID = segs[0].ID
	must(st.ScanIndex(segs[0].ID, func(k key.Key, off uint64, slen uint32) {
		fx.ROLocs = append(fx.ROLocs, recordLoc{ID: segs[0].ID, Off: off, Len: slen})
	}))
}

func buildRefFixtures(e *Env, fx *Fixtures) {
	p := e.Profile
	k := fx.StoreKeys[0]
	kk := k
	fx.RefRecord = reference.Reference{
		Name:      "bench/reference/main",
		Key:       append([]byte(nil), kk[:]...),
		User:      "bench@example.org",
		CreatedAt: 1_700_000_000_000_000_000,
		Signature: randomBytes(p.Seed+11000, 128),
		PublicKey: randomBytes(p.Seed+11001, 96),
	}
	fx.RefRecordEnc = mustV(fx.RefRecord.Encode())

	for i := 0; i < p.RefRecords; i++ {
		kx := fx.StoreKeys[i%len(fx.StoreKeys)]
		kb := kx
		rec := reference.Reference{
			Name:      fmt.Sprintf("bench/ref/%06d", i),
			Key:       append([]byte(nil), kb[:]...),
			User:      "bench@example.org",
			CreatedAt: int64(1_700_000_000_000_000_000 + int64(i)),
		}
		enc := mustV(rec.Encode())
		fx.RefNames = append(fx.RefNames, rec.Name)
		fx.RefBatch = append(fx.RefBatch, refstore.Record{Name: rec.Name, Data: enc})
	}

	fx.RefsTemplate = filepath.Join(e.Scratch, "refs-template")
	must(os.RemoveAll(fx.RefsTemplate))
	rs := mustV(refstore.Open(fx.RefsTemplate, false))
	must(rs.PutBatch(fx.RefBatch))
	must(rs.Close())
}

func buildInboxTemplate(e *Env, fx *Fixtures) {
	p := e.Profile
	// Each inbox entry is one wire pack holding a disjoint slice of a fresh
	// object population, so a drain really stores new objects.
	objs := storeObjects(p, p.InboxPacks*32, 30000)
	for i := 0; i < p.InboxPacks; i++ {
		var buf bytes.Buffer
		w := amberpack.NewWriter(&buf)
		for j := 0; j < 32; j++ {
			must(w.Add(objs[i*32+j]))
		}
		must(w.Close())
		fx.InboxPacks = append(fx.InboxPacks, buf.Bytes())
		fx.InboxRoots = append(fx.InboxRoots, objs[i*32].Key)
	}
}

// buildGCTemplate builds a store that a collection cycle can actually reclaim
// from: one reference rooted at a real tree keeps a minority of the objects
// alive, and the rest fills whole sealed segments that the sweep can drop.
func buildGCTemplate(e *Env, fx *Fixtures) {
	p := e.Profile
	fx.GCTemplate = filepath.Join(e.Scratch, "gc-template")
	must(os.RemoveAll(fx.GCTemplate))
	objDir := filepath.Join(fx.GCTemplate, "objects")
	refDir := filepath.Join(fx.GCTemplate, "refs")
	must(os.MkdirAll(objDir, 0o755))

	st := mustV(packstore.Open(objDir,
		packstore.WithSegmentSize(p.SegmentBytes), packstore.WithSync(false)))
	root, _, err := ingest.Dir(st, fx.TreeV1, ingest.Opts{Jobs: p.ThreadsMulti})
	must(err)
	fx.GCLiveRoot = root
	// Garbage: objects no reference reaches, sized to fill several segments.
	garbage := storeObjects(p, p.StoreObjects, 40000)
	_, err = st.WriteParallel(objectSeq(garbage), packstore.WriteOpts{Writers: p.ThreadsMulti})
	must(err)
	must(st.Sync())
	must(st.Close())

	rs := mustV(refstore.Open(refDir, false))
	rk := root
	rec := reference.Reference{
		Name: "gc/live", Key: append([]byte(nil), rk[:]...),
		User: "bench@example.org", CreatedAt: 1_700_000_000_000_000_000,
	}
	must(rs.Put(rec.Name, mustV(rec.Encode())))
	must(rs.Close())
}

// buildIngestTemplate stores the V1 tree once so the incremental-change case
// can start from a populated store without paying for the first ingest.
func buildIngestTemplate(e *Env, fx *Fixtures) {
	fx.IngestTemplate = filepath.Join(e.Scratch, "ingest-template")
	must(os.RemoveAll(fx.IngestTemplate))
	must(os.MkdirAll(fx.IngestTemplate, 0o755))
	st := mustV(packstore.Open(fx.IngestTemplate,
		packstore.WithSegmentSize(e.Profile.SegmentBytes), packstore.WithSync(false)))
	_, _, err := ingest.Dir(st, fx.TreeV1, ingest.Opts{Jobs: e.Profile.ThreadsMulti})
	must(err)
	must(st.Close())
}

// buildTarFixture ingests the fixture tree into memory and exports it once,
// so the export case measures traversal and PAX formatting rather than store
// I/O, and the extract case has a real archive to materialize.
func buildTarFixture(e *Env, fx *Fixtures) {
	fx.TreeMem = newMemStore()
	seq, rootPtr, err := ingest.Objects(fx.TreeV1, ingest.Opts{Jobs: e.Profile.ThreadsMulti})
	must(err)
	for o, err := range seq {
		must(err)
		must(fx.TreeMem.put(o))
	}
	fx.TreeRoot = *rootPtr
	var buf bytes.Buffer
	must(tarWrite(&buf, fx.TreeRoot, fx.TreeMem.get))
	fx.TarBytes = buf.Bytes()
}
