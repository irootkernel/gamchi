package docscheck

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestCheckedInRoadmap(t *testing.T) {
	vs := CheckFile(filepath.Join(findRepoRoot(t), RoadmapPath))
	if len(vs) != 0 {
		t.Fatalf("roadmap violations: %v", vs)
	}
}

func TestRejectsWrongTaskCount(t *testing.T) {
	vs := Check("# grokgrok\n\nCurrent Task: none.\nNext eligible Task: none.\n")
	if !hasCheck(vs, "task-count") {
		t.Fatalf("expected task-count, got %v", vs)
	}
}

func TestAllowsAppendedTask(t *testing.T) {
	body := checkedIn(t)
	body += "\n| [TASK-099](#epic-001-foundation) | extra | `Planned` | TASK-003 | extra |\n"
	vs := Check(body)
	if hasCheck(vs, "task-count") {
		t.Fatalf("appended task must not trip a fixed count, got %v", vs)
	}
}

func TestRejectsTwoActiveTasks(t *testing.T) {
	body := checkedIn(t)
	const needle = "| `Planned` | TASK-"
	if !strings.Contains(body, needle) {
		t.Fatal("need a Planned task row to promote")
	}
	body = strings.Replace(body, needle, "| `In Progress` | TASK-", 1)
	vs := Check(body)
	if !hasCheck(vs, "one-active-task") && !hasCheck(vs, "current-task") {
		t.Fatalf("expected one-active-task or current-task, got %v", vs)
	}
}

func checkedIn(t *testing.T) string {
	t.Helper()
	body, err := os.ReadFile(filepath.Join(findRepoRoot(t), RoadmapPath))
	if err != nil {
		t.Fatal(err)
	}
	return string(body)
}

func hasCheck(vs []Violation, check string) bool {
	for _, v := range vs {
		if v.Check == check {
			return true
		}
	}
	return false
}

func findRepoRoot(t *testing.T) string {
	t.Helper()
	dir, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	for {
		if _, err := os.Stat(filepath.Join(dir, "go.mod")); err == nil {
			return dir
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			t.Fatal("go.mod not found")
		}
		dir = parent
	}
}
