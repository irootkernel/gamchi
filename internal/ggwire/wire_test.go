package ggwire

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestSubsetDigest(t *testing.T) {
	body, err := os.ReadFile(filepath.Join(findRepoRoot(t), SubsetRelPath))
	if err != nil {
		t.Fatal(err)
	}
	sum := sha256.Sum256(body)
	if got := hex.EncodeToString(sum[:]); got != SubsetSHA256 {
		t.Fatalf("subset digest %s, want %s", got, SubsetSHA256)
	}
}

func TestSubsetClientMethodsMatch(t *testing.T) {
	subset := loadSubset(t)
	if len(subset.ClientMethods) != len(ClientMethods) {
		t.Fatalf("subset has %d client methods, pin has %d", len(subset.ClientMethods), len(ClientMethods))
	}
	for _, name := range ClientMethods {
		if _, ok := subset.ClientMethods[name]; !ok {
			t.Fatalf("pinned method %q missing from subset", name)
		}
	}
}

func TestSubsetClosedVocabularies(t *testing.T) {
	subset := loadSubset(t)
	for _, name := range []string{
		RequestCommandApproval, RequestFileChangeApproval, RequestUserInput,
		RequestPermissionsApproval, RequestMCPElicitation,
	} {
		if _, ok := subset.ServerRequests[name]; !ok {
			t.Fatalf("server request %q missing from subset", name)
		}
	}
	wantDecisions := map[string]string{
		"accept_once":           string(DecisionAccept),
		"accept_for_generation": string(DecisionAcceptForSession),
		"decline":               string(DecisionDecline),
		"cancel":                string(DecisionCancel),
	}
	for k, want := range wantDecisions {
		if subset.Decisions[k] != want {
			t.Fatalf("decision %s: subset %q want %q", k, subset.Decisions[k], want)
		}
	}
	policies := map[string]bool{}
	for _, p := range subset.RequiredShapes.ApprovalPolicyValues {
		policies[p] = true
	}
	for _, p := range []ApprovalPolicy{ApprovalUntrusted, ApprovalOnRequest, ApprovalNever} {
		if !policies[string(p)] {
			t.Fatalf("approvalPolicy %q missing from subset", p)
		}
	}
	sand := map[string]bool{}
	for _, s := range subset.RequiredShapes.Sandbox.ThreadValues {
		sand[s] = true
	}
	if !sand[string(SandboxReadOnly)] || !sand[string(SandboxWorkspaceWrite)] {
		t.Fatalf("sandbox values %v", subset.RequiredShapes.Sandbox.ThreadValues)
	}
	term := map[string]bool{}
	for _, s := range subset.Notifications.TerminalTurn.TerminalStatuses {
		term[s] = true
	}
	for _, s := range []TurnStatus{TurnCompleted, TurnInterrupted, TurnFailed} {
		if !term[string(s)] {
			t.Fatalf("terminal status %q missing from subset", s)
		}
	}
	if term[string(TurnInProgress)] {
		t.Fatal("inProgress must not be listed as terminal")
	}
}

type subsetFile struct {
	ClientMethods  map[string][]string        `json:"client_methods"`
	ServerRequests map[string]json.RawMessage `json:"server_requests"`
	Decisions      map[string]string          `json:"command_and_file_approval_decisions"`
	RequiredShapes struct {
		ApprovalPolicyValues []string `json:"approval_policy_values"`
		Sandbox              struct {
			ThreadValues []string `json:"thread_values"`
		} `json:"sandbox"`
	} `json:"required_shapes"`
	Notifications struct {
		TerminalTurn struct {
			TerminalStatuses []string `json:"terminal_statuses"`
		} `json:"terminal_turn"`
	} `json:"notifications"`
}

func loadSubset(t *testing.T) subsetFile {
	t.Helper()
	body, err := os.ReadFile(filepath.Join(findRepoRoot(t), SubsetRelPath))
	if err != nil {
		t.Fatal(err)
	}
	var subset subsetFile
	if err := json.Unmarshal(body, &subset); err != nil {
		t.Fatal(err)
	}
	return subset
}

func TestParseClosedEnums(t *testing.T) {
	if _, err := ParseApprovalPolicy("always"); err == nil {
		t.Fatal("expected rejection")
	}
	if _, err := ParseThreadSandbox("danger-full-access"); err == nil {
		t.Fatal("expected rejection")
	}
	p, err := ParseApprovalPolicy(string(ApprovalUntrusted))
	if err != nil || p != ApprovalUntrusted {
		t.Fatalf("untrusted: %v %q", err, p)
	}
}

func TestItemAllowlist(t *testing.T) {
	pub, fatal := KnownItemType(ItemAgentMessage)
	if !pub || fatal {
		t.Fatal("agentMessage must be public")
	}
	pub, fatal = KnownItemType("subAgentActivity")
	if pub || !fatal {
		t.Fatal("subAgentActivity must be excluded-fatal")
	}
	pub, fatal = KnownItemType("mystery")
	if pub || fatal {
		t.Fatal("unknown types are neither public nor excluded-fatal")
	}
	pub, fatal = KnownItemType("imageView")
	if pub || fatal {
		t.Fatal("imageView is not a v1 emit type")
	}
}

func TestMapStopReason(t *testing.T) {
	if MapStopReason("end_turn") != TurnCompleted {
		t.Fatal("end_turn")
	}
	if MapStopReason("cancelled") != TurnInterrupted {
		t.Fatal("cancelled")
	}
	if MapStopReason("refusal") != TurnFailed {
		t.Fatal("refusal")
	}
	if DefaultApprovalPolicy != ApprovalNever || DefaultThreadSandbox != SandboxWorkspaceWrite {
		t.Fatal("MCP defaults")
	}
}

func TestNoJobAlias(t *testing.T) {
	// The worker identity is Thread/Turn/Item. A "Job" type must not appear.
	if _, err := os.Stat(filepath.Join(findRepoRoot(t), "internal/ggwire/job.go")); err == nil {
		t.Fatal("ad hoc job.go is forbidden")
	}
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
