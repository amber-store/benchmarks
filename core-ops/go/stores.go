package main

import (
	"fmt"
	"os"
	"path/filepath"
	"sync/atomic"

	"github.com/amber-store/core/packstore"
	"github.com/amber-store/core/refstore"
)

var workCounter atomic.Uint64

// workDir returns a fresh, empty scratch directory. It lives under the
// driver's scratch root (ext4, not the session tmpfs) and is removed when the
// case releases it, so a long run does not accumulate stores.
func workDir(e *Env, name string) string {
	n := workCounter.Add(1)
	dir := filepath.Join(e.Scratch, "work", fmt.Sprintf("%s-%06d", name, n))
	must(os.RemoveAll(dir))
	must(os.MkdirAll(dir, 0o755))
	return dir
}

// storeHandle is an open packstore plus the directory it owns.
type storeHandle struct {
	dir string
	st  *packstore.Store
	own bool
}

func (h *storeHandle) close() {
	if h.st != nil {
		_ = h.st.Close()
		h.st = nil
	}
	if h.own {
		_ = os.RemoveAll(h.dir)
	}
}

func openStore(e *Env, dir string, sync bool) *storeHandle {
	st, err := packstore.Open(dir,
		packstore.WithSegmentSize(e.Profile.SegmentBytes), packstore.WithSync(sync))
	must(err)
	return &storeHandle{dir: dir, st: st, own: true}
}

// freshStore opens an empty store in a new scratch directory.
func freshStore(e *Env, name string) *storeHandle {
	return openStore(e, workDir(e, name), false)
}

func freshStoreSync(e *Env, name string) *storeHandle {
	return openStore(e, workDir(e, name), true)
}

// copiedStore duplicates a prebuilt template and opens the copy, so a
// destructive operation never consumes the template. The copy happens in
// Setup, outside every measured interval.
func copiedStore(e *Env, template, name string) *storeHandle {
	dir := workDir(e, name)
	must(copyTree(template, dir))
	return openStore(e, dir, false)
}

// copiedDir duplicates a template directory without opening anything.
func copiedDir(e *Env, template, name string) string {
	dir := workDir(e, name)
	must(copyTree(template, dir))
	return dir
}

// refHandle is an open refstore plus the directory it owns.
type refHandle struct {
	dir string
	st  *refstore.Store
}

func (h *refHandle) close() {
	if h.st != nil {
		_ = h.st.Close()
		h.st = nil
	}
	_ = os.RemoveAll(h.dir)
}

func freshRefs(e *Env, name string, sync bool) *refHandle {
	dir := workDir(e, name)
	st, err := refstore.Open(dir, sync)
	must(err)
	return &refHandle{dir: dir, st: st}
}

func copiedRefs(e *Env, template, name string, sync bool) *refHandle {
	dir := workDir(e, name)
	must(copyTree(template, dir))
	st, err := refstore.Open(dir, sync)
	must(err)
	return &refHandle{dir: dir, st: st}
}
