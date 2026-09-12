package main

// groupFn contributes the cases of one module group.
type groupFn struct {
	Name  string
	Cases func(*Env) []Case
	Check func(*Env)
}

// groups is the whole registry, in module order. Every entry maps to one
// public Go package (or, for `cbor`, to the package the Rust module of that
// name corresponds to), which is how the coverage matrix stays checkable
// against the two cores' public surfaces.
func groups() []groupFn {
	return []groupFn{
		{"key", keyCases, keyChecks},
		{"cbor", cborCases, cborChecks},
		{"chunkers", chunkerCases, chunkerChecks},
		{"fstree", fstreeCases, fstreeChecks},
		{"amberignore", ignoreCases, ignoreChecks},
		{"ingest", ingestCases, ingestChecks},
		{"amberpack", packCases, packChecks},
		{"packstore", packstoreCases, packstoreChecks},
		{"reference", referenceCases, referenceChecks},
		{"refstore", refstoreCases, refstoreChecks},
		{"inbox", inboxCases, inboxChecks},
		{"tar", tarCases, tarChecks},
		{"gc", gcCases, gcChecks},
	}
}

func registry(e *Env, only map[string]bool) []Case {
	var out []Case
	for _, g := range groups() {
		if only != nil && !only[g.Name] {
			continue
		}
		out = append(out, g.Cases(e)...)
	}
	return out
}

func runChecks(e *Env, only map[string]bool) {
	fixtureChecks(e)
	for _, g := range groups() {
		if only != nil && !only[g.Name] {
			continue
		}
		g.Check(e)
	}
}

// fixtureChecks records the canonical digests of the fixtures themselves.
// The report compares each of these against the other core's value and
// refuses to pair any timing unless they match: identical inputs are the
// precondition of a meaningful comparison, not an assumption.
func fixtureChecks(e *Env) {
	fx := e.fx
	e.pass("fixture", "fixture.corpus", "fixture/corpus-random", digest(fx.CorpusRandom))
	e.pass("fixture", "fixture.corpus", "fixture/corpus-text", digest(fx.CorpusText))
	e.pass("fixture", "fixture.payloads", "fixture/payloads-tiny", digestList(fx.Tiny.Items))
	e.pass("fixture", "fixture.payloads", "fixture/payloads-small", digestList(fx.Small.Items))
	e.pass("fixture", "fixture.payloads", "fixture/payloads-text", digestList(fx.Text.Items))
	e.pass("fixture", "fixture.payloads", "fixture/payloads-large", digestList(fx.Large.Items))
	e.pass("fixture", "fixture.payloads", "fixture/payloads-rand", digestList(fx.Rand.Items))
	e.pass("fixture", "fixture.keys", "fixture/keys", digestKeys(fx.Keys))
	e.pass("fixture", "fixture.entries", "fixture/entries-large", digest(fx.EncDirLeafLarge))
	e.pass("fixture", "fixture.tree", "fixture/tree-v1", mustV(manifest(fx.TreeV1)))
	e.pass("fixture", "fixture.tree", "fixture/tree-v2", mustV(manifest(fx.TreeV2)))
	e.pass("fixture", "fixture.tree", "fixture/ignore-dir", mustV(manifest(fx.IgnoreDir)))
	// The wire pack embeds zstd-compressed payloads, which the two cores
	// produce with different encoders; its digest is a within-core anchor,
	// not a cross-core one. The comparable statement about the same pack is
	// the (key, logical length) list it carries.
	e.passLocal("fixture", "fixture.pack", "fixture/wire-pack-bytes", digest(fx.WirePack))
	e.pass("fixture", "fixture.pack", "fixture/wire-pack-contents", packContentDigest(fx.PackObjects))
	e.pass("fixture", "fixture.store", "fixture/store-keys", digestKeys(fx.StoreKeys))
	e.pass("fixture", "fixture.tar", "fixture/tar-archive", digest(fx.TarBytes))
	e.pass("fixture", "fixture.mem", "fixture/wide-root", fx.WideRoot.String())
	e.pass("fixture", "fixture.mem", "fixture/deep-root", fx.DeepRoot.String())
	e.pass("fixture", "fixture.mem", "fixture/file-root", fx.FileRoot.String())
	e.pass("fixture", "fixture.gc", "fixture/gc-live-root", fx.GCLiveRoot.String())
}
