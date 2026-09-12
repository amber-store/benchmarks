# Coverage matrix

Every exported symbol of both cores, and what the core-operation
benchmark does with it. The lists on the two sides are extracted from the
pinned sources, not written by hand:

```sh
./core-ops/coverage/refresh.sh       # regenerate the lists and this file
```

* Exported Go symbols: **198** (`coverage/go-exports.txt`)
* Exported Rust symbols: **237** (`coverage/rust-exports.txt`)
* Matrix rows: **142**, covering every one of them exactly once
* Paired operations measured in both cores: **98**
* Operations one core exports and the other does not: **2 Go-only, 5 Rust-only**
* Grouped accessor/configuration rows: **2**
* Data types: **13**; constants: **10**; error values: **11**

`core-ops/coverage/matrix.py check` proves the mapping is total: every
extracted symbol appears in exactly one row, no row names a symbol that
does not exist, and — given a report — every operation this file calls
measured really produced samples. CI runs it.

## How to read the status column

| Status | Meaning |
|---|---|
| `measured` | Timed in both cores and compared per workload. |
| `measured-go-only` | Exported by the Go core only. Timed on that side, reported separately, never compared. |
| `measured-rust-only` | Exported by the Rust core only. Timed on that side, reported separately, never compared. |
| `grouped` | An accessor or a configuration call, folded into a batched case because timing it alone would measure the loop. |
| `checked-only` | Exercised by a correctness check without a timing of its own. |
| `type` | A data type. Constructing and reading it is part of the operations that use it. |
| `constant` | A constant. |
| `error` | An error value, type or classifier. The failure-path checks assert the classification; there is nothing to time. |

## The matrix

### `key`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `key.new` | measured | `key.New` | `key::Key::new` | `tiny-64B-random`<br>`small-4KiB-random`<br>`small-4KiB-text`<br>`large-1MiB-text`<br>`large-1MiB-random` | `key/new-small` | BLAKE3 over the payload plus header assembly; five payload sizes and both compressibilities, since the hash dominates at size. |
| `key.new_from_hash` | measured | `key.NewFromHash` | `key::Key::new_from_hash` | `batch` | `key/new-from-hash` | Header assembly alone, with the digest precomputed. |
| `key.parse` | measured | `key.Parse` | `key::Key::parse` | `canonical`<br>`malformed` | `key/parse-roundtrip`<br>`key/parse-rejects` | Both the accepting and the rejecting path; the four malformed inputs cover each documented rejection reason. |
| `key.string` | measured | `key.Key.String` | `key::Key (Display)` | `hex` | `key/parse-roundtrip` | Lowercase hex rendering. Rust spells it as a `Display` impl, which the extractor does not list as a free symbol. |
| `key.type_string` | measured | `key.Type.String`<br>`key.Type.IsValid` | `key::Type::is_valid`<br>`key::Type::from_u8` | `names` | `key/type-names` | Type-name rendering and validity. Rust's `from_u8` is the checked conversion Go performs inside `Type.IsValid`; Rust's name rendering is a `Display` impl. |
| `key.validate` | measured | `key.Key.Validate` | `key::Key::validate` | `canonical` | `key/parse-rejects` |  |
| `key.accessors` | grouped | `key.Key.Type`<br>`key.Key.Length`<br>`key.Key.LengthSize`<br>`key.Key.Hash` | `key::Key::type_`<br>`key::Key::length`<br>`key::Key::length_size`<br>`key::Key::hash`<br>`key::Key::as_bytes` | `type+length+length_size+hash` | `key/accessors` | Single field reads. Timed as one batched case: separately they would measure the loop, not the core. Rust `as_bytes` is the borrow Go gets from the array type itself. |
| `key.Key / key.Type` | type | `key.Key`<br>`key.Type` | `key::Key`<br>`key::Type` | — | — | Value types. Every operation above constructs or reads one. |
| `key sizes and type tags` | constant | `key.Size`<br>`key.Blob`<br>`key.FileNode`<br>`key.DirLeaf`<br>`key.DirNode`<br>`key.XattrSet` | `key::SIZE` | — | `key/type-names` | Go exports the five object types as package constants; in Rust they are variants of the `key::Type` enum and so are not separate exported symbols. Both are exercised by the type-name check. |
| `key errors` | error | `key.ErrBadKeyLength`<br>`key.ErrNonCanonicalLength`<br>`key.ErrReservedBitSet`<br>`key.ErrReservedType` | `key::Error` | — | `key/parse-rejects` | Go sentinels matched with `errors.Is`; Rust variants matched by pattern. The rejection check asserts one specific error per malformed input on both sides. |

### `cbor`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `cbor.decode_xattrs` | measured | `cborx.DecodeXattrs` | `cbor::decode_xattrs` | `xattrs-3`<br>`xattrs-64` | `cbor/rejects-trailing` |  |
| `cbor.encode_xattrs` | measured | `cborx.EncodeXattrs` | `cbor::encode_xattrs` | `xattrs-3`<br>`xattrs-64` | `cbor/roundtrip-xattrs-3`<br>`cbor/roundtrip-xattrs-64`<br>`cbor/canonical-stable` | Canonical byte-string-keyed CBOR map. The stability check runs the encoder 32 times because Go map iteration order is randomised. |
| `cbor.head_primitives` | measured-rust-only | — | `cbor::append_head`<br>`cbor::append_bstr`<br>`cbor::read_head`<br>`cbor::read_bstr` | `append+read/rust-only` | `cbor/head-primitives-roundtrip` | Rust-only. The Go core keeps the equivalent head and byte-string primitives unexported inside `cborx`, which publishes only the xattr codec, so there is nothing to pair them with. |
| `cbor major types` | constant | — | `cbor::MAJOR_UINT`<br>`cbor::MAJOR_NEGINT`<br>`cbor::MAJOR_BSTR`<br>`cbor::MAJOR_TSTR`<br>`cbor::MAJOR_ARRAY`<br>`cbor::MAJOR_MAP` | — | — | Rust-only constants, part of the exported head primitives above. |
| `cbor errors` | error | — | `cbor::Error` | — | — | Rust-only: Go's `cborx` returns plain wrapped errors. |

### `binaryfuse`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `binaryfuse.contains` | measured-rust-only | — | `binaryfuse::BinaryFuse16::contains` | `store-keys/rust-only` | `binaryfuse/no-false-negatives` | Rust-only, as above. |
| `binaryfuse.new` | measured-rust-only | — | `binaryfuse::BinaryFuse16::new` | `store-keys/rust-only` | `binaryfuse/no-false-negatives` | Rust-only. The Go core builds the same 16-bit binary fuse filter with the external `github.com/FastFilter/xorfilter` dependency and exports no filter package of its own, so there is no Go symbol to pair with. Both cores exercise the filter indirectly through segment sealing. |
| `binaryfuse.parse_section` | measured-rust-only | — | `binaryfuse::BinaryFuse16::parse_section` | `store-keys/rust-only` | `binaryfuse/section-roundtrip` | Rust-only, as above. |
| `binaryfuse.section_bytes` | measured-rust-only | — | `binaryfuse::BinaryFuse16::section_bytes` | `store-keys/rust-only` | `binaryfuse/section-roundtrip` | Rust-only: the Go core keeps filter-section serialization unexported inside `packstore`. |
| `binaryfuse.BinaryFuse16` | type | — | `binaryfuse::BinaryFuse16` | — | — |  |
| `binaryfuse constants` | constant | — | `binaryfuse::MAX_ITERATIONS`<br>`binaryfuse::SECTION_HEADER_SIZE`<br>`binaryfuse::SECTION_TYPE_BINARY_FUSE16` | — | — |  |
| `binaryfuse errors` | error | — | `binaryfuse::Error`<br>`binaryfuse::Error::is_corrupt` | — | — |  |

### `chunkers`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `chunkers.item_chunker` | measured | `chunkers.ItemChunker.IsBoundary` | `chunkers::ItemChunker::is_boundary` | `is_boundary/bits-7` | `chunkers/item-boundaries` |  |
| `chunkers.new_item_chunker` | measured | `chunkers.NewItemChunker` | `chunkers::ItemChunker::new` | `bits-4..12` | `chunkers/item-bounds` | A constructor, so it is batched over nine bit widths rather than timed once. |
| `chunkers.split_bytes` | measured | `chunkers.SplitBytes` | `chunkers::split_bytes` | `random/default-sizes`<br>`compressible/default-sizes`<br>`tiny-64KiB/default-sizes`<br>`compressible/4-16-64KiB` | `chunkers/split-random`<br>`chunkers/split-compressible`<br>`chunkers/split-bounds-random`<br>`chunkers/split-bounds-compressible`<br>`chunkers/split-empty`<br>`chunkers/split-propagates-error` | The UltraCDC byte chunker. Chunk boundaries determine every file key, so the checks compare the full chunk list across cores, not just the count. A non-default size configuration is measured too. |
| `chunkers option and chunker types` | type | `chunkers.ByteOpts`<br>`chunkers.ItemChunker` | `chunkers::ByteOpts`<br>`chunkers::ItemChunker` | — | — | Go re-exports the upstream `ChunkerOpts`; Rust defines an equivalent struct with the same fields. |
| `chunkers default sizes` | constant | `chunkers.DefaultMinSize`<br>`chunkers.DefaultNormalSize`<br>`chunkers.DefaultMaxSize` | `chunkers::DEFAULT_MIN_SIZE`<br>`chunkers::DEFAULT_NORMAL_SIZE`<br>`chunkers::DEFAULT_MAX_SIZE` | — | `chunkers/split-bounds-random` | The bound checks assert every chunk respects them. |
| `chunkers errors` | error | — | `chunkers::OptionsError`<br>`chunkers::SplitError` | — | `chunkers/split-propagates-error` | Rust names the option and callback failures; Go returns the upstream error and the callback error unchanged. The propagation check covers both spellings. |

### `fstree`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `fstree.check_complete` | measured | `fstree.CheckComplete` | `fstree::check_complete` | `wide/jobs-1`<br>`wide/jobs-N`<br>`incomplete/jobs-1` | `fstree/check-complete`<br>`fstree/check-complete-missing` | Single-threaded and concurrent, plus the failure path with one leaf removed from the store. |
| `fstree.child_keys` | measured | `fstree.ChildKeys` | `fstree::child_keys` | `dir-leaf-128`<br>`dir-node-128`<br>`file-node-1024` | `fstree/child-keys-dir-leaf`<br>`fstree/child-keys-blob-is-leaf` | All three interior object types plus the leaf case. |
| `fstree.collect_entries` | measured | `fstree.CollectEntries` | `fstree::collect_entries` | `wide` | `fstree/collect-entries` |  |
| `fstree.decode_dir_leaf` | measured | `fstree.DecodeDirLeaf` | `fstree::decode_dir_leaf` | `entries-8`<br>`entries-128-with-xattrs` | `fstree/decode-dir-leaf`<br>`fstree/decode-rejects-truncated` |  |
| `fstree.decode_dir_node` | measured | `fstree.DecodeDirNode` | `fstree::decode_dir_node` | `pairs-128` | `fstree/decode-dir-node` |  |
| `fstree.decode_file_node` | measured | `fstree.DecodeFileNode` | `fstree::decode_file_node` | `children-1024` | `fstree/decode-file-node` |  |
| `fstree.dir_builder` | measured | `fstree.NewDirBuilder`<br>`fstree.DirBuilder.AddEntry`<br>`fstree.DirBuilder.Finish` | `fstree::DirBuilder::new`<br>`fstree::DirBuilder::add_entry`<br>`fstree::DirBuilder::finish` | `entries-128`<br>`entries-<profile wide>` | `fstree/dir-builder-root` | The whole streaming build of one directory: chunk entries into leaves and promote their keys through the index. The root key is the cross-core statement. |
| `fstree.encode_blob` | measured | `fstree.EncodeBlob` | `fstree::encode_blob` | `tiny-64B`<br>`large-1MiB-text` | `fstree/encode-blob` |  |
| `fstree.encode_dir_leaf` | measured | `fstree.EncodeDirLeaf` | `fstree::encode_dir_leaf` | `entries-8`<br>`entries-128-with-xattrs` | `fstree/encode-dir-leaf-8`<br>`fstree/encode-dir-leaf-128` | One workload carries inline extended attributes, which is the expensive branch of the entry encoder. |
| `fstree.encode_dir_node` | measured | `fstree.EncodeDirNode` | `fstree::encode_dir_node` | `pairs-8`<br>`pairs-128` | `fstree/encode-dir-node-8`<br>`fstree/encode-dir-node-128` |  |
| `fstree.encode_file_node` | measured | `fstree.EncodeFileNode` | `fstree::encode_file_node` | `children-8`<br>`children-1024` | `fstree/encode-file-node-8`<br>`fstree/encode-file-node-1024` |  |
| `fstree.encode_xattr_set` | measured | `fstree.EncodeXattrSet` | `fstree::encode_xattr_set` | `xattrs-64` | `fstree/encode-xattr-set-64` |  |
| `fstree.index_builder_file` | measured | `fstree.NewFileIndexBuilder`<br>`fstree.IndexBuilder.AddChild`<br>`fstree.IndexBuilder.Finish` | `fstree::IndexBuilder::new_file`<br>`fstree::IndexBuilder::add_child`<br>`fstree::IndexBuilder::finish` | `children-128`<br>`children-65536` | `fstree/file-index-root` | Neither core exports the directory index builder: directories go through `DirBuilder`, which is measured above. |
| `fstree.list_entries` | measured | `fstree.ListEntries` | `fstree::list_entries` | `wide/first-page-100`<br>`wide/full-paging-100` | `fstree/list-paging` | One page, and paging the whole directory; the check asserts every entry appears exactly once in name order. |
| `fstree.lookup_entry` | measured | `fstree.LookupEntry` | `fstree::lookup_entry` | `wide/hit`<br>`wide/miss`<br>`shallow/hit` | `fstree/lookup-hit`<br>`fstree/lookup-miss` | Hit and miss, over a wide prolly tree and a single-leaf one. |
| `fstree.reachable_keys` | measured | `fstree.ReachableKeys` | `fstree::reachable_keys` | `wide`<br>`deep`<br>`file-corpus` | `fstree/reachable-root-first`<br>`fstree/reachable-distinct` | Both cores pick their own walk parallelism here, so the case is marked `auto` and both run under one CPU set. |
| `fstree.resolve_entry` | measured | `fstree.ResolveEntry` | `fstree::resolve_entry` | `deep` | `fstree/resolve-entry-root-is-nil` |  |
| `fstree.resolve_path` | measured | `fstree.ResolvePath` | `fstree::resolve_path` | `deep` | `fstree/resolve-path`<br>`fstree/resolve-path-missing`<br>`fstree/resolve-path-rejects-dotdot` |  |
| `fstree.write_content` | measured | `fstree.WriteContent` | `fstree::write_content` | `file-corpus` | `fstree/write-content` | Reassembles a multi-level file; the check compares the bytes with the source corpus. |
| `fstree object and entry types` | type | `fstree.Object`<br>`fstree.Entry`<br>`fstree.DirPair`<br>`fstree.DirBuilder`<br>`fstree.IndexBuilder`<br>`fstree.Emit` | `fstree::Object`<br>`fstree::Entry`<br>`fstree::DirPair`<br>`fstree::DirBuilder`<br>`fstree::IndexBuilder` | — | — | Go names the emit callback as an exported function type; Rust takes a generic `FnMut`, which is not a symbol. |
| `fstree errors` | error | `fstree.ErrNotFound`<br>`fstree.ErrNotDir`<br>`fstree.MissingObjectError`<br>`fstree.MissingObjectError.Error` | `fstree::Error`<br>`fstree::WalkError`<br>`fstree::WalkError::is_not_found`<br>`fstree::WalkError::is_not_dir`<br>`fstree::WalkError::missing_object`<br>`fstree::MissingObjectError`<br>`fstree::ChildKeysError`<br>`fstree::BuildError`<br>`fstree::CborError`<br>`fstree::CborType`<br>`fstree::CborType::name` | — | `fstree/lookup-miss`<br>`fstree/resolve-path-missing`<br>`fstree/check-complete-missing`<br>`fstree/decode-rejects-truncated` | Go wraps sentinels; Rust names one variant per wrap site and offers `is_*` classifiers. `CborType` is Rust's diagnostic for decoder errors, which Go gets from its CBOR dependency. The failure-path checks assert the same classification on both sides. |

### `amberignore`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `amberignore.descend` | measured | `amberignore.Matcher.Descend` | `amberignore::Matcher::descend`<br>`amberignore::descend_opt` | `8-subdirs` | `amberignore/descend-composes` | Rust's `descend_opt` is the spelling of Go's nil-receiver call. |
| `amberignore.ignored` | measured | `amberignore.Matcher.Ignored` | `amberignore::Matcher::ignored`<br>`amberignore::ignored_opt` | `mixed-names`<br>`nil-matcher` | `amberignore/root-patterns`<br>`amberignore/nil-matcher-ignores-nothing` | The nil-matcher workload measures Go's nil receiver against Rust's `ignored_opt(None, ..)`. |
| `amberignore.root` | measured | `amberignore.Root` | `amberignore::Matcher::root` | `load-root` | `amberignore/root-patterns` |  |
| `amberignore.Matcher` | type | `amberignore.Matcher` | `amberignore::Matcher` | — | — |  |
| `amberignore.FileName` | constant | `amberignore.FileName` | `amberignore::FILE_NAME` | — | `amberignore/root-patterns` | The check asserts the ignore file is never itself ignored. |

### `ingest`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `ingest.dir` | measured | `ingest.Dir` | `ingest::dir` | `tree/fresh-store/jobs-1`<br>`tree/fresh-store/jobs-N`<br>`tree/incremental-change/jobs-1`<br>`tree/incremental-change/jobs-N`<br>`single-file/jobs-1` | `ingest/dir-root`<br>`ingest/dir-dedups`<br>`ingest/dir-incremental` | Fresh store, a re-ingest after a deterministic incremental change, and a single regular file. |
| `ingest.objects` | measured | `ingest.Objects` | `ingest::objects` | `tree/jobs-1`<br>`tree/jobs-N` | `ingest/objects-root`<br>`ingest/objects-jobs-invariant`<br>`ingest/objects-complete`<br>`ingest/objects-resolves-file`<br>`ingest/objects-honours-ignore`<br>`ingest/objects-honours-negation` | The root key of the ingested fixture tree is identical in both cores, which is the deepest single equality this benchmark asserts. |
| `ingest.scan` | measured | `ingest.Scan` | `ingest::scan` | `tree/jobs-1`<br>`tree/jobs-N`<br>`tree/no-ignore/jobs-1` | `ingest/scan-filtered`<br>`ingest/scan-unfiltered`<br>`ingest/scan-jobs-invariant` | With and without ignore filtering, single-threaded and concurrent. |
| `ingest option types` | type | `ingest.Opts`<br>`ingest.ChunkOpts`<br>`ingest.Progress` | `ingest::Opts`<br>`ingest::ChunkOpts`<br>`ingest::Progress`<br>`ingest::ObjectStream`<br>`ingest::Root`<br>`ingest::Root::get` | — | — | Rust returns the object stream and the deferred root as named types; Go returns an iterator and a `*key.Key`. `Root::get` is the read of that deferred value, exercised by every `ingest.objects` case. |
| `ingest defaults` | constant | `ingest.DefaultItemBits`<br>`ingest.DefaultXattrInlineMax` | `ingest::DEFAULT_ITEM_BITS`<br>`ingest::DEFAULT_XATTR_INLINE_MAX` | — | — | Both drivers build their synthetic trees with the default item width, so the builder cases match the ingest path. |
| `ingest errors` | error | — | `ingest::Error` | — | — | Rust names the build failures; Go returns `*fs.PathError` and the encoder errors unchanged. |

### `amberpack`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `amberpack.decode_payload` | measured | `amberpack.DecodePayload` | `amberpack::decode_payload` | `mixed-records` | `amberpack/record-roundtrip` |  |
| `amberpack.encode_record` | measured | `amberpack.EncodeRecord` | `amberpack::encode_record` | `tiny-64B-random`<br>`small-4KiB-random`<br>`small-4KiB-text`<br>`large-1MiB-text`<br>`large-1MiB-random` | `amberpack/record-roundtrip`<br>`amberpack/compresses-only-when-smaller` | Compressible and random payloads at three sizes: the compressor is the whole cost, and the two cores use different zstd implementations. |
| `amberpack.parse_record` | measured | `amberpack.ParseRecord` | `amberpack::parse_record` | `mixed-records`<br>`corrupt-crc` | `amberpack/detects-crc-corruption`<br>`amberpack/rejects-non-canonical-key` | Validation of a good record and of a record with a flipped payload byte. |
| `amberpack.reader_all` | measured | `amberpack.NewReader`<br>`amberpack.Reader.All` | `amberpack::Reader::new`<br>`amberpack::Reader (Iterator)` | `mixed-objects`<br>`truncated-stream` | `amberpack/reader-roundtrip`<br>`amberpack/rejects-truncated`<br>`amberpack/rejects-legacy-magic` | Rust spells `All` as the `Reader`'s own `Iterator` impl. |
| `amberpack.reader_records` | measured | `amberpack.Reader.Records` | `amberpack::Reader::records` | `mixed-objects` | `amberpack/record-passthrough` |  |
| `amberpack.writer_add` | measured | `amberpack.NewWriter`<br>`amberpack.Writer.Add`<br>`amberpack.Writer.Close` | `amberpack::Writer::new`<br>`amberpack::Writer::add`<br>`amberpack::Writer::finish` | `mixed-objects` | `amberpack/empty-pack` |  |
| `amberpack.writer_add_record` | measured | `amberpack.Writer.AddRecord` | `amberpack::Writer::add_record` | `pre-encoded-records` | `amberpack/record-passthrough` | The zero-copy push path: pre-encoded records appended verbatim. |
| `amberpack record and stream types` | type | `amberpack.Reader`<br>`amberpack.Writer`<br>`amberpack.Record`<br>`amberpack.RawRecord` | `amberpack::Reader`<br>`amberpack::Writer`<br>`amberpack::Record`<br>`amberpack::RawRecord`<br>`amberpack::Records` | — | — | `Records` is the Rust iterator `Reader::records` returns; Go returns an `iter.Seq2`. |
| `amberpack constants` | constant | `amberpack.RecHeaderSize`<br>`amberpack.MaxPayload` | `amberpack::REC_HEADER_SIZE`<br>`amberpack::MAX_PAYLOAD` | — | — | Both drivers slice record payloads with the header size, so a disagreement would fail the round-trip check. |
| `amberpack errors` | error | `amberpack.ErrCorrupt`<br>`amberpack.ErrMalformed` | `amberpack::Error`<br>`amberpack::Error::is_corrupt`<br>`amberpack::Error::is_malformed` | — | `amberpack/detects-crc-corruption`<br>`amberpack/rejects-truncated` |  |

### `packstore`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `packstore.append_record` | measured | `packstore.Store.AppendRecord`<br>`packstore.Store.Sync` | `packstore::Store::append_record`<br>`packstore::Store::sync` | `pre-encoded+sync` | — | A batch of pre-encoded records followed by the one fsync that makes them durable; the two operations only make sense together. |
| `packstore.barrier` | measured | `packstore.Store.BeginBarrier`<br>`packstore.Store.ObserveKeys`<br>`packstore.Store.AbortBarrier` | `packstore::Store::begin_barrier`<br>`packstore::Store::observe_keys`<br>`packstore::Store::abort_barrier` | `begin+observe+abort` | — | The grey-capture span is one operation in practice: opening it, observing a closure and discarding it. |
| `packstore.close` | measured | `packstore.Store.Close` | `packstore::Store::close` | `populated` | `packstore/closed-store-errors` |  |
| `packstore.compact` | measured | `packstore.Store.Compact` | `packstore::Store::compact` | `90-percent-dead`<br>`nothing-dead` | `packstore/compact-retains-live`<br>`packstore/compact-reclaims`<br>`packstore/verify-after-compact` | Run on an isolated copy of a store whose sealed segments are 90 % dead, so there is real reclamation, and the retained objects are verified afterwards. |
| `packstore.get` | measured | `packstore.Store.Get` | `packstore::Store::get` | `hit`<br>`miss` | `packstore/get-content-addressed`<br>`packstore/get-miss` |  |
| `packstore.get_record` | measured | `packstore.Store.GetRecord` | `packstore::Store::get_record` | `hit` | `packstore/get-record` |  |
| `packstore.has` | measured | `packstore.Store.Has` | `packstore::Store::has` | `hit`<br>`miss` | `packstore/has` |  |
| `packstore.has_outside` | measured | `packstore.Store.HasOutside` | `packstore::Store::has_outside` | `sealed-segment` | `packstore/has-outside-miss` |  |
| `packstore.liveness` | measured | `packstore.Store.Liveness` | `packstore::Store::liveness` | `tenth-live` | `packstore/liveness-accounts-all` |  |
| `packstore.mark_set_contains` | measured | `packstore.MarkSet.Contains` | `packstore::MarkSet::contains` | `all-keys` | `packstore/markset` |  |
| `packstore.mark_set_mark` | measured | `packstore.MarkSet.Mark`<br>`packstore.MarkSet.Marked` | `packstore::MarkSet::mark`<br>`packstore::MarkSet::marked` | `all-keys` | `packstore/markset` | `Marked` is a counter read, batched into the marking case. |
| `packstore.missing` | measured | `packstore.Store.Missing` | `packstore::Store::missing` | `half-present` | `packstore/missing` | Order and multiplicity are part of the contract and are checked. |
| `packstore.new_mark_set` | measured | `packstore.Store.NewMarkSet` | `packstore::Store::new_mark_set` | `snapshot` | `packstore/markset` |  |
| `packstore.open` | measured | `packstore.Open` | `packstore::Store::open`<br>`packstore::Store::open_with` | `empty`<br>`populated-reopen` | `packstore/scan-index-matches-footer` | Reopening a populated store is the recovery path: sealed segments are mmapped and validated and the active segment is tail-scanned. |
| `packstore.put` | measured | `packstore.Store.Put` | `packstore::Store::put` | `tiny-128B/sync-off`<br>`tiny-128B/sync-on`<br>`large-256KiB/sync-off`<br>`duplicate/sync-off` | `packstore/get-content-addressed` | Both durability boundaries and both object sizes, plus the dedup-hit path. |
| `packstore.put_verified` | measured | `packstore.Store.PutVerified` | `packstore::Store::put_verified` | `already-intact` | `packstore/repairs-corruption` | The timed workload is the no-repair-needed path. The repair itself is a correctness check: a payload byte is flipped in an isolated copy of a sealed segment, the scrub must see it, and the repair must restore the object. Nothing destructive ever touches a shared fixture. |
| `packstore.record` | measured | `packstore.Store.Record` | `packstore::Store::record` | `by-location` | `packstore/record-by-location` |  |
| `packstore.remove` | measured | `packstore.Store.Remove` | `packstore::Store::remove` | `one-sealed-segment` | — | Destructive: measured on an isolated copy per repetition. |
| `packstore.scan_index` | measured | `packstore.Store.ScanIndex` | `packstore::Store::scan_index` | `one-segment` | `packstore/scan-index-matches-footer` |  |
| `packstore.segments` | measured | `packstore.Store.Segments` | `packstore::Store::segments` | `list` | — |  |
| `packstore.sort_by_location` | measured | `packstore.Store.SortByLocation` | `packstore::Store::sort_by_location` | `scattered` | `packstore/sort-is-permutation` |  |
| `packstore.stored_size` | measured | `packstore.Store.StoredSize` | `packstore::Store::stored_size` | `hit` | `packstore/stored-size` |  |
| `packstore.verify` | measured | `packstore.Store.Verify` | `packstore::Store::verify` | `full-scrub` | `packstore/verify-clean`<br>`packstore/verify-detects-corruption`<br>`packstore/verify-after-compact` | The scrub is also the oracle for the repair test: it must report the injected corruption and must be clean afterwards. |
| `packstore.wipe` | measured | `packstore.Store.Wipe` | `packstore::Store::wipe` | `populated` | `packstore/wipe` | Destructive: measured on an isolated copy per repetition. |
| `packstore.write_batch` | measured | `packstore.Store.WriteBatch` | `packstore::Store::write_batch` | `mixed-objects` | — |  |
| `packstore.write_parallel` | measured | `packstore.Store.WriteParallel` | `packstore::Store::write_parallel` | `mixed-objects/writers-1/verify-false`<br>`mixed-objects/writers-1/verify-true`<br>`mixed-objects/writers-N/verify-false`<br>`mixed-objects/writers-N/verify-true`<br>`duplicate-stream/writers-N` | `packstore/verify-rejects-mismatch`<br>`packstore/dedups-within-batch` | One and many writers, with and without per-object verification, plus a fully duplicate stream. |
| `packstore store options` | grouped | `packstore.WithSegmentSize`<br>`packstore.WithSync`<br>`packstore.Option` | `packstore::Options`<br>`packstore::Options::new`<br>`packstore::Options::segment_size`<br>`packstore::Options::sync` | `packstore.open/*`<br>`packstore.put/sync-on`<br>`packstore.put/sync-off` | — | Configuration, not work: Go uses functional options, Rust a builder. Every store case applies both of them, and the sync setting is a measured dimension of `packstore.put`. |
| `packstore.oldest_inflight_write` | grouped | `packstore.Store.OldestInflightWrite` | `packstore::Store::oldest_inflight_write` | `idle` | — | A single guarded field read; batched 4096 times so the interval is measurable at all. |
| `packstore data types` | type | `packstore.Store`<br>`packstore.Object`<br>`packstore.MarkSet`<br>`packstore.SegmentInfo`<br>`packstore.SegmentLiveness`<br>`packstore.WriteOpts`<br>`packstore.WriteStats`<br>`packstore.CompactOpts`<br>`packstore.CompactStats` | `packstore::Store`<br>`packstore::Object`<br>`packstore::MarkSet`<br>`packstore::SegmentInfo`<br>`packstore::SegmentLiveness`<br>`packstore::WriteOpts`<br>`packstore::WriteStats`<br>`packstore::CompactOpts`<br>`packstore::CompactStats` | — | — |  |
| `packstore constants` | constant | `packstore.DefaultBatchSize`<br>`packstore.DefaultSegmentSize` | `packstore::DEFAULT_BATCH_SIZE`<br>`packstore::DEFAULT_SEGMENT_SIZE` | — | — | The profile overrides the segment size so the fixtures really have several sealed segments; the batch size is left at the default on both sides. |
| `packstore errors` | error | `packstore.ErrNotFound`<br>`packstore.ErrClosed`<br>`packstore.ErrCorrupt`<br>`packstore.ErrVerify`<br>`packstore.ErrUnknownSegment` | `packstore::Error`<br>`packstore::Error::is_not_found`<br>`packstore::Error::is_closed`<br>`packstore::Error::is_corrupt`<br>`packstore::Error::is_verify`<br>`packstore::Error::is_unknown_segment` | — | `packstore/get-miss`<br>`packstore/closed-store-errors`<br>`packstore/verify-detects-corruption`<br>`packstore/verify-rejects-mismatch` | `ErrUnknownSegment` / `is_unknown_segment` is the only sentinel without its own check: reaching it needs a segment id that was never sealed, which the benchmark has no operation for. |

### `reference`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `reference.decode` | measured | `reference.Decode` | `reference::Reference::decode` | `signed` | `reference/roundtrip`<br>`reference/rejects-trailing` |  |
| `reference.encode` | measured | `reference.Reference.Encode` | `reference::Reference::encode` | `signed` | `reference/encode`<br>`reference/roundtrip` |  |
| `reference.signature_payload` | measured | `reference.Reference.SignaturePayload` | `reference::Reference::signature_payload` | `signed` | `reference/signature-payload`<br>`reference/signature-payload-excludes-sig` |  |
| `reference.validate_name` | measured | `reference.ValidateName` | `reference::validate_name` | `valid` | `reference/name-rules` |  |
| `reference.validate_user` | measured | `reference.ValidateUser` | `reference::validate_user` | `valid` | `reference/user-rules` |  |
| `reference.Reference` | type | `reference.Reference` | `reference::Reference` | — | — |  |
| `reference limits` | constant | `reference.MaxNameLen`<br>`reference.MaxUserLen`<br>`reference.MaxSignatureLen`<br>`reference.MaxPublicKeyLen` | `reference::MAX_NAME_LEN`<br>`reference::MAX_USER_LEN`<br>`reference::MAX_SIGNATURE_LEN`<br>`reference::MAX_PUBLIC_KEY_LEN` | — | `reference/name-rules` |  |
| `reference errors` | error | — | `reference::Error`<br>`reference::DecodeError`<br>`reference::NameError`<br>`reference::UserError` | — | `reference/name-rules`<br>`reference/user-rules` | Rust names the validation and decoding failures; Go returns formatted errors without exported sentinels. |

### `refstore`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `refstore.all` | measured | `refstore.Store.All` | `refstore::Store::all` | `records-N` | `refstore/all-lexicographic` |  |
| `refstore.close` | measured | `refstore.Store.Close` | `refstore::Store (Drop)` | `populated` | — | Go exports `Close`; the redb port closes on drop and exports no close method, so the Rust case measures the drop. Same operation, different spelling. |
| `refstore.delete` | measured | `refstore.Store.Delete` | `refstore::Store::delete` | `records-N` | `refstore/delete`<br>`refstore/delete-absent` |  |
| `refstore.get` | measured | `refstore.Store.Get` | `refstore::Store::get` | `hit`<br>`miss` | `refstore/get-verbatim`<br>`refstore/get-miss` |  |
| `refstore.open` | measured | `refstore.Open` | `refstore::Store::open` | `empty`<br>`populated-reopen` | — | Pebble in Go, redb in Rust: the same operation over different storage engines. See the scope limits in the report. |
| `refstore.put` | measured | `refstore.Store.Put` | `refstore::Store::put` | `records-N/sync-false`<br>`records-N/sync-true` | `refstore/put-overwrites` | Both durability settings. |
| `refstore.put_batch` | measured | `refstore.Store.PutBatch` | `refstore::Store::put_batch` | `records-N/sync-false` | `refstore/batch-last-wins` |  |
| `refstore.wipe` | measured | `refstore.Store.Wipe` | `refstore::Store::wipe` | `records-N` | `refstore/wipe` |  |
| `refstore data types` | type | `refstore.Store`<br>`refstore.Record` | `refstore::Store`<br>`refstore::Record` | — | — |  |
| `refstore errors` | error | `refstore.ErrNotFound` | `refstore::Error`<br>`refstore::Error::is_not_found` | — | `refstore/get-miss`<br>`refstore/delete-absent` |  |

### `inbox`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `inbox.close` | measured | `inbox.Inbox.Close` | `inbox::Inbox::close` | `drained` | — |  |
| `inbox.commit` | measured | `inbox.Inbox.Commit` | `inbox::Inbox::commit` | `packs-N/duplicate` | `inbox/commit-adds`<br>`inbox/commit-idempotent` | The timed workload is the idempotent path, reached by retiring the workers first so a committed entry is still on disk. The first-commit path is timed as part of `inbox.drain`. |
| `inbox.discard` | measured | `inbox.Inbox.Discard` | `inbox::Inbox::discard` | `packs-N` | `inbox/discard-removes-tmp` |  |
| `inbox.drain` | measured | `inbox.Inbox.WaitFor` | `inbox::Inbox::wait_for` | `packs-N/new` | `inbox/drain-stores-objects`<br>`inbox/wait-for-empty-group` | End to end: commit every staged pack (the `inbox.commit` symbols, inventoried on the row above) and then wait for the worker pool to store its objects. The wait cannot be timed alone, because the drain starts at the commit, so this row carries the wait and the composite. |
| `inbox.open` | measured | `inbox.Open` | `inbox::Inbox::open` | `empty`<br>`sweeps-staged-tmp-files` | — | The second workload is the recovery path: staged-but-uncommitted files left by a previous run are swept at open. |
| `inbox.stage` | measured | `inbox.Inbox.Stage` | `inbox::Inbox::stage` | `packs-N` | `inbox/stage-hashes-body` |  |
| `inbox.with_gate` | measured-go-only | `inbox.Option`<br>`inbox.WithGate` | — | — | — | Go-only. The Go inbox takes functional options and exports `WithGate` to bracket each entry's store write with the collector's write gate; the Rust inbox has no options parameter and no equivalent export, so there is nothing to pair. Not measured on the Go side either: timing an option constructor would measure a closure allocation. |
| `inbox data types` | type | `inbox.Inbox`<br>`inbox.Meta` | `inbox::Inbox`<br>`inbox::Meta`<br>`inbox::LogFn` | — | — | Rust names the log callback type; Go takes a `*slog.Logger` from its standard library. |

### `gc`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `gc.close` | measured | `gc.Collector.Close` | `gc::Collector::close` | `idle` | — |  |
| `gc.open` | measured | `gc.Open` | `gc::Collector::open` | `populated` | — |  |
| `gc.prepare_ref` | measured | `gc.Collector.PrepareRef` | `gc::Collector::prepare_ref`<br>`gc::PreparedRef`<br>`gc::PreparedRef::commit`<br>`gc::PreparedRef::abort` | `tree-root/commit`<br>`tree-root/abort`<br>`missing-root` | `gc/prepare-accepts-complete`<br>`gc/prepare-rejects-incomplete` | Commit, abort and the rejection of a root whose objects are not stored. Go returns two closures; Rust returns a `PreparedRef` guard. |
| `gc.release_ref` | measured | `gc.Collector.ReleaseRef` | `gc::Collector::release_ref` | `batch` | `gc/release-is-a-noop` | A no-op kept for protocol symmetry in both cores; batched 4096 times so there is an interval to measure. |
| `gc.run` | measured | `gc.Collector.Run` | `gc::Collector::run` | `reclaimable-packs`<br>`nothing-to-reclaim` | `gc/run-reclaims`<br>`gc/run-marks-live`<br>`gc/run-retains-live`<br>`gc/run-tree-still-complete`<br>`gc/store-scrubs-clean-after-run`<br>`gc/reaches-a-fixed-point`<br>`gc/fixed-point-retains-live` | A real cycle over a fixture with several whole reclaimable packs and one referenced tree. The checks assert the tree survives, still reads end to end, and that repeated cycles terminate. |
| `gc.status` | measured | `gc.Collector.Status` | `gc::Collector::status` | `mark+score` | `gc/status-marks-live`<br>`gc/status-sees-garbage` | A full advisory mark plus per-pack scoring. |
| `gc.why` | measured | `gc.Collector.Why` | `gc::Collector::why` | `live-root` | `gc/why-names-the-reference`<br>`gc/why-unreferenced` |  |
| `gc.wipe` | measured | `gc.Collector.Wipe` | `gc::Collector::wipe` | `store-reset` | `gc/wipe-resets` |  |
| `gc.begin_write` | measured-go-only | `gc.Collector.BeginWrite` | — | `gate-span` | `gc/begin-write-release-is-idempotent` | Go-only. The Rust core keeps the equivalent gate internal: `packstore::Store::begin_write` is `pub(super)` and the collector exports no `BeginWrite`. Measured on the Go side and reported in the single-core table, never paired. |
| `gc data types` | type | `gc.Collector`<br>`gc.Options`<br>`gc.CycleStats`<br>`gc.Status`<br>`gc.PackStatus` | `gc::Collector`<br>`gc::Options`<br>`gc::CycleStats`<br>`gc::Status`<br>`gc::PackStatus` | — | — |  |
| `gc defaults` | constant | `gc.DefaultGrace`<br>`gc.DefaultGarbage` | `gc::DEFAULT_GRACE`<br>`gc::DEFAULT_GARBAGE` | — | — | Both drivers pass the default garbage line explicitly and set a one-nanosecond grace, so freshly written fixture segments are eligible and the free-space policy never enters the measurement. |
| `gc errors` | error | `gc.ErrCycleRunning` | `gc::Error`<br>`gc::Error::is_cycle_running`<br>`gc::Error::is_canceled` | — | — | Overlapping and cancelled cycles. The benchmark never overlaps cycles — it runs them one at a time by construction — so these are inventoried rather than exercised. |

### `tarexport`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `tarexport.write` | measured | `tarexport.Write` | `tarexport::write` | `fixture-tree` | `tar/export-bytes`<br>`tar/export-rejects-non-directory` | PAX export is specified to be byte-identical across the cores, and the check compares the whole archive digest. |

### `tarextract`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `tarextract.extract` | measured | `tarextract.Extract` | `tarextract::extract` | `fixture-tree` | `tar/extract-manifest`<br>`tar/extract-content-matches-source`<br>`tar/extract-symlink`<br>`tar/extract-honours-ingest-filter` | The extracted tree's canonical manifest — type, permissions, size, mtime, content digest, symlink target — is compared across cores. |

### `tar`

| Operation | Status | Go | Rust | Workloads | Checks | Notes |
|---|---|---|---|---|---|---|
| `tar types` | type | `tarexport.Getter` | `tarexport::Error`<br>`tarextract::Error` | — | — | Go names the fetch callback type; Rust takes a generic closure and names the error types instead. |

