package main

import (
	"io"
	"syscall"

	"github.com/amber-store/core/cborx"
	"github.com/amber-store/core/key"
	"github.com/amber-store/core/tarexport"
)

// encodeXattrs names the Go core's canonical byte-string-keyed CBOR map
// encoder. The Rust core spells the same operation `cbor::encode_xattrs`; the
// coverage matrix pairs them under `cbor.encode_xattrs`.
func encodeXattrs(m map[string][]byte) []byte { return cborx.EncodeXattrs(m) }

func decodeXattrs(b []byte) (map[string][]byte, error) { return cborx.DecodeXattrs(b) }

func tarWrite(w io.Writer, root key.Key, get func(key.Key) ([]byte, error)) error {
	return tarexport.Write(w, root, tarexport.Getter(get))
}

func setXattr(path, name string, value []byte) error {
	return syscall.Setxattr(path, name, value, 0)
}
