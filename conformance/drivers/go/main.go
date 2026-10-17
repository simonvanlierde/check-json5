// Conformance driver for the Go parsers. Usage: driver <parser> < paths
// Prints "ready", then "<path>\t<accept|reject|crash>" per path. See conformance/README.md.
package main

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"

	"github.com/tailscale/hujson"
	"github.com/tidwall/jsonc"
	"github.com/titanous/json5"
)

var parsers = map[string]func([]byte) error{
	"go-hujson": func(b []byte) error {
		_, err := hujson.Parse(b)
		return err
	},
	"go-json5": func(b []byte) error {
		var v any
		return json5.Unmarshal(b, &v)
	},
	// tidwall/jsonc turns comments and trailing commas into spaces; encoding/json parses the rest.
	"go-tidwall-jsonc": func(b []byte) error {
		var v any
		return json.Unmarshal(jsonc.ToJSON(b), &v)
	},
	// The usual way to read a JSON Lines stream in Go. It splits on whitespace, not on lines.
	"go-json-stream": func(b []byte) error {
		dec := json.NewDecoder(bytes.NewReader(b))
		for {
			var v any
			if err := dec.Decode(&v); errors.Is(err, io.EOF) {
				return nil
			} else if err != nil {
				return err
			}
		}
	},
}

func verdict(path string, parse func([]byte) error) (result string) {
	b, err := os.ReadFile(path)
	if err != nil {
		return "crash"
	}
	defer func() {
		if recover() != nil {
			result = "crash"
		}
	}()
	if parse(b) != nil {
		return "reject"
	}
	return "accept"
}

func main() {
	name := ""
	if len(os.Args) > 1 {
		name = os.Args[1]
	}
	parse, ok := parsers[name]
	if !ok {
		fmt.Fprintln(os.Stderr, "unknown parser:", name)
		os.Exit(2)
	}
	fmt.Println("ready")
	scanner := bufio.NewScanner(os.Stdin)
	for scanner.Scan() {
		if path := scanner.Text(); path != "" {
			fmt.Printf("%s\t%s\n", path, verdict(path, parse))
		}
	}
}
