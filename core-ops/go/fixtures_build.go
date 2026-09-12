package main

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
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
//
// A set is one point in the (size, content) grid: its Dims are what the
// report sweeps, and its Name is only a label for that point.
type payloadSet struct {
	Name      string
	Items     [][]byte
	Bytes     int64
	ItemBytes int64
	Content   string
}

// dims renders the set as workload dimensions. Every point of the grid
// belongs to the object-size sweep, in the series named by its content kind;
// the empty object is its own series, because a zero-length object has no
// content and so belongs to no content curve.
func (ps payloadSet) dims() Dims {
	return Dims{
		ItemBytes: ps.ItemBytes, Items: int64(len(ps.Items)), Content: ps.Content,
		Sweep: "item_bytes", Series: ps.Content,
	}
}

// limit returns the leading part of the set whose total is at most max bytes,
// for operations that write every item to disk and would otherwise dominate
// the run. At least one item is always kept.
func (ps payloadSet) limit(max int64) payloadSet {
	if ps.Bytes <= max || ps.ItemBytes == 0 {
		return ps
	}
	n := int(max / ps.ItemBytes)
	minimum := 1
	if ps.Content == "duplicate" {
		minimum = 2
	}
	if n < minimum {
		n = minimum
	}
	if n > len(ps.Items) {
		n = len(ps.Items)
	}
	out := ps
	out.Items = ps.Items[:n]
	out.Bytes = int64(n) * ps.ItemBytes
	return out
}

// payloadSizes are the object-size classes every size-sensitive operation is
// swept over: an empty object, a tiny one, a kilobyte, a megabyte and a
// large multi-megabyte one. The classes are absolute sizes, not fractions of
// the profile, so the same workload means the same thing in both profiles
// and a scaling plot from a quick run is comparable with a standard one.
var payloadSizes = []struct {
	Name  string
	Bytes int
}{
	{"empty", 0},
	{"tiny-64B", 64},
	{"4KiB", 4 << 10},
	{"1MiB", 1 << 20},
	{"large-8MiB", 8 << 20},
}

// payloadContents are the content kinds crossed with those sizes:
//
//	random     incompressible, every item distinct — the hash and the store
//	           both see new bytes and no encoder can win
//	text       compressible, every item distinct — the record encoder's zstd
//	           path engages and the stored size is smaller than the logical
//	duplicate  every item byte-identical — a content-addressed store sees one
//	           object and the rest are dedup hits
//
// The empty size is not crossed with these: a zero-length object has no
// content to be random, compressible or duplicated, so the grid has one
// `empty` point rather than three identical ones.
var payloadContents = []string{"random", "text", "duplicate"}

func newPayloadSet(name, content string, count, size int, seed uint64) payloadSet {
	ps := payloadSet{
		Name: name, Content: content, ItemBytes: int64(size),
		Items: make([][]byte, count),
	}
	for i := range ps.Items {
		s := seed + uint64(i)*0x100000001B3
		switch content {
		case "text":
			ps.Items[i] = compressibleBytes(s, size)
		case "duplicate":
			// Every item is the same bytes, so the set has one distinct
			// object in it however many items it holds.
			if i == 0 {
				ps.Items[i] = randomBytes(seed, size)
			} else {
				ps.Items[i] = ps.Items[0]
			}
		default:
			ps.Items[i] = randomBytes(s, size)
		}
		ps.Bytes += int64(size)
	}
	return ps
}

// buildPayloadMatrix builds the whole (size, content) grid once. Every set
// holds about Profile.PayloadTotal bytes, so the grid costs the same at
// every size and a per-byte rate is comparable across the row.
func buildPayloadMatrix(p Profile) []payloadSet {
	var out []payloadSet
	for i, sz := range payloadSizes {
		if sz.Bytes == 0 {
			out = append(out, newPayloadSet("empty", "empty", p.BatchOps*4, 0, p.Seed+10))
			continue
		}
		items := int(p.PayloadTotal / int64(sz.Bytes))
		if items < 1 {
			items = 1
		}
		if items > 16384 {
			items = 16384
		}
		for j, content := range payloadContents {
			count := items
			if content == "duplicate" && count < 2 {
				count = 2
			}
			seed := p.Seed + 10 + uint64(i)*97 + uint64(j)*7919
			out = append(out, newPayloadSet(sz.Name+"-"+content, content, count, sz.Bytes, seed))
		}
	}
	return out
}

// payloadNamed returns one point of the grid by name.
func payloadNamed(sets []payloadSet, name string) payloadSet {
	for _, ps := range sets {
		if ps.Name == name {
			return ps
		}
	}
	panic("no payload set named " + name)
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

	// Payloads is the whole (size, content) grid: every size class crossed
	// with every content kind. Size-sensitive operations are swept over it.
	Payloads []payloadSet
	// Named points of that grid, for the cases and checks that need one
	// particular shape rather than the sweep.
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

	// fstree codec inputs and their encodings, swept over entry counts.
	// EntrySets crosses the entry-count sweep with two structured
	// variants: one whose entries carry extended attributes, and one that
	// is the 128-entry set with a single entry rewritten, which is the
	// partial-change case a real incremental update produces.
	EntrySets        []entrySet
	PairSets         []pairSet
	ChildSets        []childSet
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
	Mem *memStore
	// Dirs is the directory-width sweep: one built directory per entry
	// count in Profile.TreeWidths, all in Mem.
	Dirs []memDir
	// Chains is the path-depth sweep: one chain of nested directories per
	// depth in Profile.TreeDepths.
	Chains []memChain
	// FileIndexes is the file-index fan-out sweep: one multi-level file
	// node per child count in Profile.FanOuts.
	FileIndexes []memFileIndex
	WideRoot    key.Key
	WideNames   [][]byte // present names, ascending
	MissName    []byte   // a name that is not in the wide directory
	DeepRoot    key.Key
	DeepPath    string
	DeepDepth   int
	ShallowRoot key.Key
	FileRoot    key.Key
	FileBytes   int64
	// FileChunks is how many content chunks the corpus file really split
	// into, counted outside every measured interval so a per-chunk rate has
	// an exact denominator.
	FileChunks      int64
	IncompleteRoot  key.Key // a wide tree with one Blob leaf removed
	IncompleteStore *memStore

	// On-disk trees.
	TreeV1 string
	TreeV2 string
	// IngestTemplate is a packstore that already holds the V1 tree, so the
	// incremental-change case measures a re-ingest against real dedup hits.
	IngestTemplate string
	// TreeBytes and TreeFiles are what writeFixtureTree laid down, ignored
	// files included. They size the fixture; they are *not* a rate
	// denominator, because ingest does not include all of it.
	TreeBytes int64
	TreeFiles int64
	// Exact per-tree counts, measured outside every timed interval with the
	// core's own scan, so each rate has the denominator that belongs to it:
	// a filtered walk, an unfiltered walk and the changed successor tree
	// cover different files and different bytes.
	V1Included   treeCounts // .amberignore applied — what ingest really covers
	V1Unfiltered treeCounts // ignore rules disabled — strictly more
	V2Included   treeCounts // the successor tree, .amberignore applied
	// V1IncludedManifest is the listing the restored tree must reproduce:
	// the source tree restricted to the paths the fixture's own ignore
	// rules keep, computed by the harness rather than by either core.
	V1IncludedManifest []string
	IgnoreDir          string

	// Pack fixtures. PackObjects is the object population both cores
	// encode; WirePack and WireRecords are *this* core's own encoding of
	// it, which is what the writer cases are measured producing and whose
	// size the report states per core.
	PackObjects []fstree.Object
	WirePack    []byte
	WireRecords [][]byte
	// Wire holds every producer's pack, read from the shared wire
	// directory. Both drivers load the same files, so the reader and
	// decoder cases are measured on byte-identical input.
	Wire []WirePack

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
	// InboxLogicalBytes is the payload the staged packs carry, which is the
	// same in both cores; the packs' encoded sizes are not, and are
	// reported per core instead of used as a shared denominator.
	InboxLogicalBytes int64

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

// treeCounts is the exact extent of one on-disk tree as an ingest walk sees
// it: how many files it covers and how many logical bytes those files hold.
// Both numbers are taken outside every measured interval.
type treeCounts struct {
	Files int64 `json:"files"`
	Bytes int64 `json:"bytes"`
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

	fx.Payloads = buildPayloadMatrix(p)
	fx.Tiny = payloadNamed(fx.Payloads, "tiny-64B-random")
	fx.Small = payloadNamed(fx.Payloads, "4KiB-random")
	fx.Text = payloadNamed(fx.Payloads, "4KiB-text")
	fx.Large = payloadNamed(fx.Payloads, "1MiB-text")
	fx.Rand = payloadNamed(fx.Payloads, "1MiB-random")

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

// entrySet, pairSet and childSet are the points of the node-codec sweeps:
// one encoded node per entry count, with the content kind that produced it.
type entrySet struct {
	Name    string
	Content string
	// Series is the scaling family this point belongs to. The plain sets
	// form the entry-count curve; the with-xattrs and partial-change
	// variants are single points at a fixed count and so are their own
	// series, which keeps them out of a curve they would distort.
	Series  string
	Entries []fstree.Entry
	Enc     []byte
}

type pairSet struct {
	Name  string
	Pairs []fstree.DirPair
	Enc   []byte
}

type childSet struct {
	Name string
	Keys []key.Key
	Enc  []byte
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

	// --- the node-codec sweeps -------------------------------------------
	// Entry count is the dimension; the plain sets vary only in how many
	// entries they hold. Two further points are structured variants at a
	// fixed count: one whose entries carry extended attributes, and one
	// that is the 128-entry set with a single entry's content key
	// rewritten -- the shape an incremental update actually produces.
	addEntries := func(name, content, series string, es []fstree.Entry) {
		fx.EntrySets = append(fx.EntrySets, entrySet{
			Name: name, Content: content, Series: series, Entries: es,
			Enc: mustV(fstree.EncodeDirLeaf(es)).Bytes,
		})
	}
	for _, n := range []int{8, 128, 1024} {
		es := make([]fstree.Entry, 0, n)
		for i := 0; i < n; i++ {
			es = append(es, entryFor(i, p.Seed+40, nil))
		}
		addEntries(fmt.Sprintf("entries-%d", n), "structured", "plain", es)
	}
	addEntries("entries-128-with-xattrs", "structured", "with-xattrs", fx.EntriesLarge)
	changed := make([]fstree.Entry, len(fx.EntriesLarge))
	copy(changed, fx.EntriesLarge)
	one := changed[len(changed)/2]
	var h [32]byte
	copy(h[:], randomBytes(p.Seed+41, 32))
	ck := mustV(key.NewFromHash(key.Blob, 4242, h))
	one.ContentKey = keyBytes(ck)
	changed[len(changed)/2] = one
	addEntries("entries-128-partial-change", "partial-change", "partial-change", changed)

	for _, n := range []int{8, 128, 1024} {
		ps := mkPairs(n)
		fx.PairSets = append(fx.PairSets, pairSet{
			Name: fmt.Sprintf("pairs-%d", n), Pairs: ps,
			Enc: mustV(fstree.EncodeDirNode(ps)).Bytes,
		})
	}
	for _, n := range p.FanOuts {
		ks := mkChildren(n)
		fx.ChildSets = append(fx.ChildSets, childSet{
			Name: fmt.Sprintf("children-%d", n), Keys: ks,
			Enc: mustV(fstree.EncodeFileNode(ks)).Bytes,
		})
	}

	// The item chunker decides boundaries on the canonical encoding of one
	// item; feed it the same encodings the builders would.
	for i := 0; i < p.BatchOps; i++ {
		k := fx.Keys[i%len(fx.Keys)]
		kk := k
		fx.ItemEncodings = append(fx.ItemEncodings, append([]byte(nil), kk[:]...))
	}
}

// memDir is one point of the directory-width sweep: a real prolly tree of
// Entries entries, with every name it holds and one that it does not.
type memDir struct {
	Entries  int
	Root     key.Key
	Names    [][]byte // present names, ascending
	MissName []byte
	// Objects is how many stored objects the directory's own encoding came
	// to — the leaves, the index levels and the blobs the entries point at
	// — counted outside every measured interval.
	Objects int64
}

// memChain is one point of the path-depth sweep: Depth nested directories
// with a resolvable path through all of them.
type memChain struct {
	Depth int
	Root  key.Key
	Path  string
}

// memFileIndex is one point of the file-index fan-out sweep.
type memFileIndex struct {
	Children int
	Root     key.Key
	Keys     []key.Key
}

// buildMemTrees builds the in-memory directory and file trees the read paths
// walk. Three dimensions are swept independently, each with the others held
// fixed, so a scaling plot reads one variable at a time:
//
//   - directory width, over Profile.TreeWidths, for the builders, the
//     lookups, the listing and the traversals;
//   - path depth, over Profile.TreeDepths, for path resolution;
//   - file-index fan-out, over Profile.FanOuts, for the file builders.
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

	build := func(n int, seed uint64) (key.Key, [][]byte, int64) {
		before := fx.Mem.len()
		db := fstree.NewDirBuilder(ic)
		names := make([][]byte, 0, n)
		for i := 0; i < n; i++ {
			en := storedEntry(i, seed)
			names = append(names, en.Name)
			must(db.AddEntry(fx.Mem.put, en))
		}
		root, err := db.Finish(fx.Mem.put)
		must(err)
		return root, names, int64(fx.Mem.len() - before)
	}

	// --- directory width ------------------------------------------------
	for i, n := range p.TreeWidths {
		root, names, objs := build(n, p.Seed+70+uint64(i)*131)
		fx.Dirs = append(fx.Dirs, memDir{
			Entries: n, Root: root, Names: names,
			MissName: []byte("entry-zzzzzzzz"), Objects: objs,
		})
	}
	widest := fx.Dirs[len(fx.Dirs)-1]
	fx.WideRoot, fx.WideNames = widest.Root, widest.Names
	fx.MissName = widest.MissName
	fx.ShallowRoot = fx.Dirs[0].Root

	// --- path depth -------------------------------------------------------
	// Each chain is a stack of directories, each holding the next one plus a
	// little noise, so resolution really descends every level.
	chainOf := func(depth int, seed uint64) memChain {
		child := fx.Dirs[0].Root
		var comps []string
		for d := depth - 1; d >= 0; d-- {
			db := fstree.NewDirBuilder(ic)
			name := fmt.Sprintf("level%03d", d)
			ck := child
			// Entries must be added in bytewise name order: "entry-..."
			// sorts before "level...".
			must(db.AddEntry(fx.Mem.put, storedEntry(d, seed)))
			must(db.AddEntry(fx.Mem.put, fstree.Entry{
				Name: []byte(name), Mode: modeDir, UID: 1000, GID: 1000,
				Mtime: 1_700_000_000_000_000_000, ContentKey: append([]byte(nil), ck[:]...),
			}))
			root, err := db.Finish(fx.Mem.put)
			must(err)
			child = root
			comps = append([]string{name}, comps...)
		}
		return memChain{Depth: depth, Root: child, Path: filepath.Join(comps...)}
	}
	for i, d := range p.TreeDepths {
		fx.Chains = append(fx.Chains, chainOf(d, p.Seed+72+uint64(i)*211))
	}
	deepest := fx.Chains[len(fx.Chains)-1]
	fx.DeepRoot, fx.DeepPath, fx.DeepDepth = deepest.Root, deepest.Path, deepest.Depth

	// --- file-index fan-out ----------------------------------------------
	// Every index is built over the same synthetic child keys, so the only
	// thing that changes along the sweep is how many of them there are.
	for i, n := range p.FanOuts {
		keys := make([]key.Key, 0, n)
		for j := 0; j < n; j++ {
			var arr [32]byte
			copy(arr[:], randomBytes(p.Seed+60+uint64(i)*7+uint64(j), 32))
			keys = append(keys, mustV(key.NewFromHash(key.Blob, 65536, arr)))
		}
		fb := fstree.NewFileIndexBuilder(ic)
		for _, k := range keys {
			must(fb.AddChild(fx.Mem.put, k, nil))
		}
		root := mustV(fb.Finish(fx.Mem.put))
		fx.FileIndexes = append(fx.FileIndexes, memFileIndex{Children: n, Root: root, Keys: keys})
	}

	// One file built the way ingest builds files: content-defined chunks
	// under FileNode index levels. The chunk count is recorded, so the
	// per-chunk denominator is exact rather than assumed.
	fb := fstree.NewFileIndexBuilder(ic)
	nblobs := int64(0)
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
	fx.FileChunks = nblobs

	// A copy of the narrowest tree with one leaf missing, for the
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

	// Exact rate denominators, taken here — before anything is timed — with
	// the core's own walk. A filtered scan, an unfiltered scan and the
	// successor tree cover different files and different bytes; using one
	// number for all three would misreport every rate but one.
	fx.V1Included = scanCounts(fx.TreeV1, false)
	fx.V1Unfiltered = scanCounts(fx.TreeV1, true)
	fx.V2Included = scanCounts(fx.TreeV2, false)

	// The listing a restored tree has to reproduce, derived from the
	// fixture's own ignore rules rather than from either core.
	fx.V1IncludedManifest = mustV(manifestLines(fx.TreeV1, fixtureIncluded))
}

// scanCounts is ingest.Scan used as a measuring tape rather than as a
// measured operation: it runs once, at fixture-build time, so the timed
// cases can divide by an exact count instead of an estimate.
func scanCounts(root string, ignoreRules bool) treeCounts {
	files, bytesTotal, err := ingest.Scan(root, ignoreRules, 1)
	must(err)
	return treeCounts{Files: int64(files), Bytes: int64(bytesTotal)}
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

// WirePack is one producing core's encoding of the shared object
// population, as read from the wire directory. Its producer, content hash
// and encoded size are recorded in the report: two cores' packs carry the
// same objects but not the same compressed bytes, so a decode measurement
// has to say which bytes it decoded.
type WirePack struct {
	Producer string   `json:"producer"`
	SHA256   string   `json:"sha256"`
	Bytes    int64    `json:"bytes"`
	Objects  int      `json:"objects"`
	Data     []byte   `json:"-"`
	Records  [][]byte `json:"-"`
}

// packObjectPopulation is the object population both cores encode into a
// wire pack: a mix of tiny and large, compressible and random payloads, from
// the profile's seed alone. It is a pure function of the profile, so the
// --emit-wire pass does not have to build any other fixture.
func packObjectPopulation(p Profile) []fstree.Object {
	n := maxInt(p.BatchOps/4, 64)
	out := make([]fstree.Object, 0, n)
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
		out = append(out, o)
	}
	return out
}

// encodeWirePack writes the population as one wire pack with this core's
// encoder.
func encodeWirePack(objs []fstree.Object) []byte {
	var buf bytes.Buffer
	w := amberpack.NewWriter(&buf)
	for _, o := range objs {
		must(w.Add(o))
	}
	must(w.Close())
	return buf.Bytes()
}

// emitWirePack is the production pass: it writes this core's encoding of the
// shared object population into the wire directory, where the measurement
// pass of *both* drivers will read it. Nothing is measured here.
func emitWirePack(p Profile, dir string) error {
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return err
	}
	objs := packObjectPopulation(p)
	pack := encodeWirePack(objs)
	path := filepath.Join(dir, wireProducer+".pack")
	if err := os.WriteFile(path, pack, 0o644); err != nil {
		return err
	}
	sum := sha256.Sum256(pack)
	fmt.Printf("%s: wrote %s (%d objects, %d bytes, sha256 %s)\n",
		wireProducer, path, len(objs), len(pack), hex.EncodeToString(sum[:]))
	return nil
}

// wireProducers are the packs every driver reads, in a fixed order, so the
// two documents carry the same workloads in the same sequence.
var wireProducers = []string{"go", "rust"}

const wireProducer = "go"

// loadWirePacks reads every producer's pack out of the shared wire
// directory and splits it into its raw records. Both drivers run this over
// the same files, so `amberpack.reader_all/producer-rust` in the Go document
// and in the Rust document are the same bytes, byte for byte.
func loadWirePacks(e *Env, fx *Fixtures) {
	for _, producer := range wireProducers {
		path := filepath.Join(e.WireDir, producer+".pack")
		data, err := os.ReadFile(path)
		if err != nil {
			panic(fmt.Sprintf("wire pack %s: %v (run --emit-wire for both cores first)", path, err))
		}
		sum := sha256.Sum256(data)
		wp := WirePack{
			Producer: producer,
			SHA256:   hex.EncodeToString(sum[:]),
			Bytes:    int64(len(data)),
			Data:     data,
		}
		r := amberpack.NewReader(bytes.NewReader(data))
		for rec, err := range r.Records() {
			must(err)
			wp.Records = append(wp.Records, append([]byte(nil), rec.Bytes...))
		}
		wp.Objects = len(wp.Records)
		fx.Wire = append(fx.Wire, wp)
	}
}

// wireOf returns the loaded pack of one producer.
func (fx *Fixtures) wireOf(producer string) WirePack {
	for _, w := range fx.Wire {
		if w.Producer == producer {
			return w
		}
	}
	panic("no wire pack for producer " + producer)
}

func buildPackFixtures(e *Env, fx *Fixtures) {
	p := e.Profile
	fx.PackObjects = packObjectPopulation(p)
	fx.WirePack = encodeWirePack(fx.PackObjects)
	for _, o := range fx.PackObjects {
		rec, err := amberpack.EncodeRecord(o.Key, o.Bytes)
		must(err)
		fx.WireRecords = append(fx.WireRecords, rec)
	}
	loadWirePacks(e, fx)
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
	fx.InboxLogicalBytes = packBytes(objs[:p.InboxPacks*32])
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
