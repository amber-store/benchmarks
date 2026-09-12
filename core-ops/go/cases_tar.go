package main

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"

	"github.com/amber-store/core/tarextract"
)

func tarCases(e *Env) []Case {
	fx := e.fx
	return []Case{
		{
			Group: "tar", Op: "tarexport.write", Workload: "fixture-tree", Threads: 1,
			Ops: 1, Bytes: int64(len(fx.TarBytes)),
			Setup: func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var c countingWriter
				if err := tarWrite(&c, en.fx.TreeRoot, en.fx.TreeMem.get); err != nil {
					panic(err)
				}
				return uint64(c.n)
			},
		},
		{
			Group: "tar", Op: "tarextract.extract", Workload: "fixture-tree", Threads: 1,
			Ops: 1, Bytes: int64(len(fx.TarBytes)), PerRep: true,
			Setup: func(en *Env) any { return workDir(en, "tar-extract") },
			Run: func(en *Env, s any) uint64 {
				dir := s.(string)
				if err := tarextract.Extract(bytes.NewReader(en.fx.TarBytes), dir); err != nil {
					panic(err)
				}
				return uint64(len(en.fx.TarBytes))
			},
			Free: func(_ *Env, s any) { _ = os.RemoveAll(s.(string)) },
		},
	}
}

func tarChecks(e *Env) {
	fx := e.fx

	// The PAX export is specified to be byte-identical across the two
	// cores, so its digest is a comparable statement.
	var buf bytes.Buffer
	err := tarWrite(&buf, fx.TreeRoot, fx.TreeMem.get)
	e.want("tar", "tarexport.write", "tar/export-bytes",
		err == nil && bytes.Equal(buf.Bytes(), fx.TarBytes),
		fmt.Sprintf("export is not deterministic: %v", err), digest(buf.Bytes()))

	// Exporting a non-directory root must be refused.
	blobKey := fx.PackObjects[0].Key
	var sink countingWriter
	err = tarWrite(&sink, blobKey, fx.TreeMem.get)
	e.want("tar", "tarexport.write", "tar/export-rejects-non-directory", err != nil,
		"a Blob root was accepted as a directory", "rejected")

	// Extraction materializes the same tree in both cores: the manifest
	// digest (type, permissions, size, content hash, symlink target) is the
	// cross-core statement, and a spot check against the source tree is the
	// within-core one.
	dir := workDir(e, "check-extract")
	defer os.RemoveAll(dir)
	if err := tarextract.Extract(bytes.NewReader(fx.TarBytes), dir); err != nil {
		e.fail("tar", "tarextract.extract", "tar/extract", err.Error())
		return
	}
	m, merr := manifest(dir)
	e.want("tar", "tarextract.extract", "tar/extract-manifest", merr == nil,
		fmt.Sprintf("manifest of the extracted tree: %v", merr), m)

	same := true
	var detail string
	for _, rel := range []string{"data/d000/f000.bin", "data/d000/keep.tmp", "big.bin", "wide/w000000.bin"} {
		want, err1 := os.ReadFile(filepath.Join(fx.TreeV1, rel))
		got, err2 := os.ReadFile(filepath.Join(dir, rel))
		if err1 != nil || err2 != nil || !bytes.Equal(want, got) {
			same = false
			detail = fmt.Sprintf("%s: %v / %v", rel, err1, err2)
			break
		}
	}
	e.want("tar", "tarextract.extract", "tar/extract-content-matches-source", same,
		"an extracted file differs from the source tree: "+detail, "ok")

	// Symlinks survive the round trip.
	link, lerr := os.Readlink(filepath.Join(dir, "links", "ln000"))
	e.want("tar", "tarextract.extract", "tar/extract-symlink",
		lerr == nil && link == "../data/d000/f000.bin",
		fmt.Sprintf("symlink target %q (%v)", link, lerr), link)

	// The ignored directory never entered the archive, so it cannot come
	// back out of it.
	_, serr := os.Stat(filepath.Join(dir, "build"))
	e.want("tar", "tarextract.extract", "tar/extract-honours-ingest-filter", os.IsNotExist(serr),
		fmt.Sprintf("the ignored build/ directory was extracted: %v", serr), "absent")
}
