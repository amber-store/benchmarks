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
			Ops: int(fx.V1Included.Files), Bytes: int64(len(fx.TarBytes)), BytesKind: BytesEncoded,
			Dims:          Dims{Files: fx.V1Included.Files, Entries: fx.V1Included.Files, Content: "tree"},
			CrossChecksum: true,
			Setup:         func(*Env) any { return nil },
			Run: func(en *Env, _ any) uint64 {
				var c countingWriter
				if err := tarWrite(&c, en.fx.TreeRoot, en.fx.TreeMem.get); err != nil {
					panic(err)
				}
				return foldI64(newFold(), c.n)
			},
		},
		{
			Group: "tar", Op: "tarextract.extract", Workload: "fixture-tree", Threads: 1,
			Ops: int(fx.V1Included.Files), Bytes: int64(len(fx.TarBytes)), BytesKind: BytesEncoded,
			Dims:          Dims{Files: fx.V1Included.Files, Entries: fx.V1Included.Files, Content: "tree"},
			PerRep:        true,
			CrossChecksum: true,
			Setup:         func(en *Env) any { return workDir(en, "tar-extract") },
			Run: func(en *Env, s any) uint64 {
				dir := s.(string)
				if err := tarextract.Extract(bytes.NewReader(en.fx.TarBytes), dir); err != nil {
					panic(err)
				}
				return foldI64(newFold(), int64(len(en.fx.TarBytes)))
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

	// The whole extracted tree is compared with the listing the harness
	// derived from the source tree and the fixture's own ignore rules --
	// every path, its type, its permissions, its size, its mtime, its
	// content digest and its symlink target. A handful of spot-checked
	// files would not be a restore verification, and two cores agreeing
	// with each other would prove only that they agree.
	got, gerr := manifestLines(dir, nil)
	want := fx.V1IncludedManifest
	same := gerr == nil && len(want) > 0 && len(got) == len(want)
	if same {
		for i := range want {
			if want[i] != got[i] {
				same = false
				break
			}
		}
	}
	e.want("tar", "tarextract.extract", "tar/extract-is-the-included-source-tree", same,
		fmt.Sprintf("the extracted tree is not the included source tree: %v; %s",
			gerr, firstDifference(want, got)),
		digestStrings(got))

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
