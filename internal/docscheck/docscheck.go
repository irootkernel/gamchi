// Package docscheck validates that docs/roadmap/README.md is the sole
// lifecycle authority for grokgrok epics and tasks (TASK-001).
package docscheck

import (
	"fmt"
	"os"
	"regexp"
	"strconv"
	"strings"
)

const RoadmapPath = "docs/roadmap/README.md"

const maxTasksPerEpic = 7

const (
	statusPlanned    = "Planned"
	statusInProgress = "In Progress"
	statusInReview   = "In Review"
	statusCompleted  = "Completed"
	statusBlocked    = "Blocked"
	statusDeferred   = "Deferred"
)

var validStatuses = map[string]bool{
	statusPlanned: true, statusInProgress: true, statusInReview: true,
	statusCompleted: true, statusBlocked: true, statusDeferred: true,
}

var (
	taskRowRe = regexp.MustCompile(
		`(?m)^\| \[TASK-([0-9]{3})\]\([^)]*\) \| ([^|]+) \| ` +
			"`" + `([^` + "`" + `]+)` + "`" + ` \| ([^|]+) \|`,
	)
	epicHeaderRe = regexp.MustCompile(`(?m)^## (EPIC-[0-9]{3}): `)
	epicStatusRe = regexp.MustCompile("(?m)^Status: `([^`]*)`$")
	currentRe    = regexp.MustCompile("(?m)^Current Task: (`TASK-[0-9]{3}`|none)")
	nextRe       = regexp.MustCompile("(?m)^Next eligible Task: (`TASK-[0-9]{3}`|none)")
	epicSplitRe  = regexp.MustCompile(`(?m)^## EPIC-[0-9]{3}: `)
)

// Violation is one roadmap integrity failure.
type Violation struct {
	Check   string
	Message string
}

func (v Violation) String() string { return v.Check + ": " + v.Message }

type taskRow struct {
	id     string
	title  string
	status string
}

// CheckFile loads the repository roadmap and checks it.
func CheckFile(path string) []Violation {
	body, err := os.ReadFile(path)
	if err != nil {
		return []Violation{{Check: "roadmap-readable", Message: err.Error()}}
	}
	return Check(string(body))
}

// Check validates roadmap markdown.
func Check(body string) []Violation {
	var vs []Violation
	rows := parseTasks(body)
	if len(rows) == 0 {
		vs = append(vs, Violation{Check: "task-count", Message: "roadmap has no TASK rows"})
	}
	seen := map[string]int{}
	active := []string{}
	for _, row := range rows {
		seen[row.id]++
		if !validStatuses[row.status] {
			vs = append(vs, Violation{
				Check:   "task-status",
				Message: fmt.Sprintf("TASK-%s has invalid status %q", row.id, row.status),
			})
		}
		if row.status == statusInProgress || row.status == statusInReview {
			active = append(active, "TASK-"+row.id)
		}
	}
	for id, n := range seen {
		if n > 1 {
			vs = append(vs, Violation{
				Check:   "task-set",
				Message: fmt.Sprintf("TASK-%s appears %d times", id, n),
			})
		}
	}

	if len(active) > 1 {
		vs = append(vs, Violation{
			Check:   "one-active-task",
			Message: "at most one In Progress or In Review task, got " + strings.Join(active, ", "),
		})
	}

	epics := epicHeaderRe.FindAllStringSubmatch(body, -1)
	if len(epics) == 0 {
		vs = append(vs, Violation{Check: "epic-count", Message: "roadmap has no EPIC headers"})
	}
	for i, m := range epics {
		want := fmt.Sprintf("EPIC-%03d", i+1)
		if m[1] != want {
			vs = append(vs, Violation{
				Check:   "epic-order",
				Message: fmt.Sprintf("epic %d is %s, want %s", i+1, m[1], want),
			})
		}
	}
	if n := len(epicStatusRe.FindAllStringSubmatch(body, -1)); n != len(epics) {
		vs = append(vs, Violation{
			Check:   "epic-status",
			Message: fmt.Sprintf("want %d epic Status lines, got %d", len(epics), n),
		})
	}

	for i, section := range epicTaskSections(body) {
		n := len(parseTasks(section))
		if n > maxTasksPerEpic {
			vs = append(vs, Violation{
				Check:   "epic-task-cap",
				Message: fmt.Sprintf("EPIC-%03d has %d tasks, max %d", i+1, n, maxTasksPerEpic),
			})
		}
	}

	cur := currentRe.FindStringSubmatch(body)
	nxt := nextRe.FindStringSubmatch(body)
	if cur == nil {
		vs = append(vs, Violation{Check: "current-task", Message: "missing Current Task line"})
	}
	if nxt == nil {
		vs = append(vs, Violation{Check: "next-task", Message: "missing Next eligible Task line"})
	}
	if cur != nil {
		got := strings.Trim(cur[1], "`")
		switch len(active) {
		case 0:
			if got != "none" {
				vs = append(vs, Violation{
					Check:   "current-task",
					Message: "Current Task must be none when no task is In Progress or In Review, got " + got,
				})
			}
		case 1:
			if got != active[0] {
				vs = append(vs, Violation{
					Check:   "current-task",
					Message: "Current Task must be " + active[0] + ", got " + got,
				})
			}
		}
	}
	if nxt != nil {
		got := strings.Trim(nxt[1], "`")
		if got != "none" {
			if _, err := strconv.Atoi(strings.TrimPrefix(got, "TASK-")); err != nil {
				vs = append(vs, Violation{Check: "next-task", Message: "malformed Next eligible Task " + got})
			}
		}
	}
	return vs
}

func epicTaskSections(body string) []string {
	idxs := epicSplitRe.FindAllStringIndex(body, -1)
	if len(idxs) == 0 {
		return nil
	}
	out := make([]string, 0, len(idxs))
	for i, loc := range idxs {
		end := len(body)
		if i+1 < len(idxs) {
			end = idxs[i+1][0]
		}
		out = append(out, body[loc[0]:end])
	}
	return out
}

func parseTasks(body string) []taskRow {
	matches := taskRowRe.FindAllStringSubmatch(body, -1)
	rows := make([]taskRow, 0, len(matches))
	for _, m := range matches {
		rows = append(rows, taskRow{
			id:     m[1],
			title:  strings.TrimSpace(m[2]),
			status: strings.TrimSpace(m[3]),
		})
	}
	return rows
}
