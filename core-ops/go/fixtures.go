package main

import (
	"encoding/binary"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/amber-store/core/key"
	"golang.org/x/sys/unix"
)

// ---------------------------------------------------------------------------
// Deterministic pseudo-randomness
// ---------------------------------------------------------------------------

// rng is SplitMix64. The Rust driver implements the same generator with the
// same constants, so both cores see byte-identical fixtures from the same
// seed — which the fixture digest checks then prove rather than assume.
type rng struct{ s uint64 }

func newRNG(seed uint64) *rng { return &rng{s: seed} }

func (r *rng) next() uint64 {
	r.s += 0x9E3779B97F4A7C15
	z := r.s
	z = (z ^ (z >> 30)) * 0xBF58476D1CE4E5B9
	z = (z ^ (z >> 27)) * 0x94D049BB133111EB
	return z ^ (z >> 31)
}

// n returns a value in [0, bound).
func (r *rng) n(bound int) int {
	if bound <= 0 {
		return 0
	}
	return int(r.next() % uint64(bound))
}

// randomBytes fills n bytes with incompressible noise.
func randomBytes(seed uint64, n int) []byte {
	r := newRNG(seed)
	b := make([]byte, n)
	for i := 0; i+8 <= n; i += 8 {
		binary.LittleEndian.PutUint64(b[i:], r.next())
	}
	if rem := n % 8; rem != 0 {
		var tail [8]byte
		binary.LittleEndian.PutUint64(tail[:], r.next())
		copy(b[n-rem:], tail[:rem])
	}
	return b
}

// lexicon is the deterministic word list the compressible corpus is drawn
// from. Short, repetitive tokens make the stream compress the way real
// source text does, which is what separates the zstd-sensitive record paths
// from the random ones.
var lexicon = buildLexicon()

func buildLexicon() []string {
	parts := []string{"amber", "store", "chunk", "pack", "segment", "key", "tree",
		"leaf", "node", "index", "blob", "entry", "record", "footer", "filter"}
	out := make([]string, 0, 256)
	for i := 0; i < 256; i++ {
		out = append(out, fmt.Sprintf("%s-%s-%02x", parts[i%len(parts)], parts[(i/len(parts)+3)%len(parts)], i))
	}
	return out
}

// compressibleBytes builds n bytes of repetitive, zstd-friendly text.
func compressibleBytes(seed uint64, n int) []byte {
	r := newRNG(seed)
	var b strings.Builder
	b.Grow(n + 32)
	for b.Len() < n {
		b.WriteString(lexicon[r.n(len(lexicon))])
		if r.n(12) == 0 {
			b.WriteByte('\n')
		} else {
			b.WriteByte(' ')
		}
	}
	return []byte(b.String())[:n]
}

// ---------------------------------------------------------------------------
// Canonical digests
// ---------------------------------------------------------------------------

// digest fingerprints bytes with the core's own key construction, so the two
// drivers compute it with the same primitive and the report can compare the
// hex strings directly.
func digest(b []byte) string {
	k, err := key.New(key.Blob, uint64(len(b)), b)
	if err != nil {
		return "error:" + err.Error()
	}
	return k.String()
}

// digestList fingerprints a sequence of byte strings unambiguously: each item
// is prefixed with its big-endian length, so no concatenation collides with a
// different split.
func digestList(items [][]byte) string {
	var buf []byte
	var l [8]byte
	for _, it := range items {
		binary.BigEndian.PutUint64(l[:], uint64(len(it)))
		buf = append(buf, l[:]...)
		buf = append(buf, it...)
	}
	return digest(buf)
}

func digestKeys(keys []key.Key) string {
	items := make([][]byte, 0, len(keys))
	for _, k := range keys {
		kk := k
		items = append(items, kk[:])
	}
	return digestList(items)
}

func digestStrings(ss []string) string {
	items := make([][]byte, 0, len(ss))
	for _, s := range ss {
		items = append(items, []byte(s))
	}
	return digestList(items)
}

// ---------------------------------------------------------------------------
// Filesystem helpers
// ---------------------------------------------------------------------------

func must(err error) {
	if err != nil {
		panic(err)
	}
}

func mustV[T any](v T, err error) T {
	must(err)
	return v
}

// copyTree duplicates a directory so a destructive operation measures on its
// own copy and the template survives for the next repetition.
func copyTree(src, dst string) error {
	return filepath.WalkDir(src, func(p string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, p)
		if err != nil {
			return err
		}
		target := filepath.Join(dst, rel)
		switch {
		case d.IsDir():
			return os.MkdirAll(target, 0o755)
		case d.Type()&os.ModeSymlink != 0:
			link, err := os.Readlink(p)
			if err != nil {
				return err
			}
			return os.Symlink(link, target)
		default:
			in, err := os.Open(p)
			if err != nil {
				return err
			}
			defer in.Close()
			info, err := in.Stat()
			if err != nil {
				return err
			}
			out, err := os.OpenFile(target, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, info.Mode().Perm())
			if err != nil {
				return err
			}
			if _, err := io.Copy(out, in); err != nil {
				out.Close()
				return err
			}
			return out.Close()
		}
	})
}

// fixedTime returns the deterministic modification time of one fixture path.
// Ingest encodes mtimes into every directory entry, so two trees with the
// same contents but different timestamps produce different root keys. Both
// drivers derive the timestamp from the relative path with the same FNV-1a
// hash, which makes the whole tree — content and metadata — reproducible.
func fixedTime(rel string) (sec int64, nsec int64) {
	h := uint64(14695981039346656037)
	for i := 0; i < len(rel); i++ {
		h ^= uint64(rel[i])
		h *= 1099511628211
	}
	return 1_700_000_000 + int64(h%100_000), int64(h % 1_000_000_000)
}

// stampTree gives every entry of the tree its deterministic timestamp.
// Directories are stamped after their contents, because creating a child
// updates its parent's mtime.
func stampTree(root string) error {
	var dirs []string
	err := filepath.WalkDir(root, func(p string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(root, p)
		if err != nil {
			return err
		}
		if d.IsDir() {
			dirs = append(dirs, p)
			return nil
		}
		return stampPath(p, rel)
	})
	if err != nil {
		return err
	}
	// Deepest first, so a parent is stamped after its children.
	sort.Slice(dirs, func(i, j int) bool { return len(dirs[i]) > len(dirs[j]) })
	for _, d := range dirs {
		rel, err := filepath.Rel(root, d)
		if err != nil {
			return err
		}
		if err := stampPath(d, rel); err != nil {
			return err
		}
	}
	return nil
}

func stampPath(path, rel string) error {
	sec, nsec := fixedTime(rel)
	ts := []unix.Timespec{{Sec: sec, Nsec: nsec}, {Sec: sec, Nsec: nsec}}
	return unix.UtimesNanoAt(unix.AT_FDCWD, path, ts, unix.AT_SYMLINK_NOFOLLOW)
}

// manifest renders a directory tree as a canonical, platform-stable listing:
// one line per path in sorted order carrying the type, the permission bits,
// the size, the modification time and the content digest. Both drivers
// compute it the same way, so comparing the two digests proves the fixtures
// really are identical -- including the timestamps ingest folds into every
// entry -- before any timing is compared.
func manifest(root string) (string, error) {
	lines, err := manifestLines(root, nil)
	if err != nil {
		return "", err
	}
	return digestStrings(lines), nil
}

// manifestLines is manifest's listing, before it is digested, optionally
// restricted to the paths keep accepts. A nil keep accepts everything.
func manifestLines(root string, keep func(rel string, isDir bool) bool) ([]string, error) {
	var lines []string
	err := filepath.WalkDir(root, func(p string, d os.DirEntry, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(root, p)
		if err != nil {
			return err
		}
		if rel == "." {
			return nil
		}
		if keep != nil && !keep(rel, d.IsDir()) {
			if d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		mt := fmt.Sprintf("%d.%09d", info.ModTime().Unix(), info.ModTime().Nanosecond())
		switch {
		case d.IsDir():
			lines = append(lines, fmt.Sprintf("d %04o %s %s", info.Mode().Perm(), mt, rel))
		case d.Type()&os.ModeSymlink != 0:
			link, err := os.Readlink(p)
			if err != nil {
				return err
			}
			lines = append(lines, fmt.Sprintf("l %s %s -> %s", rel, mt, link))
		default:
			b, err := os.ReadFile(p)
			if err != nil {
				return err
			}
			lines = append(lines, fmt.Sprintf("f %04o %d %s %s %s",
				info.Mode().Perm(), len(b), mt, digest(b), rel))
		}
		return nil
	})
	if err != nil {
		return nil, err
	}
	sort.Strings(lines)
	return lines, nil
}

// fixtureIncluded says whether one path of the fixture tree is expected to
// survive ingest, by applying the .amberignore rules the harness itself
// wrote into that tree ("*.tmp", "!keep.tmp", "/build/").
//
// It is deliberately an independent statement of the expectation: the
// restored-tree check compares the complete restored listing against this,
// not against anything either core computed. Two cores agreeing with each
// other proves only that they agree.
func fixtureIncluded(rel string, _ bool) bool {
	if rel == "build" || strings.HasPrefix(rel, "build"+string(filepath.Separator)) {
		return false
	}
	base := filepath.Base(rel)
	if strings.HasSuffix(base, ".tmp") && base != "keep.tmp" {
		return false
	}
	return true
}

// firstDifference reports the first line on which two sorted listings
// disagree, so a failed comparison names the path rather than two hashes.
func firstDifference(want, got []string) string {
	for i := 0; i < len(want) || i < len(got); i++ {
		var w, g string
		if i < len(want) {
			w = want[i]
		}
		if i < len(got) {
			g = got[i]
		}
		if w != g {
			return fmt.Sprintf("line %d of %d/%d: expected %q, restored %q", i, len(want), len(got), w, g)
		}
	}
	return ""
}

// ---------------------------------------------------------------------------
// Output folds
// ---------------------------------------------------------------------------

// Every measured case returns a fold of everything its calls produced, and
// the driver records that fold in each sample. The fold serves two purposes
// at once:
//
//   - nothing the case computed can be dead code, because the complete
//     output reaches a value the driver writes to its report; and
//   - the two cores' folds are directly comparable wherever they are
//     specified to produce the same bytes, so a timing is accompanied by
//     evidence that the operation really did the thing it is named after.
//
// The Rust driver implements the identical FNV-1a over the identical byte
// sequences, which is what makes the second property hold.
const (
	fnvOffset uint64 = 14695981039346656037
	fnvPrime  uint64 = 1099511628211
)

// newFold starts a fold.
func newFold() uint64 { return fnvOffset }

// foldBytes folds every byte of b, not a sample of them.
func foldBytes(acc uint64, b []byte) uint64 {
	for _, x := range b {
		acc ^= uint64(x)
		acc *= fnvPrime
	}
	return acc
}

// foldU64 folds a number as its eight little-endian bytes.
func foldU64(acc, v uint64) uint64 {
	for i := 0; i < 8; i++ {
		acc ^= v & 0xFF
		acc *= fnvPrime
		v >>= 8
	}
	return acc
}

func foldI64(acc uint64, v int64) uint64 { return foldU64(acc, uint64(v)) }

func foldStr(acc uint64, s string) uint64 { return foldBytes(acc, []byte(s)) }

func foldBool(acc uint64, b bool) uint64 {
	if b {
		return foldU64(acc, 1)
	}
	return foldU64(acc, 0)
}

// foldKey folds all 32 bytes of a key: the type and length header and the
// whole digest, so a case that constructs keys cannot be reduced to one that
// only assembles headers.
func foldKey(acc uint64, k key.Key) uint64 { return foldBytes(acc, k[:]) }

// jobsLabel and writersLabel name a worker count by kind rather than by
// number. The number is a profile setting and is recorded in the sample's
// dimensions; keeping it out of the workload label is what lets one coverage
// manifest describe every profile.
func jobsLabel(n int) string {
	if n == 1 {
		return "jobs-1"
	}
	return "jobs-N"
}

func writersLabel(n int) string {
	if n == 1 {
		return "writers-1"
	}
	return "writers-N"
}
