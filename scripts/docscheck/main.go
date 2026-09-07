package main

import (
	"fmt"
	"os"

	"github.com/irootkernel/grokgrok/internal/docscheck"
)

func main() {
	vs := docscheck.CheckFile(docscheck.RoadmapPath)
	if len(vs) == 0 {
		return
	}
	for _, v := range vs {
		fmt.Fprintln(os.Stderr, v)
	}
	os.Exit(1)
}
