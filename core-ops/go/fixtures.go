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
		return "", err
	}
	sort.Strings(lines)
	return digest([]byte(strings.Join(lines, "\n"))), nil
}
