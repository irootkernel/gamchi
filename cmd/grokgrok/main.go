// Command grokgrok is the local Grok worker, MCP host surface, and later
// app-server. TASK-001 only advertises the command grammar; worker, MCP, and
// app-server are not implemented yet.
package main

import (
	"fmt"
	"io"
	"os"
	"strings"
)

const usage = `Usage:
  grokgrok version
  grokgrok worker <start|wait|status|result|cancel|list> ...
  grokgrok mcp
  grokgrok app-server --listen unix://<absolute-path> [--home <absolute-path>]
`

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

func run(args []string, stdout, stderr io.Writer) int {
	if len(args) == 0 {
		fmt.Fprint(stderr, usage)
		return 2
	}
	switch args[0] {
	case "version", "--version":
		fmt.Fprintln(stdout, "grokgrok/dev")
		return 0
	case "worker", "mcp", "app-server":
		fmt.Fprintf(stderr, "GROKGROK_STARTUP_ERROR NOT_IMPLEMENTED %s\n", args[0])
		return 1
	default:
		fmt.Fprint(stderr, "GROKGROK_STARTUP_ERROR INVALID_CONFIG\n")
		if !strings.HasPrefix(args[0], "-") {
			fmt.Fprint(stderr, usage)
		}
		return 1
	}
}
