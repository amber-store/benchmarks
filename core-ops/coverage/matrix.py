#!/usr/bin/env python3
"""The coverage matrix: every exported symbol of both cores, accounted for.

`ROWS` maps each exported Go symbol and each exported Rust symbol to one of:

  * a paired benchmark operation measured in both cores,
  * an operation only one core exports, measured on that side and never
    compared,
  * a correctness check that exercises it without a separate timing,
  * a grouped case, for accessors and constructors too small to time alone,
  * or a precise reason it is not an operation at all (a data type, a
    constant, an error value).

`check` proves the mapping is total and unambiguous: every symbol the
extractors find appears in exactly one row, and every row names only symbols
that exist. Given a report it additionally proves that every operation the
matrix calls measured really produced samples.

Regenerate the extracted surfaces and the Markdown with:

    ./core-ops/coverage/refresh.sh
"""
import argparse
import json
import os
import sys

# Status values, in the order they are rendered.
MEASURED = 'measured'                  # paired, timed in both cores
MEASURED_GO = 'measured-go-only'       # exported by Go only
MEASURED_RUST = 'measured-rust-only'   # exported by Rust only
GROUPED = 'grouped'                    # folded into a grouped timed case
CHECKED = 'checked-only'               # exercised by a check, not timed alone
TYPE = 'type'                          # a data type, not an operation
CONST = 'constant'                     # a constant, not an operation
ERROR = 'error'                        # an error value/type/classifier


def R(op, status, go=(), rust=(), cases=(), checks=(), note=''):
    return {
        'op': op, 'status': status, 'go': list(go), 'rust': list(rust),
        'cases': list(cases), 'checks': list(checks), 'note': note,
    }


ROWS = [
    # ---------------------------------------------------------------- key
    R('key.new', MEASURED, ['key.New'], ['key::Key::new'],
      ['1MiB-duplicate', '1MiB-random', '1MiB-text', '4KiB-duplicate', '4KiB-random',
       '4KiB-text', 'empty', 'large-8MiB-duplicate', 'large-8MiB-random',
       'large-8MiB-text', 'tiny-64B-duplicate', 'tiny-64B-random', 'tiny-64B-text'],
      ['key/new-small'],
      'BLAKE3 over the payload plus header assembly; five payload sizes and '
      'both compressibilities, since the hash dominates at size.'),
    R('key.new_from_hash', MEASURED, ['key.NewFromHash'], ['key::Key::new_from_hash'],
      ['batch'], ['key/new-from-hash'],
      'Header assembly alone, with the digest precomputed.'),
    R('key.parse', MEASURED, ['key.Parse'], ['key::Key::parse'],
      ['canonical', 'malformed'],
      ['key/parse-roundtrip', 'key/parse-rejects'],
      'Both the accepting and the rejecting path; the four malformed inputs '
      'cover each documented rejection reason.'),
    R('key.validate', MEASURED, ['key.Key.Validate'], ['key::Key::validate'],
      ['canonical'], ['key/parse-rejects'], ''),
    R('key.accessors', GROUPED,
      ['key.Key.Type', 'key.Key.Length', 'key.Key.LengthSize', 'key.Key.Hash'],
      ['key::Key::type_', 'key::Key::length', 'key::Key::length_size',
       'key::Key::hash', 'key::Key::as_bytes'],
      ['type+length+length_size+hash'], ['key/accessors'],
      'Single field reads. Timed as one batched case: separately they would '
      'measure the loop, not the core. Rust `as_bytes` is the borrow Go gets '
      'from the array type itself.'),
    R('key.string', MEASURED, ['key.Key.String'], ['key::Key::<Display>', 'key::Key::<Debug>'],
      ['hex'], ['key/parse-roundtrip'],
      'Lowercase hex rendering. Rust spells it as a `Display` impl, which the '
      'extractor does not list as a free symbol.'),
    R('key.type_string', MEASURED,
      ['key.Type.String', 'key.Type.IsValid'],
      ['key::Type::is_valid', 'key::Type::from_u8'],
      ['names'], ['key/type-names'],
      'Type-name rendering and validity. Rust\'s `from_u8` is the checked '
      'conversion Go performs inside `Type.IsValid`; Rust\'s name rendering '
      'is a `Display` impl.'),
    R('key.Key / key.Type', TYPE, ['key.Key', 'key.Type'], ['key::Key', 'key::Type', 'key::Type::<Display>'],
      note='Value types. Every operation above constructs or reads one.'),
    R('key sizes and type tags', CONST,
      ['key.Size', 'key.Blob', 'key.FileNode', 'key.DirLeaf', 'key.DirNode',
       'key.XattrSet'],
      ['key::SIZE'],
      checks=['key/type-names'],
      note='Go exports the five object types as package constants; in Rust '
           'they are variants of the `key::Type` enum and so are not separate '
           'exported symbols. Both are exercised by the type-name check.'),
    R('key errors', ERROR,
      ['key.ErrBadKeyLength', 'key.ErrNonCanonicalLength',
       'key.ErrReservedBitSet', 'key.ErrReservedType'],
      ['key::Error'],
      checks=['key/parse-rejects'],
      note='Go sentinels matched with `errors.Is`; Rust variants matched by '
           'pattern. The rejection check asserts one specific error per '
           'malformed input on both sides.'),

    # --------------------------------------------------------------- cbor
    R('cbor.encode_xattrs', MEASURED, ['cborx.EncodeXattrs'], ['cbor::encode_xattrs'],
      ['xattrs-3', 'xattrs-64'],
      ['cbor/roundtrip-xattrs-3', 'cbor/roundtrip-xattrs-64',
       'cbor/canonical-stable'],
      'Canonical byte-string-keyed CBOR map. The stability check runs the '
      'encoder 32 times because Go map iteration order is randomised.'),
    R('cbor.decode_xattrs', MEASURED, ['cborx.DecodeXattrs'], ['cbor::decode_xattrs'],
      ['xattrs-3', 'xattrs-64'], ['cbor/rejects-trailing'], ''),
    R('cbor.head_primitives', MEASURED_RUST, [],
      ['cbor::append_head', 'cbor::append_bstr', 'cbor::read_head', 'cbor::read_bstr'],
      ['append+read/rust-only'], ['cbor/head-primitives-roundtrip'],
      'Rust-only. The Go core keeps the equivalent head and byte-string '
      'primitives unexported inside `cborx`, which publishes only the xattr '
      'codec, so there is nothing to pair them with.'),
    R('cbor major types', CONST, [],
      ['cbor::MAJOR_UINT', 'cbor::MAJOR_NEGINT', 'cbor::MAJOR_BSTR',
       'cbor::MAJOR_TSTR', 'cbor::MAJOR_ARRAY', 'cbor::MAJOR_MAP'],
      note='Rust-only constants, part of the exported head primitives above.'),
    R('cbor errors', ERROR, [], ['cbor::Error', 'cbor::Error::<Display>', 'cbor::Error::<Error>'],
      note='Rust-only: Go\'s `cborx` returns plain wrapped errors.'),

    # ---------------------------------------------------------- binaryfuse
    R('binaryfuse.new', MEASURED_RUST, [], ['binaryfuse::BinaryFuse16::new'],
      ['store-keys/rust-only'], ['binaryfuse/no-false-negatives'],
      'Rust-only. The Go core builds the same 16-bit binary fuse filter with '
      'the external `github.com/FastFilter/xorfilter` dependency and exports '
      'no filter package of its own, so there is no Go symbol to pair with. '
      'Both cores exercise the filter indirectly through segment sealing.'),
    R('binaryfuse.contains', MEASURED_RUST, [], ['binaryfuse::BinaryFuse16::contains'],
      ['store-keys/rust-only'], ['binaryfuse/no-false-negatives'], 'Rust-only, as above.'),
    R('binaryfuse.section_bytes', MEASURED_RUST, [],
      ['binaryfuse::BinaryFuse16::section_bytes'],
      ['store-keys/rust-only'], ['binaryfuse/section-roundtrip'],
      'Rust-only: the Go core keeps filter-section serialization unexported '
      'inside `packstore`.'),
    R('binaryfuse.parse_section', MEASURED_RUST, [],
      ['binaryfuse::BinaryFuse16::parse_section'],
      ['store-keys/rust-only'], ['binaryfuse/section-roundtrip'],
      'Rust-only, as above.'),
    R('binaryfuse.BinaryFuse16', TYPE, [], ['binaryfuse::BinaryFuse16']),
    R('binaryfuse constants', CONST, [],
      ['binaryfuse::MAX_ITERATIONS', 'binaryfuse::SECTION_HEADER_SIZE',
       'binaryfuse::SECTION_TYPE_BINARY_FUSE16']),
    R('binaryfuse errors', ERROR, [],
      ['binaryfuse::Error', 'binaryfuse::Error::is_corrupt']),

    # ----------------------------------------------------------- chunkers
    R('chunkers.split_bytes', MEASURED, ['chunkers.SplitBytes'], ['chunkers::split_bytes'],
      ['compressible/4-16-64KiB', 'compressible/default-sizes', 'random/default-sizes',
       'tiny-64KiB/default-sizes'],
      ['chunkers/split-random', 'chunkers/split-compressible',
       'chunkers/split-bounds-random', 'chunkers/split-bounds-compressible',
       'chunkers/split-empty', 'chunkers/split-propagates-error'],
      'The UltraCDC byte chunker. Chunk boundaries determine every file key, '
      'so the checks compare the full chunk list across cores, not just the '
      'count. A non-default size configuration is measured too.'),
    R('chunkers.new_item_chunker', MEASURED,
      ['chunkers.NewItemChunker'], ['chunkers::ItemChunker::new'],
      ['bits-4..12'], ['chunkers/item-bounds'],
      'A constructor, so it is batched over nine bit widths rather than '
      'timed once.'),
    R('chunkers.item_chunker', MEASURED,
      ['chunkers.ItemChunker.IsBoundary'], ['chunkers::ItemChunker::is_boundary'],
      ['is_boundary/bits-7'], ['chunkers/item-boundaries'], ''),
    R('chunkers option and chunker types', TYPE,
      ['chunkers.ByteOpts', 'chunkers.ItemChunker'],
      ['chunkers::ByteOpts', 'chunkers::ItemChunker'],
      note='Go re-exports the upstream `ChunkerOpts`; Rust defines an '
           'equivalent struct with the same fields.'),
    R('chunkers default sizes', CONST,
      ['chunkers.DefaultMinSize', 'chunkers.DefaultNormalSize', 'chunkers.DefaultMaxSize'],
      ['chunkers::DEFAULT_MIN_SIZE', 'chunkers::DEFAULT_NORMAL_SIZE',
       'chunkers::DEFAULT_MAX_SIZE'],
      checks=['chunkers/split-bounds-random'],
      note='The bound checks assert every chunk respects them.'),
    R('chunkers errors', ERROR, [], ['chunkers::OptionsError', 'chunkers::SplitError'],
      checks=['chunkers/split-propagates-error'],
      note='Rust names the option and callback failures; Go returns the '
           'upstream error and the callback error unchanged. The '
           'propagation check covers both spellings.'),

    # ------------------------------------------------------------- fstree
    R('fstree.encode_blob', MEASURED, ['fstree.EncodeBlob'], ['fstree::encode_blob'],
      ['1MiB-duplicate', '1MiB-random', '1MiB-text', '4KiB-duplicate', '4KiB-random',
       '4KiB-text', 'empty', 'large-8MiB-duplicate', 'large-8MiB-random',
       'large-8MiB-text', 'tiny-64B-duplicate', 'tiny-64B-random', 'tiny-64B-text'], ['fstree/encode-blob'], ''),
    R('fstree.encode_dir_leaf', MEASURED, ['fstree.EncodeDirLeaf'], ['fstree::encode_dir_leaf'],
      ['entries-1024', 'entries-128', 'entries-128-partial-change',
       'entries-128-with-xattrs', 'entries-8'],
      ['fstree/encode-dir-leaf-8', 'fstree/encode-dir-leaf-128'],
      'One workload carries inline extended attributes, which is the '
      'expensive branch of the entry encoder.'),
    R('fstree.encode_dir_node', MEASURED, ['fstree.EncodeDirNode'], ['fstree::encode_dir_node'],
      ['pairs-1024', 'pairs-128', 'pairs-8'],
      ['fstree/encode-dir-node-8', 'fstree/encode-dir-node-128'], ''),
    R('fstree.encode_file_node', MEASURED, ['fstree.EncodeFileNode'], ['fstree::encode_file_node'],
      ['children-1024', 'children-128', 'children-65536', 'children-8'],
      ['fstree/encode-file-node-8', 'fstree/encode-file-node-1024'], ''),
    R('fstree.encode_xattr_set', MEASURED, ['fstree.EncodeXattrSet'], ['fstree::encode_xattr_set'],
      ['xattrs-64'], ['fstree/encode-xattr-set-64'], ''),
    R('fstree.decode_dir_leaf', MEASURED, ['fstree.DecodeDirLeaf'], ['fstree::decode_dir_leaf'],
      ['entries-1024', 'entries-128', 'entries-128-partial-change',
       'entries-128-with-xattrs', 'entries-8'],
      ['fstree/decode-dir-leaf', 'fstree/decode-rejects-truncated'], ''),
    R('fstree.decode_dir_node', MEASURED, ['fstree.DecodeDirNode'], ['fstree::decode_dir_node'],
      ['pairs-1024', 'pairs-128', 'pairs-8'], ['fstree/decode-dir-node'], ''),
    R('fstree.decode_file_node', MEASURED, ['fstree.DecodeFileNode'], ['fstree::decode_file_node'],
      ['children-1024', 'children-128', 'children-65536', 'children-8'], ['fstree/decode-file-node'], ''),
    R('fstree.child_keys', MEASURED, ['fstree.ChildKeys'], ['fstree::child_keys'],
      ['dir-leaf-128', 'dir-node-128', 'file-node-1024'],
      ['fstree/child-keys-dir-leaf', 'fstree/child-keys-blob-is-leaf'],
      'All three interior object types plus the leaf case.'),
    R('fstree.dir_builder', MEASURED,
      ['fstree.NewDirBuilder', 'fstree.DirBuilder.AddEntry', 'fstree.DirBuilder.Finish'],
      ['fstree::DirBuilder::new', 'fstree::DirBuilder::add_entry',
       'fstree::DirBuilder::finish'],
      ['entries-16', 'entries-256', 'entries-40000', 'entries-4096'],
      ['fstree/dir-builder-root'],
      'The whole streaming build of one directory: chunk entries into leaves '
      'and promote their keys through the index. The root key is the '
      'cross-core statement.'),
    R('fstree.index_builder_file', MEASURED,
      ['fstree.NewFileIndexBuilder', 'fstree.IndexBuilder.AddChild',
       'fstree.IndexBuilder.Finish'],
      ['fstree::IndexBuilder::new_file', 'fstree::IndexBuilder::add_child',
       'fstree::IndexBuilder::finish'],
      ['children-1024', 'children-128', 'children-65536', 'children-8'], ['fstree/file-index-root'],
      'Neither core exports the directory index builder: directories go '
      'through `DirBuilder`, which is measured above.'),
    R('fstree.lookup_entry', MEASURED, ['fstree.LookupEntry'], ['fstree::lookup_entry'],
      ['width-16/hit', 'width-16/miss', 'width-256/hit', 'width-256/miss',
       'width-40000/hit', 'width-40000/miss', 'width-4096/hit', 'width-4096/miss'],
      ['fstree/lookup-hit', 'fstree/lookup-miss'],
      'Hit and miss, over a wide prolly tree and a single-leaf one.'),
    R('fstree.list_entries', MEASURED, ['fstree.ListEntries'], ['fstree::list_entries'],
      ['widest/first-page-100', 'width-16/full-paging-100', 'width-256/full-paging-100',
       'width-40000/full-paging-100', 'width-4096/full-paging-100'], ['fstree/list-paging'],
      'One page, and paging the whole directory; the check asserts every '
      'entry appears exactly once in name order.'),
    R('fstree.collect_entries', MEASURED, ['fstree.CollectEntries'], ['fstree::collect_entries'],
      ['width-16', 'width-256', 'width-40000', 'width-4096'], ['fstree/collect-entries'], ''),
    R('fstree.resolve_path', MEASURED, ['fstree.ResolvePath'], ['fstree::resolve_path'],
      ['depth-1', 'depth-12', 'depth-24', 'depth-4'],
      ['fstree/resolve-path', 'fstree/resolve-path-missing',
       'fstree/resolve-path-rejects-dotdot'], ''),
    R('fstree.resolve_entry', MEASURED, ['fstree.ResolveEntry'], ['fstree::resolve_entry'],
      ['depth-1', 'depth-12', 'depth-24', 'depth-4'], ['fstree/resolve-entry-root-is-nil'], ''),
    R('fstree.write_content', MEASURED, ['fstree.WriteContent'], ['fstree::write_content'],
      ['file-corpus'], ['fstree/write-content'],
      'Reassembles a multi-level file; the check compares the bytes with the '
      'source corpus.'),
    R('fstree.reachable_keys', MEASURED, ['fstree.ReachableKeys'], ['fstree::reachable_keys'],
      ['deep', 'file-corpus', 'width-16', 'width-256', 'width-40000', 'width-4096'],
      ['fstree/reachable-root-first', 'fstree/reachable-distinct'],
      'Both cores pick their own walk parallelism here, so the case is '
      'marked `auto` and both run under one CPU set.'),
    R('fstree.check_complete', MEASURED, ['fstree.CheckComplete'], ['fstree::check_complete'],
      ['incomplete/jobs-1', 'widest/jobs-1', 'widest/jobs-N', 'width-16/jobs-1',
       'width-256/jobs-1', 'width-40000/jobs-1', 'width-4096/jobs-1'],
      ['fstree/check-complete', 'fstree/check-complete-missing'],
      'Single-threaded and concurrent, plus the failure path with one leaf '
      'removed from the store.'),
    R('fstree object and entry types', TYPE,
      ['fstree.Object', 'fstree.Entry', 'fstree.DirPair', 'fstree.DirBuilder',
       'fstree.IndexBuilder', 'fstree.Emit'],
      ['fstree::Object', 'fstree::Entry', 'fstree::DirPair', 'fstree::DirBuilder',
       'fstree::IndexBuilder'],
      note='Go names the emit callback as an exported function type; Rust '
           'takes a generic `FnMut`, which is not a symbol.'),
    R('fstree errors', ERROR,
      ['fstree.ErrNotFound', 'fstree.ErrNotDir', 'fstree.MissingObjectError',
       'fstree.MissingObjectError.Error'],
      ['fstree::Error', 'fstree::WalkError', 'fstree::WalkError::is_not_found',
       'fstree::WalkError::is_not_dir', 'fstree::WalkError::missing_object',
       'fstree::MissingObjectError', 'fstree::ChildKeysError', 'fstree::BuildError',
       'fstree::CborError', 'fstree::CborType', 'fstree::CborType::name',
       'fstree::Error::<Display>', 'fstree::Error::<Error>',
       'fstree::WalkError::<Display>', 'fstree::WalkError::<Error>',
       'fstree::MissingObjectError::<Display>', 'fstree::MissingObjectError::<Error>',
       'fstree::ChildKeysError::<Display>', 'fstree::ChildKeysError::<Error>',
       'fstree::BuildError::<Display>', 'fstree::BuildError::<Error>',
       'fstree::CborError::<Display>', 'fstree::CborError::<Error>',
       'fstree::CborType::<Display>'],
      checks=['fstree/lookup-miss', 'fstree/resolve-path-missing',
              'fstree/check-complete-missing', 'fstree/decode-rejects-truncated'],
      note='Go wraps sentinels; Rust names one variant per wrap site and '
           'offers `is_*` classifiers. `CborType` is Rust\'s diagnostic for '
           'decoder errors, which Go gets from its CBOR dependency. The '
           'failure-path checks assert the same classification on both sides.'),

    # -------------------------------------------------------- amberignore
    R('amberignore.root', MEASURED, ['amberignore.Root'], ['amberignore::Matcher::root'],
      ['load-root'], ['amberignore/root-patterns'], ''),
    R('amberignore.descend', MEASURED,
      ['amberignore.Matcher.Descend'],
      ['amberignore::Matcher::descend', 'amberignore::descend_opt'],
      ['8-subdirs'], ['amberignore/descend-composes'],
      'Rust\'s `descend_opt` is the spelling of Go\'s nil-receiver call.'),
    R('amberignore.ignored', MEASURED,
      ['amberignore.Matcher.Ignored'],
      ['amberignore::Matcher::ignored', 'amberignore::ignored_opt'],
      ['mixed-names', 'nil-matcher'],
      ['amberignore/root-patterns', 'amberignore/nil-matcher-ignores-nothing'],
      'The nil-matcher workload measures Go\'s nil receiver against Rust\'s '
      '`ignored_opt(None, ..)`.'),
    R('amberignore.Matcher', TYPE, ['amberignore.Matcher'], ['amberignore::Matcher']),
    R('amberignore.FileName', CONST, ['amberignore.FileName'], ['amberignore::FILE_NAME'],
      checks=['amberignore/root-patterns'],
      note='The check asserts the ignore file is never itself ignored.'),

    # ------------------------------------------------------------- ingest
    R('ingest.scan', MEASURED, ['ingest.Scan'], ['ingest::scan'],
      ['filtered/jobs-1', 'filtered/jobs-N', 'unfiltered/jobs-1'],
      ['ingest/scan-filtered', 'ingest/scan-unfiltered', 'ingest/scan-jobs-invariant'],
      'With and without ignore filtering, single-threaded and concurrent.'),
    R('ingest.objects', MEASURED, ['ingest.Objects'], ['ingest::objects'],
      ['tree/jobs-1', 'tree/jobs-N'],
      ['ingest/objects-root', 'ingest/objects-jobs-invariant',
       'ingest/objects-complete', 'ingest/objects-resolves-file',
       'ingest/objects-honours-ignore', 'ingest/objects-honours-negation'],
      'The root key of the ingested fixture tree is identical in both cores, '
      'which is the deepest single equality this benchmark asserts.'),
    R('ingest.dir', MEASURED, ['ingest.Dir'], ['ingest::dir'],
      ['single-file/jobs-1', 'tree/fresh-store/jobs-1', 'tree/fresh-store/jobs-N',
       'tree/incremental-change/jobs-1', 'tree/incremental-change/jobs-N',
       'tree/unchanged-repeat/jobs-1', 'tree/unchanged-repeat/jobs-N'],
      ['ingest/dir-root', 'ingest/dir-dedups', 'ingest/dir-incremental'],
      'Fresh store, a re-ingest after a deterministic incremental change, '
      'and a single regular file.'),
    R('ingest option types', TYPE,
      ['ingest.Opts', 'ingest.ChunkOpts', 'ingest.Progress'],
      ['ingest::Opts', 'ingest::Opts::<Debug>', 'ingest::ChunkOpts', 'ingest::Progress',
       'ingest::ObjectStream', 'ingest::ObjectStream::<Iterator>', 'ingest::Root',
       'ingest::Root::get', 'ingest::ChanSink::<Emit>', 'ingest::MapSink::<Emit>'],
      note='Rust returns the object stream and the deferred root as named '
           'types; Go returns an iterator and a `*key.Key`. `Root::get` is '
           'the read of that deferred value, exercised by every '
           '`ingest.objects` case.'),
    R('ingest defaults', CONST,
      ['ingest.DefaultItemBits', 'ingest.DefaultXattrInlineMax'],
      ['ingest::DEFAULT_ITEM_BITS', 'ingest::DEFAULT_XATTR_INLINE_MAX'],
      note='Both drivers build their synthetic trees with the default item '
           'width, so the builder cases match the ingest path.'),
    R('ingest errors', ERROR, [], ['ingest::Error'],
      note='Rust names the build failures; Go returns `*fs.PathError` and the '
           'encoder errors unchanged.'),

    # ---------------------------------------------------------- amberpack
    R('amberpack.encode_record', MEASURED, ['amberpack.EncodeRecord'], ['amberpack::encode_record'],
      ['1MiB-duplicate', '1MiB-random', '1MiB-text', '4KiB-duplicate', '4KiB-random',
       '4KiB-text', 'empty', 'large-8MiB-duplicate', 'large-8MiB-random',
       'large-8MiB-text', 'tiny-64B-duplicate', 'tiny-64B-random', 'tiny-64B-text'],
      ['amberpack/record-roundtrip', 'amberpack/compresses-only-when-smaller'],
      'Compressible and random payloads at three sizes: the compressor is '
      'the whole cost, and the two cores use different zstd implementations.'),
    R('amberpack.parse_record', MEASURED, ['amberpack.ParseRecord'], ['amberpack::parse_record'],
      ['corrupt-crc/producer-go', 'corrupt-crc/producer-rust', 'mixed-records/producer-go',
       'mixed-records/producer-rust'],
      ['amberpack/detects-crc-corruption', 'amberpack/rejects-non-canonical-key'],
      'Validation of a good record and of a record with a flipped payload '
      'byte.'),
    R('amberpack.decode_payload', MEASURED, ['amberpack.DecodePayload'], ['amberpack::decode_payload'],
      ['mixed-records/producer-go', 'mixed-records/producer-rust'], ['amberpack/record-roundtrip'], ''),
    R('amberpack.writer_add', MEASURED,
      ['amberpack.NewWriter', 'amberpack.Writer.Add', 'amberpack.Writer.Close'],
      ['amberpack::Writer::new', 'amberpack::Writer::add', 'amberpack::Writer::finish'],
      ['mixed-objects'], ['amberpack/empty-pack'], ''),
    R('amberpack.writer_add_record', MEASURED,
      ['amberpack.Writer.AddRecord'], ['amberpack::Writer::add_record'],
      ['pre-encoded-records/producer-go', 'pre-encoded-records/producer-rust'], ['amberpack/record-passthrough'],
      'The zero-copy push path: pre-encoded records appended verbatim.'),
    R('amberpack.reader_all', MEASURED,
      ['amberpack.NewReader', 'amberpack.Reader.All'],
      ['amberpack::Reader::new', 'amberpack::Reader::<Iterator>'],
      ['mixed-objects/producer-go', 'mixed-objects/producer-rust',
       'truncated-stream/producer-go', 'truncated-stream/producer-rust'],
      ['amberpack/reader-roundtrip', 'amberpack/rejects-truncated',
       'amberpack/rejects-legacy-magic'],
      'Rust spells `All` as the `Reader`\'s own `Iterator` impl.'),
    R('amberpack.reader_records', MEASURED,
      ['amberpack.Reader.Records'], ['amberpack::Reader::records'],
      ['mixed-objects/producer-go', 'mixed-objects/producer-rust'], ['amberpack/record-passthrough'], ''),
    R('amberpack record and stream types', TYPE,
      ['amberpack.Reader', 'amberpack.Writer', 'amberpack.Record', 'amberpack.RawRecord'],
      ['amberpack::Reader', 'amberpack::Writer', 'amberpack::Record',
       'amberpack::RawRecord', 'amberpack::Records', 'amberpack::Records::<Iterator>'],
      note='`Records` is the Rust iterator `Reader::records` returns; Go '
           'returns an `iter.Seq2`.'),
    R('amberpack constants', CONST,
      ['amberpack.RecHeaderSize', 'amberpack.MaxPayload'],
      ['amberpack::REC_HEADER_SIZE', 'amberpack::MAX_PAYLOAD'],
      note='Both drivers slice record payloads with the header size, so a '
           'disagreement would fail the round-trip check.'),
    R('amberpack errors', ERROR,
      ['amberpack.ErrCorrupt', 'amberpack.ErrMalformed'],
      ['amberpack::Error', 'amberpack::Error::is_corrupt', 'amberpack::Error::is_malformed'],
      checks=['amberpack/detects-crc-corruption', 'amberpack/rejects-truncated']),

    # ---------------------------------------------------------- packstore
    R('packstore.open', MEASURED, ['packstore.Open'],
      ['packstore::Store::open', 'packstore::Store::open_with'],
      ['empty', 'populated-reopen'], ['packstore/scan-index-matches-footer'],
      'Reopening a populated store is the recovery path: sealed segments are '
      'mmapped and validated and the active segment is tail-scanned.'),
    R('packstore.close', MEASURED, ['packstore.Store.Close'], ['packstore::Store::close'],
      ['populated'], ['packstore/closed-store-errors'], ''),
    R('packstore.put', MEASURED, ['packstore.Store.Put'], ['packstore::Store::put'],
      ['1MiB-duplicate/sync-off', '1MiB-random/sync-off', '1MiB-text/sync-off',
       '4KiB-duplicate/sync-off', '4KiB-random/already-present',
       '4KiB-random/sync-off', '4KiB-random/sync-on', '4KiB-text/sync-off',
       'empty/sync-off', 'large-8MiB-duplicate/sync-off',
       'large-8MiB-random/sync-off', 'large-8MiB-text/sync-off',
       'tiny-64B-duplicate/sync-off', 'tiny-64B-random/sync-off',
       'tiny-64B-text/sync-off'],
      ['packstore/get-content-addressed'],
      'Both durability boundaries and both object sizes, plus the dedup-hit '
      'path.'),
    R('packstore.write_batch', MEASURED, ['packstore.Store.WriteBatch'],
      ['packstore::Store::write_batch'], ['mixed-objects'],
      ['packstore/write-batch-stores-all'], ''),
    R('packstore.write_parallel', MEASURED, ['packstore.Store.WriteParallel'],
      ['packstore::Store::write_parallel'],
      ['duplicate-stream/writers-N', 'mixed-objects/writers-1/verify-false',
       'mixed-objects/writers-1/verify-true', 'mixed-objects/writers-N/verify-false',
       'mixed-objects/writers-N/verify-true'],
      ['packstore/verify-rejects-mismatch', 'packstore/dedups-within-batch'],
      'One and many writers, with and without per-object verification, plus '
      'a fully duplicate stream.'),
    R('packstore.append_record', MEASURED,
      ['packstore.Store.AppendRecord', 'packstore.Store.Sync'],
      ['packstore::Store::append_record', 'packstore::Store::sync'],
      ['pre-encoded+sync'],
      ['packstore/append-record-roundtrip', 'packstore/append-record-rejects-nil',
       'packstore/append-record-scrubs-clean'],
      'A batch of pre-encoded records followed by the one fsync that makes '
      'them durable; the two operations only make sense together. The checks '
      'reopen the store after the sync, so they cover the fsync and not just '
      'the appends.'),
    R('packstore.get', MEASURED, ['packstore.Store.Get'], ['packstore::Store::get'],
      ['hit', 'miss'], ['packstore/get-content-addressed', 'packstore/get-miss'], ''),
    R('packstore.get_record', MEASURED, ['packstore.Store.GetRecord'],
      ['packstore::Store::get_record'], ['hit'], ['packstore/get-record'], ''),
    R('packstore.has', MEASURED, ['packstore.Store.Has'], ['packstore::Store::has'],
      ['hit', 'miss'], ['packstore/has'], ''),
    R('packstore.stored_size', MEASURED, ['packstore.Store.StoredSize'],
      ['packstore::Store::stored_size'], ['hit'], ['packstore/stored-size'], ''),
    R('packstore.missing', MEASURED, ['packstore.Store.Missing'],
      ['packstore::Store::missing'], ['half-present'], ['packstore/missing'],
      'Order and multiplicity are part of the contract and are checked.'),
    R('packstore.sort_by_location', MEASURED, ['packstore.Store.SortByLocation'],
      ['packstore::Store::sort_by_location'], ['scattered'],
      ['packstore/sort-is-permutation'], ''),
    R('packstore.segments', MEASURED, ['packstore.Store.Segments'],
      ['packstore::Store::segments'], ['list'],
      ['packstore/segments-are-ordered-and-nonempty',
       'packstore/segments-account-for-the-store'],
      'Which objects land in which sealed segment follows the compressed '
      'record sizes, so the per-segment counts are a within-core anchor; the '
      'shape of the list -- strictly increasing ids, no empty segment -- is '
      'the cross-core statement.'),
    R('packstore.scan_index', MEASURED, ['packstore.Store.ScanIndex'],
      ['packstore::Store::scan_index'], ['all-segments'],
      ['packstore/scan-index-matches-footer'], ''),
    R('packstore.record', MEASURED, ['packstore.Store.Record'],
      ['packstore::Store::record'], ['by-location'], ['packstore/record-by-location'], ''),
    R('packstore.has_outside', MEASURED, ['packstore.Store.HasOutside'],
      ['packstore::Store::has_outside'], ['sealed-segment'],
      ['packstore/has-outside-miss'], ''),
    R('packstore.verify', MEASURED, ['packstore.Store.Verify'],
      ['packstore::Store::verify'], ['full-scrub'],
      ['packstore/verify-clean', 'packstore/verify-detects-corruption',
       'packstore/verify-after-compact'],
      'The scrub is also the oracle for the repair test: it must report the '
      'injected corruption and must be clean afterwards.'),
    R('packstore.liveness', MEASURED, ['packstore.Store.Liveness'],
      ['packstore::Store::liveness'], ['tenth-live'],
      ['packstore/liveness-accounts-all'], ''),
    R('packstore.compact', MEASURED, ['packstore.Store.Compact'],
      ['packstore::Store::compact'], ['90-percent-dead', 'nothing-dead'],
      ['packstore/compact-retains-live', 'packstore/compact-reclaims',
       'packstore/verify-after-compact'],
      'Run on an isolated copy of a store whose sealed segments are 90 % '
      'dead, so there is real reclamation, and the retained objects are '
      'verified afterwards.'),
    R('packstore.remove', MEASURED, ['packstore.Store.Remove'],
      ['packstore::Store::remove'], ['one-sealed-segment'],
      ['packstore/remove-drops-only-that-segment'],
      'Destructive: measured on an isolated copy per repetition. The check '
      'asserts that exactly the removed segment\'s objects went, that the '
      'next segment\'s stayed, and that the store still scrubs clean.'),
    R('packstore.wipe', MEASURED, ['packstore.Store.Wipe'], ['packstore::Store::wipe'],
      ['populated'], ['packstore/wipe'],
      'Destructive: measured on an isolated copy per repetition.'),
    R('packstore.put_verified', MEASURED, ['packstore.Store.PutVerified'],
      ['packstore::Store::put_verified'], ['already-intact'],
      ['packstore/repairs-corruption'],
      'The timed workload is the no-repair-needed path. The repair itself is '
      'a correctness check: a payload byte is flipped in an isolated copy of '
      'a sealed segment, the scrub must see it, and the repair must restore '
      'the object. Nothing destructive ever touches a shared fixture.'),
    R('packstore.new_mark_set', MEASURED, ['packstore.Store.NewMarkSet'],
      ['packstore::Store::new_mark_set'], ['snapshot'], ['packstore/markset'], ''),
    R('packstore.mark_set_mark', MEASURED,
      ['packstore.MarkSet.Mark', 'packstore.MarkSet.Marked'],
      ['packstore::MarkSet::mark', 'packstore::MarkSet::marked'],
      ['all-keys'], ['packstore/markset'],
      '`Marked` is a counter read, batched into the marking case.'),
    R('packstore.mark_set_contains', MEASURED, ['packstore.MarkSet.Contains'],
      ['packstore::MarkSet::contains'], ['all-keys'], ['packstore/markset'], ''),
    R('packstore.barrier', MEASURED,
      ['packstore.Store.BeginBarrier', 'packstore.Store.ObserveKeys',
       'packstore.Store.AbortBarrier'],
      ['packstore::Store::begin_barrier', 'packstore::Store::observe_keys',
       'packstore::Store::abort_barrier'],
      ['begin+observe+abort'],
      ['packstore/barrier-observed-keys-survive-compact',
       'packstore/barrier-abort-protects-nothing'],
      'The grey-capture span is one operation in practice: opening it, '
      'observing a closure and discarding it. Its observable contract is what '
      'the next compaction retains, and that is what the checks assert.'),
    R('packstore.oldest_inflight_write', GROUPED,
      ['packstore.Store.OldestInflightWrite'], ['packstore::Store::oldest_inflight_write'],
      ['idle'], ['packstore/oldest-inflight-idle-is-none'],
      'A single guarded field read; batched 4096 times so the interval is '
      'measurable at all. The check pins the answer it gives on the idle '
      'store the case measures.'),
    R('packstore store options', GROUPED,
      ['packstore.WithSegmentSize', 'packstore.WithSync', 'packstore.Option'],
      ['packstore::Options', 'packstore::Options::<Default>', 'packstore::Options::new',
       'packstore::Options::segment_size', 'packstore::Options::sync'],
      ['packstore.open/*', 'packstore.put/sync-on', 'packstore.put/sync-off'],
      [],
      'Configuration, not work: Go uses functional options, Rust a builder. '
      'Every store case applies both of them, and the sync setting is a '
      'measured dimension of `packstore.put`.'),
    R('packstore data types', TYPE,
      ['packstore.Store', 'packstore.Object', 'packstore.MarkSet',
       'packstore.SegmentInfo', 'packstore.SegmentLiveness', 'packstore.WriteOpts',
       'packstore.WriteStats', 'packstore.CompactOpts', 'packstore.CompactStats'],
      ['packstore::Store', 'packstore::Store::<Debug>', 'packstore::Store::<Drop>',
       'packstore::Object', 'packstore::Object::<From>', 'packstore::MarkSet',
       'packstore::SegmentInfo', 'packstore::SegmentLiveness', 'packstore::WriteOpts',
       'packstore::WriteStats', 'packstore::CompactOpts', 'packstore::CompactOpts::<Debug>',
       'packstore::CompactOpts::<Default>', 'packstore::CompactStats',
       'packstore::SealedSegment::<Debug>', 'packstore::WriteToken::<Drop>']),
    R('packstore constants', CONST,
      ['packstore.DefaultBatchSize', 'packstore.DefaultSegmentSize'],
      ['packstore::DEFAULT_BATCH_SIZE', 'packstore::DEFAULT_SEGMENT_SIZE'],
      note='The profile overrides the segment size so the fixtures really '
           'have several sealed segments; the batch size is left at the '
           'default on both sides.'),
    R('packstore errors', ERROR,
      ['packstore.ErrNotFound', 'packstore.ErrClosed', 'packstore.ErrCorrupt',
       'packstore.ErrVerify', 'packstore.ErrUnknownSegment'],
      ['packstore::Error', 'packstore::Error::is_not_found', 'packstore::Error::is_closed',
       'packstore::Error::is_corrupt', 'packstore::Error::is_verify',
       'packstore::Error::is_unknown_segment'],
      checks=['packstore/get-miss', 'packstore/closed-store-errors',
              'packstore/verify-detects-corruption', 'packstore/verify-rejects-mismatch'],
      note='`ErrUnknownSegment` / `is_unknown_segment` is the only sentinel '
           'without its own check: reaching it needs a segment id that was '
           'never sealed, which the benchmark has no operation for.'),

    # ---------------------------------------------------------- reference
    R('reference.encode', MEASURED, ['reference.Reference.Encode'],
      ['reference::Reference::encode'], ['signed'],
      ['reference/encode', 'reference/roundtrip'], ''),
    R('reference.decode', MEASURED, ['reference.Decode'], ['reference::Reference::decode'],
      ['signed'], ['reference/roundtrip', 'reference/rejects-trailing'], ''),
    R('reference.signature_payload', MEASURED, ['reference.Reference.SignaturePayload'],
      ['reference::Reference::signature_payload'], ['signed'],
      ['reference/signature-payload', 'reference/signature-payload-excludes-sig'], ''),
    R('reference.validate_name', MEASURED, ['reference.ValidateName'],
      ['reference::validate_name'], ['valid'], ['reference/name-rules'], ''),
    R('reference.validate_user', MEASURED, ['reference.ValidateUser'],
      ['reference::validate_user'], ['valid'], ['reference/user-rules'], ''),
    R('reference.Reference', TYPE, ['reference.Reference'], ['reference::Reference']),
    R('reference limits', CONST,
      ['reference.MaxNameLen', 'reference.MaxUserLen', 'reference.MaxSignatureLen',
       'reference.MaxPublicKeyLen'],
      ['reference::MAX_NAME_LEN', 'reference::MAX_USER_LEN',
       'reference::MAX_SIGNATURE_LEN', 'reference::MAX_PUBLIC_KEY_LEN'],
      checks=['reference/name-rules']),
    R('reference errors', ERROR, [],
      ['reference::Error', 'reference::DecodeError', 'reference::NameError',
       'reference::UserError'],
      checks=['reference/name-rules', 'reference/user-rules'],
      note='Rust names the validation and decoding failures; Go returns '
           'formatted errors without exported sentinels.'),

    # ----------------------------------------------------------- refstore
    R('refstore.open', MEASURED, ['refstore.Open'], ['refstore::Store::open'],
      ['empty', 'populated-reopen'], ['refstore/reopen-sees-committed-records'],
      'Pebble in Go, redb in Rust: the same operation over different storage '
      'engines. See the scope limits in the report. The check writes, closes '
      'and reopens, which is what makes an open a usable store.'),
    R('refstore.close', MEASURED, ['refstore.Store.Close'], [],
      ['populated'], ['refstore/reopen-sees-committed-records'],
      'Go exports `Close`; the redb port has no close method and no Drop '
      'impl of its own -- the handle it owns closes when the value goes out '
      'of scope -- so the Rust case measures that drop and the row names no '
      'Rust symbol. The check reopens the store afterwards, which is what '
      'makes the drop a close rather than a leak.'),
    R('refstore.put', MEASURED, ['refstore.Store.Put'], ['refstore::Store::put'],
      ['records/sync-false', 'records/sync-true'],
      ['refstore/put-overwrites'],
      'Both durability settings.'),
    R('refstore.put_batch', MEASURED, ['refstore.Store.PutBatch'],
      ['refstore::Store::put_batch'], ['records/sync-false'],
      ['refstore/batch-last-wins'], ''),
    R('refstore.get', MEASURED, ['refstore.Store.Get'], ['refstore::Store::get'],
      ['hit', 'miss'], ['refstore/get-verbatim', 'refstore/get-miss'], ''),
    R('refstore.delete', MEASURED, ['refstore.Store.Delete'], ['refstore::Store::delete'],
      ['records'], ['refstore/delete', 'refstore/delete-absent'], ''),
    R('refstore.all', MEASURED, ['refstore.Store.All'], ['refstore::Store::all'],
      ['records'], ['refstore/all-lexicographic'], ''),
    R('refstore.wipe', MEASURED, ['refstore.Store.Wipe'], ['refstore::Store::wipe'],
      ['records'], ['refstore/wipe'], ''),
    R('refstore data types', TYPE, ['refstore.Store', 'refstore.Record'],
      ['refstore::Store', 'refstore::Store::<Debug>', 'refstore::Record']),
    R('refstore errors', ERROR, ['refstore.ErrNotFound'],
      ['refstore::Error', 'refstore::Error::is_not_found'],
      checks=['refstore/get-miss', 'refstore/delete-absent']),

    # -------------------------------------------------------------- inbox
    R('inbox.open', MEASURED, ['inbox.Open'], ['inbox::Inbox::open'],
      ['empty', 'sweeps-staged-tmp-files'], ['inbox/open-sweeps-staged-tmp-files'],
      'The second workload is the recovery path: staged-but-uncommitted '
      'files left by a previous run are swept at open, which is what the '
      'check asserts.'),
    R('inbox.stage', MEASURED, ['inbox.Inbox.Stage'], ['inbox::Inbox::stage'],
      ['packs'], ['inbox/stage-hashes-body'], ''),
    R('inbox.commit', MEASURED, ['inbox.Inbox.Commit'], ['inbox::Inbox::commit'],
      ['packs/duplicate'], ['inbox/commit-adds', 'inbox/commit-idempotent'],
      'The timed workload is the idempotent path, reached by retiring the '
      'workers first so a committed entry is still on disk. The first-commit '
      'path is timed as part of `inbox.drain`.'),
    R('inbox.discard', MEASURED, ['inbox.Inbox.Discard'], ['inbox::Inbox::discard'],
      ['packs'], ['inbox/discard-removes-tmp'], ''),
    R('inbox.drain', MEASURED,
      ['inbox.Inbox.WaitFor'],
      ['inbox::Inbox::wait_for'],
      ['packs/new'], ['inbox/drain-stores-objects', 'inbox/wait-for-empty-group'],
      'End to end: commit every staged pack (the `inbox.commit` symbols, '
      'inventoried on the row above) and then wait for the worker pool to '
      'store its objects. The wait cannot be timed alone, because the drain '
      'starts at the commit, so this row carries the wait and the composite.'),
    R('inbox.close', MEASURED, ['inbox.Inbox.Close'], ['inbox::Inbox::close'],
      ['drained'], ['inbox/close-drains-then-retires'],
      'Close retires the workers after the entries they were given have been '
      'drained; the check asserts that everything committed before it is in '
      'the store and that the directory is left with nothing to sweep.'),
    R('inbox.with_gate', CHECKED, ['inbox.Option', 'inbox.WithGate'], [],
      [], ['inbox/with-gate-brackets-writes'],
      'Go-only. The Go inbox takes functional options and exports `WithGate` '
      'to bracket each entry\'s store write with the collector\'s write gate; '
      'the Rust inbox has no options parameter and no equivalent export, so '
      'there is nothing to pair. It is not timed either -- timing an option '
      'constructor would measure a closure allocation -- so it is exercised '
      'by a check that drains a pack through a counting gate and asserts the '
      'gate was entered.'),
    R('inbox data types', TYPE, ['inbox.Inbox', 'inbox.Meta'],
      ['inbox::Inbox', 'inbox::Inbox::<Drop>', 'inbox::Meta', 'inbox::LogFn'],
      note='Rust names the log callback type; Go takes a `*slog.Logger` from '
           'its standard library.'),

    # ----------------------------------------------------------------- gc
    R('gc.open', MEASURED, ['gc.Open'], ['gc::Collector::open'],
      ['populated'], ['gc/open-yields-a-usable-collector'], ''),
    R('gc.close', MEASURED, ['gc.Collector.Close'], ['gc::Collector::close'], ['idle'], ['gc/close-then-reopen'], 'The check closes a collector and opens another over the same directories, and asks the new one why the live root is live: a close that discarded the reference graph would not be able to answer.'),
    R('gc.prepare_ref', MEASURED, ['gc.Collector.PrepareRef'],
      ['gc::Collector::prepare_ref', 'gc::PreparedRef', 'gc::PreparedRef::commit',
       'gc::PreparedRef::abort'],
      ['missing-root', 'tree-root/abort', 'tree-root/commit'],
      ['gc/prepare-accepts-complete', 'gc/prepare-rejects-incomplete'],
      'Commit, abort and the rejection of a root whose objects are not '
      'stored. Go returns two closures; Rust returns a `PreparedRef` guard.'),
    R('gc.release_ref', MEASURED, ['gc.Collector.ReleaseRef'],
      ['gc::Collector::release_ref'], ['batch'], ['gc/release-is-a-noop'],
      'A no-op kept for protocol symmetry in both cores; batched 4096 times '
      'so there is an interval to measure.'),
    R('gc.status', MEASURED, ['gc.Collector.Status'], ['gc::Collector::status'],
      ['mark+score'], ['gc/status-marks-live', 'gc/status-sees-garbage'],
      'A full advisory mark plus per-pack scoring.'),
    R('gc.why', MEASURED, ['gc.Collector.Why'], ['gc::Collector::why'],
      ['live-root'], ['gc/why-names-the-reference', 'gc/why-unreferenced'], ''),
    R('gc.run', MEASURED, ['gc.Collector.Run'], ['gc::Collector::run'],
      ['nothing-to-reclaim', 'reclaimable-packs'],
      ['gc/run-reclaims', 'gc/run-marks-live', 'gc/run-retains-live',
       'gc/run-tree-still-complete', 'gc/store-scrubs-clean-after-run',
       'gc/reaches-a-fixed-point', 'gc/fixed-point-retains-live'],
      'A real cycle over a fixture with several whole reclaimable packs and '
      'one referenced tree. The checks assert the tree survives, still reads '
      'end to end, and that repeated cycles terminate.'),
    R('gc.wipe', MEASURED, ['gc.Collector.Wipe'], ['gc::Collector::wipe'],
      ['store-reset'], ['gc/wipe-resets'], ''),
    R('gc.begin_write', MEASURED_GO, ['gc.Collector.BeginWrite'], [],
      ['gate-span'], ['gc/begin-write-release-is-idempotent'],
      'Go-only. The Rust core keeps the equivalent gate internal: '
      '`packstore::Store::begin_write` is `pub(super)` and the collector '
      'exports no `BeginWrite`. Measured on the Go side and reported in the '
      'single-core table, never paired.'),
    R('gc data types', TYPE,
      ['gc.Collector', 'gc.Options', 'gc.CycleStats', 'gc.Status', 'gc.PackStatus'],
      ['gc::Collector', 'gc::Collector::<Drop>', 'gc::Options', 'gc::CycleStats',
       'gc::Status', 'gc::PackStatus']),
    R('gc defaults', CONST,
      ['gc.DefaultGrace', 'gc.DefaultGarbage'],
      ['gc::DEFAULT_GRACE', 'gc::DEFAULT_GARBAGE'],
      note='Both drivers pass the default garbage line explicitly and set a '
           'one-nanosecond grace, so freshly written fixture segments are '
           'eligible and the free-space policy never enters the measurement.'),
    R('gc errors', ERROR, ['gc.ErrCycleRunning'],
      ['gc::Error', 'gc::Error::is_cycle_running', 'gc::Error::is_canceled'],
      note='Overlapping and cancelled cycles. The benchmark never overlaps '
           'cycles — it runs them one at a time by construction — so these '
           'are inventoried rather than exercised.'),

    # ----------------------------------------------------------------- tar
    R('tarexport.write', MEASURED, ['tarexport.Write'], ['tarexport::write'],
      ['fixture-tree'], ['tar/export-bytes', 'tar/export-rejects-non-directory'],
      'PAX export is specified to be byte-identical across the cores, and '
      'the check compares the whole archive digest.'),
    R('tarextract.extract', MEASURED, ['tarextract.Extract'], ['tarextract::extract'],
      ['fixture-tree'],
      ['tar/extract-manifest', 'tar/extract-content-matches-source',
       'tar/extract-symlink', 'tar/extract-honours-ingest-filter'],
      'The extracted tree\'s canonical manifest — type, permissions, size, '
      'mtime, content digest, symlink target — is compared across cores.'),
    R('tar types', TYPE, ['tarexport.Getter'], ['tarexport::Error', 'tarexport::Error::<Display>', 'tarexport::Error::<Error>',
       'tarexport::GoQuote::<Display>', 'tarexport::TarWriter::<Write>',
       'tarextract::Error', 'tarextract::Error::<Display>', 'tarextract::Error::<Error>',
       'tarextract::TarReader::<Read>'],
      note='Go names the fetch callback type; Rust takes a generic closure '
           'and names the error types instead.'),
]


def load_symbols(path):
    with open(path) as fh:
        return [line.strip() for line in fh if line.strip()]


def measured_statuses():
    """The statuses whose rows name an operation the benchmark times."""
    return (MEASURED, MEASURED_GO, MEASURED_RUST, GROUPED)


def is_op(name):
    """Rows whose `op` is an operation id, as opposed to a descriptive label
    for a group of symbols ("key errors", "packstore types")."""
    return ' ' not in name and '.' in name


def expected_sets():
    """The manifest, as the report consumes it.

    `operations` maps each paired operation to the exact set of workloads it
    must have been measured at; `single_core` names the cases one core
    measures alone; `checks` is every check id the matrix cites as evidence.
    """
    operations, single = {}, {'go': [], 'rust': []}
    for row in ROWS:
        if not is_op(row['op']):
            continue
        if row['status'] in (MEASURED, GROUPED):
            operations[row['op']] = sorted(row['cases'])
        elif row['status'] == MEASURED_GO:
            single['go'].extend(sorted((row['op'], c) for c in row['cases']))
        elif row['status'] == MEASURED_RUST:
            single['rust'].extend(sorted((row['op'], c) for c in row['cases']))
    checks = sorted({c for row in ROWS for c in row['checks']})
    return {'operations': operations, 'single_core': single, 'checks': checks}


def check(go_path, rust_path, report_path=None):
    """Returns (problems, stats). An empty problem list means the matrix is total."""
    go_syms = set(load_symbols(go_path))
    rust_syms = set(load_symbols(rust_path))
    problems = []

    # Every symbol the matrix names must exist, exactly as the extractors
    # spell it. There is no prose exemption: a trait implementation is
    # extracted as `module::Type::<Trait>` and a re-export under the module
    # that re-exports it, precisely so that "it is an impl block" can no
    # longer hide an operation from this check.
    claimed_go, claimed_rust = {}, {}
    for row in ROWS:
        for s in row['go']:
            if s in claimed_go:
                problems.append(f'Go symbol {s} is claimed by two rows: '
                                f'{claimed_go[s]!r} and {row["op"]!r}')
            claimed_go[s] = row['op']
            if s not in go_syms:
                problems.append(f'row {row["op"]!r} names Go symbol {s}, which does not exist')
        for s in row['rust']:
            if s in claimed_rust:
                problems.append(f'Rust symbol {s} is claimed by two rows: '
                                f'{claimed_rust[s]!r} and {row["op"]!r}')
            claimed_rust[s] = row['op']
            if s not in rust_syms:
                problems.append(f'row {row["op"]!r} names Rust symbol {s}, which does not exist')

    for s in sorted(go_syms - set(claimed_go)):
        problems.append(f'Go symbol {s} is not covered by the matrix')
    for s in sorted(rust_syms - set(claimed_rust)):
        problems.append(f'Rust symbol {s} is not covered by the matrix')

    measured_ops = {r['op'] for r in ROWS if r['status'] == MEASURED and is_op(r['op'])}
    go_only = {r['op'] for r in ROWS if r['status'] == MEASURED_GO and is_op(r['op'])}
    rust_only = {r['op'] for r in ROWS if r['status'] == MEASURED_RUST and is_op(r['op'])}
    grouped = {r['op'] for r in ROWS if r['status'] == GROUPED and is_op(r['op'])}

    # A timed operation has to point at evidence that it did what it is
    # named after. Presence in the sample document is not evidence: it says
    # a function ran, not that it worked.
    for row in ROWS:
        if row['status'] in measured_statuses() and is_op(row['op']):
            if not row['checks']:
                problems.append(f'row {row["op"]!r} is timed but names no correctness check; '
                                f'a timing with no evidence measures an unknown operation')
            if not row['cases']:
                problems.append(f'row {row["op"]!r} is timed but names no workload')

    if report_path:
        with open(report_path) as fh:
            report = json.load(fh)
        want = expected_sets()

        # Exact cases, not merely present operations. A workload that
        # disappeared, or one that appeared without being written down, is a
        # change to what the report means and has to be declared.
        paired = {}
        for r in report['paired']:
            paired.setdefault(r['op'], set()).add(r['workload'])
        for op in sorted(set(want['operations']) - set(paired)):
            problems.append(f'the matrix calls {op!r} measured in both cores, '
                            f'but the report has no paired samples for it')
        for op in sorted(set(paired) - set(want['operations'])):
            problems.append(f'the report pairs {op!r}, which the matrix does not list as '
                            f'measured in both cores')
        for op in sorted(set(want['operations']) & set(paired)):
            expect, got = set(want['operations'][op]), paired[op]
            if expect != got:
                problems.append(
                    f'{op} was measured at workloads the matrix does not expect: '
                    f'missing={sorted(expect - got)} unexpected={sorted(got - expect)}')

        single = {}
        for r in report['unpaired']:
            single.setdefault(r['core'], set()).add((r['op'], r['workload']))
        for core in ('go', 'rust'):
            expect = set(want['single_core'][core])
            got = single.get(core, set())
            if expect != got:
                problems.append(
                    f'the {core}-only measured set is not the matrix\'s: '
                    f'missing={sorted(expect - got)} unexpected={sorted(got - expect)}')

        # Every check the matrix cites as evidence has to have been recorded.
        recorded = {c['id'] for c in report.get('checks', [])}
        if recorded:
            missing = sorted(set(want['checks']) - recorded)
            if missing:
                problems.append(f'the matrix cites checks the run did not record: {missing}')

    stats = {
        'go_symbols': len(go_syms),
        'rust_symbols': len(rust_syms),
        'rows': len(ROWS),
        'paired_operations': len(measured_ops),
        'go_only_operations': len(go_only),
        'rust_only_operations': len(rust_only),
        'grouped': len(grouped),
        'checked_only': len([r for r in ROWS if r['status'] == CHECKED]),
        'workloads': sum(len(r['cases']) for r in ROWS if is_op(r['op'])),
        'checks_cited': len({c for row in ROWS for c in row['checks']}),
        'types': len([r for r in ROWS if r['status'] == TYPE]),
        'constants': len([r for r in ROWS if r['status'] == CONST]),
        'errors': len([r for r in ROWS if r['status'] == ERROR]),
    }
    return problems, stats


def render(stats, go_path, rust_path):
    def cell(items):
        return '<br>'.join(f'`{s}`' for s in items) if items else '—'

    L = []
    add = L.append
    add('# Coverage matrix')
    add('')
    add('Every exported symbol of both cores, and what the core-operation')
    add('benchmark does with it. The lists on the two sides are extracted from the')
    add('pinned sources, not written by hand:')
    add('')
    add('```sh')
    add('./core-ops/coverage/refresh.sh       # regenerate the lists and this file')
    add('```')
    add('')
    add(f'* Exported Go symbols: **{stats["go_symbols"]}** '
        f'(`coverage/go-exports.txt`)')
    add(f'* Exported Rust symbols: **{stats["rust_symbols"]}** '
        f'(`coverage/rust-exports.txt`)')
    add(f'* Matrix rows: **{stats["rows"]}**, covering every one of them exactly once')
    add(f'* Paired operations measured in both cores: **{stats["paired_operations"]}**')
    add(f'* Operations one core exports and the other does not: '
        f'**{stats["go_only_operations"]} Go-only, {stats["rust_only_operations"]} Rust-only**')
    add(f'* Grouped accessor/configuration rows: **{stats["grouped"]}**')
    add(f'* Data types: **{stats["types"]}**; constants: **{stats["constants"]}**; '
        f'error values: **{stats["errors"]}**')
    add('')
    add('`core-ops/coverage/matrix.py check` proves the mapping is total: every')
    add('extracted symbol appears in exactly one row, no row names a symbol that')
    add('does not exist, and — given a report — every operation this file calls')
    add('measured really produced samples. CI runs it.')
    add('')
    add('## How to read the status column')
    add('')
    add('| Status | Meaning |')
    add('|---|---|')
    add(f'| `{MEASURED}` | Timed in both cores and compared per workload. |')
    add(f'| `{MEASURED_GO}` | Exported by the Go core only. Timed on that side, '
        'reported separately, never compared. |')
    add(f'| `{MEASURED_RUST}` | Exported by the Rust core only. Timed on that side, '
        'reported separately, never compared. |')
    add(f'| `{GROUPED}` | An accessor or a configuration call, folded into a batched '
        'case because timing it alone would measure the loop. |')
    add(f'| `{CHECKED}` | Exercised by a correctness check without a timing of its own. |')
    add(f'| `{TYPE}` | A data type. Constructing and reading it is part of the '
        'operations that use it. |')
    add(f'| `{CONST}` | A constant. |')
    add(f'| `{ERROR}` | An error value, type or classifier. The failure-path checks '
        'assert the classification; there is nothing to time. |')
    add('')

    order = [MEASURED, MEASURED_GO, MEASURED_RUST, GROUPED, CHECKED, TYPE, CONST, ERROR]
    modules = []
    for row in ROWS:
        mod = row['op'].split('.')[0].split(' ')[0]
        if mod not in modules:
            modules.append(mod)

    add('## The matrix')
    add('')
    for mod in modules:
        rows = [r for r in ROWS if r['op'].split('.')[0].split(' ')[0] == mod]
        add(f'### `{mod}`')
        add('')
        add('| Operation | Status | Go | Rust | Workloads | Checks | Notes |')
        add('|---|---|---|---|---|---|---|')
        for r in sorted(rows, key=lambda r: (order.index(r['status']), r['op'])):
            add('| `{op}` | {st} | {go} | {rs} | {wl} | {ck} | {note} |'.format(
                op=r['op'], st=r['status'], go=cell(r['go']), rs=cell(r['rust']),
                wl=cell(r['cases']), ck=cell(r['checks']),
                note=r['note'].replace('|', r'\|')))
        add('')
    return '\n'.join(L) + '\n'


def sync(go_path, rust_path, source=None):
    """Rewrites every row's workload list from two raw sample documents.

    The workload lists are the manifest the report is checked against, so
    they have to be exact. Keeping them exact by hand across a hundred rows
    would guarantee they drift, so they are generated from a real run and
    committed; `check --report` then fails if a later run disagrees with
    what was committed.
    """
    import ast

    source = source or os.path.abspath(__file__)
    go = json.load(open(go_path))
    rust = json.load(open(rust_path))
    gcases, rcases = {}, {}
    for doc, into in ((go, gcases), (rust, rcases)):
        for smp in doc['samples']:
            into.setdefault(smp['op'], set()).add(smp['workload'])
    paired = {op: sorted(gcases[op] & rcases[op])
              for op in set(gcases) & set(rcases) if gcases[op] & rcases[op]}
    go_only = {op: sorted(w) for op, w in gcases.items() if op not in rcases}
    rust_only = {op: sorted(w) for op, w in rcases.items() if op not in gcases}

    text = open(source).read()
    tree = ast.parse(text)
    lines = text.splitlines(keepends=True)
    offsets = [0]
    for line in lines:
        offsets.append(offsets[-1] + len(line))

    def span(node):
        return (offsets[node.lineno - 1] + node.col_offset,
                offsets[node.end_lineno - 1] + node.end_col_offset)

    edits = []
    for node in ast.walk(tree):
        if not (isinstance(node, ast.Call) and getattr(node.func, 'id', None) == 'R'):
            continue
        if len(node.args) < 5:
            continue
        op = node.args[0].value
        status = getattr(node.args[1], 'id', None)
        want = None
        if status == 'MEASURED' or status == 'GROUPED':
            want = paired.get(op)
        elif status == 'MEASURED_GO':
            want = go_only.get(op)
        elif status == 'MEASURED_RUST':
            want = rust_only.get(op)
        if want is None:
            continue
        start, end = span(node.args[4])
        indent = ' ' * (node.args[4].col_offset)
        rendered = _render_list(want, node.args[4].col_offset)
        if text[start:end] != rendered:
            edits.append((start, end, rendered))
    for start, end, rendered in sorted(edits, reverse=True):
        text = text[:start] + rendered + text[end:]
    with open(source, 'w') as fh:
        fh.write(text)
    return len(edits)


def _render_list(items, col):
    """A source-shaped list literal that fits the file's 92-column style."""
    if not items:
        return '[]'
    one = '[' + ', '.join(repr(i) for i in items) + ']'
    if col + len(one) <= 92:
        return one
    out, line = [], '['
    pad = ' ' * (col + 1)
    for i, item in enumerate(items):
        piece = repr(item) + (',' if i < len(items) - 1 else ']')
        if len(line) + len(piece) + 1 > 92 - col and line.strip() not in ('[',):
            out.append(line)
            line = pad + piece
        else:
            line = line + (' ' if line not in ('[',) and not line.endswith('[') else '') + piece
    out.append(line)
    return '\n'.join(out)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('command', choices=['check', 'render', 'sync'])
    here = os.path.dirname(os.path.abspath(__file__))
    ap.add_argument('--go', default=os.path.join(here, 'go-exports.txt'))
    ap.add_argument('--rust', default=os.path.join(here, 'rust-exports.txt'))
    ap.add_argument('--samples-go', default=None,
                    help='a raw Go sample document, for `sync`')
    ap.add_argument('--samples-rust', default=None,
                    help='a raw Rust sample document, for `sync`')
    ap.add_argument('--report', default=None,
                    help='a core-ops report.json, to prove the measured rows were measured')
    ap.add_argument('--out', default=os.path.join(here, '..', 'COVERAGE.md'))
    args = ap.parse_args()

    if args.command == 'sync':
        if not (args.samples_go and args.samples_rust):
            print('coverage: sync needs --samples-go and --samples-rust', file=sys.stderr)
            sys.exit(2)
        n = sync(args.samples_go, args.samples_rust)
        print(f'coverage: rewrote {n} workload list(s) in matrix.py')
        return

    problems, stats = check(args.go, args.rust, args.report)
    if problems:
        for p in problems:
            print(f'coverage: {p}', file=sys.stderr)
        print(f'coverage: {len(problems)} problem(s)', file=sys.stderr)
        sys.exit(1)

    if args.command == 'render':
        with open(args.out, 'w') as fh:
            fh.write(render(stats, args.go, args.rust))
        print(f'coverage: wrote {args.out}')
    print('coverage: ' + ', '.join(f'{k}={v}' for k, v in stats.items()))


if __name__ == '__main__':
    main()
