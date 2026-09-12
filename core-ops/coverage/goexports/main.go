// Command goexports lists the exported API of every public package in a Go
// core checkout, one symbol per line, so the coverage matrix can be checked
// against the surface it claims to cover rather than against memory.
//
// Output lines are `package.Symbol`, `package.Type.Method` and
// `package.Type` in sorted order. Internal packages, commands and test files
// are skipped: they are not API.
package main

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: goexports <core-source-dir>")
		os.Exit(2)
	}
	root := os.Args[1]
	entries, err := os.ReadDir(root)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	var out []string
	for _, e := range entries {
		if !e.IsDir() || strings.HasPrefix(e.Name(), ".") {
			continue
		}
		switch e.Name() {
		case "cmd", "docs", "specs", "architecture", "internal", "amberignore-testdata":
			if e.Name() != "amberignore" {
				continue
			}
		}
		if e.Name() == "cmd" || e.Name() == "docs" || e.Name() == "specs" ||
			e.Name() == "architecture" || e.Name() == "internal" {
			continue
		}
		syms, err := packageExports(filepath.Join(root, e.Name()))
		if err != nil {
			fmt.Fprintf(os.Stderr, "%s: %v\n", e.Name(), err)
			os.Exit(1)
		}
		out = append(out, syms...)
	}
	sort.Strings(out)
	for _, s := range out {
		fmt.Println(s)
	}
}

func packageExports(dir string) ([]string, error) {
	fset := token.NewFileSet()
	pkgs, err := parser.ParseDir(fset, dir, func(fi os.FileInfo) bool {
		return !strings.HasSuffix(fi.Name(), "_test.go")
	}, 0)
	if err != nil {
		return nil, err
	}
	var out []string
	for name, pkg := range pkgs {
		if strings.HasSuffix(name, "_test") {
			continue
		}
		for _, f := range pkg.Files {
			for _, d := range f.Decls {
				switch decl := d.(type) {
				case *ast.FuncDecl:
					if !decl.Name.IsExported() {
						continue
					}
					if decl.Recv == nil {
						out = append(out, fmt.Sprintf("%s.%s", name, decl.Name.Name))
						continue
					}
					recv := receiverName(decl.Recv)
					if recv == "" || !ast.IsExported(recv) {
						continue
					}
					out = append(out, fmt.Sprintf("%s.%s.%s", name, recv, decl.Name.Name))
				case *ast.GenDecl:
					for _, spec := range decl.Specs {
						switch s := spec.(type) {
						case *ast.TypeSpec:
							if s.Name.IsExported() {
								out = append(out, fmt.Sprintf("%s.%s", name, s.Name.Name))
							}
						case *ast.ValueSpec:
							for _, id := range s.Names {
								if id.IsExported() {
									out = append(out, fmt.Sprintf("%s.%s", name, id.Name))
								}
							}
						}
					}
				}
			}
		}
	}
	return out, nil
}

func receiverName(fl *ast.FieldList) string {
	if fl == nil || len(fl.List) == 0 {
		return ""
	}
	switch t := fl.List[0].Type.(type) {
	case *ast.StarExpr:
		if id, ok := t.X.(*ast.Ident); ok {
			return id.Name
		}
	case *ast.Ident:
		return t.Name
	}
	return ""
}
