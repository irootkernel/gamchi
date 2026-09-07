// Package ggwire pins subset-derived wire names (methods, policies, statuses)
// and grokgrok emit policy for ThreadItems. The subset is a Dolgorae client
// requirement, not this package's item allowlist. There is no ad hoc job JSON.
package ggwire

import "fmt"

// SubsetSHA256 is SHA-256 of docs/protocol/references/dolgorae-codex-0.149.0-required-subset.json.
const SubsetSHA256 = "7d6b33228266826eb5077192867f31f0caf2df1875525c194ee23f179050d409"

// SubsetRelPath is the repository-relative subset artifact.
const SubsetRelPath = "docs/protocol/references/dolgorae-codex-0.149.0-required-subset.json"

// UserAgent is the honest initialize identity. It is not Codex or CCAS.
// "grokgrok" is a working title.
const UserAgent = "grokgrok/app-server-v1"

// Transport bounds from Dolgorae src/app_server.rs / CCAS REQ-TRANSPORT-004.
const (
	MaxHTTPUpgradeBytes        = 16 * 1024
	MaxWebSocketFrameBytes     = 16 * 1024 * 1024
	MaxWebSocketMessageBytes   = 32 * 1024 * 1024
	MaxSolicitedEnvelopeBytes  = 64 * 1024
	MaxCorrelatedMessages      = 4096
	EarlyTopLevelIDPrefixBytes = 64 * 1024
)

// Client methods Dolgorae sends after initialize/initialized.
const (
	MethodInitialize        = "initialize"
	MethodAccountRead       = "account/read"
	MethodModelList         = "model/list"
	MethodThreadStart       = "thread/start"
	MethodThreadResume      = "thread/resume"
	MethodThreadRead        = "thread/read"
	MethodThreadFork        = "thread/fork"
	MethodTurnStart         = "turn/start"
	MethodTurnInterrupt     = "turn/interrupt"
	NotificationInitialized = "initialized"
)

// Server notifications Dolgorae must not have opted out of.
const (
	NotifyItemStarted                = "item/started"
	NotifyItemCompleted              = "item/completed"
	NotifyItemFileChangePatchUpdated = "item/fileChange/patchUpdated"
	NotifyTurnCompleted              = "turn/completed"
	NotifyThreadStarted              = "thread/started"
)

// Server requests. Support follows the subset classification.
const (
	RequestCommandApproval     = "item/commandExecution/requestApproval"
	RequestFileChangeApproval  = "item/fileChange/requestApproval"
	RequestUserInput           = "item/tool/requestUserInput"
	RequestPermissionsApproval = "item/permissions/requestApproval" // recognized-unsupported
	RequestMCPElicitation      = "mcpServer/elicitation/request"    // recognized-unsupported
)

// ClientMethods is the closed method list from the subset.
var ClientMethods = []string{
	MethodInitialize, MethodAccountRead, MethodModelList,
	MethodThreadStart, MethodThreadResume, MethodThreadRead, MethodThreadFork,
	MethodTurnStart, MethodTurnInterrupt,
}

// ApprovalPolicy is the subset's closed thread/turn approval policy.
type ApprovalPolicy string

const (
	ApprovalUntrusted ApprovalPolicy = "untrusted"
	ApprovalOnRequest ApprovalPolicy = "on-request"
	ApprovalNever     ApprovalPolicy = "never"
)

// ThreadSandbox is the subset's thread sandbox string.
type ThreadSandbox string

const (
	SandboxReadOnly       ThreadSandbox = "read-only"
	SandboxWorkspaceWrite ThreadSandbox = "workspace-write"
)

// MCP spawn defaults (write-capable subagent). App-server omitted fields
// follow the Dolgorae/CCAS wire, not these.
const (
	DefaultApprovalPolicy = ApprovalNever
	DefaultThreadSandbox  = SandboxWorkspaceWrite
)

// TurnSandboxType is the subset's turn sandboxPolicy type.
type TurnSandboxType string

const (
	TurnSandboxReadOnly       TurnSandboxType = "readOnly"
	TurnSandboxWorkspaceWrite TurnSandboxType = "workspaceWrite"
)

// TurnStatus is the subset's turn/completed status vocabulary.
type TurnStatus string

const (
	TurnInProgress  TurnStatus = "inProgress"
	TurnCompleted   TurnStatus = "completed"
	TurnInterrupted TurnStatus = "interrupted"
	TurnFailed      TurnStatus = "failed"
)

// ItemType is a public ThreadItem type grokgrok may emit (ACP-projectable).
type ItemType string

const (
	ItemUserMessage      ItemType = "userMessage"
	ItemAgentMessage     ItemType = "agentMessage"
	ItemPlan             ItemType = "plan"
	ItemCommandExecution ItemType = "commandExecution"
	ItemFileChange       ItemType = "fileChange"
	ItemWebSearch        ItemType = "webSearch"
)

// PublicItemTypes is grokgrok emit policy, not a subset-derived allowlist.
var PublicItemTypes = []ItemType{
	ItemUserMessage, ItemAgentMessage, ItemPlan, ItemCommandExecution,
	ItemFileChange, ItemWebSearch,
}

// ExcludedFatalItemTypes are subset schema branches grokgrok will not emit.
// Presence in the subset means Dolgorae requires Codex to have the schema,
// not that grokgrok should produce them.
var ExcludedFatalItemTypes = []ItemType{
	"collabAgentToolCall", "subAgentActivity",
}

// ApprovalDecision is the subset's command/file approval response.
type ApprovalDecision string

const (
	DecisionAccept           ApprovalDecision = "accept"
	DecisionAcceptForSession ApprovalDecision = "acceptForSession"
	DecisionDecline          ApprovalDecision = "decline"
	DecisionCancel           ApprovalDecision = "cancel"
)

// Thread is a durable conversation identity plus resolved defaults.
type Thread struct {
	ID                    string
	CWD                   string
	Model                 string
	Sandbox               ThreadSandbox
	ApprovalPolicy        ApprovalPolicy
	DeveloperInstructions string
	ACPSessionID          string
}

// Turn is one admitted prompt on a thread.
type Turn struct {
	ID            string
	Status        TurnStatus
	Input         []UserInput
	Model         string
	Effort        string
	StopReason    string // raw ACP session/prompt stopReason
	FailureReason string // e.g. worker_gone; empty on completed
}

// UserInput is one turn/start input element. Text is the v1 worker path.
type UserInput struct {
	Type string
	Text string
}

// Item is one ordered ThreadItem on a turn.
type Item struct {
	ID     string
	Type   ItemType
	Text   string
	Status string
}

// ParseApprovalPolicy rejects unknown values rather than ignoring them.
func ParseApprovalPolicy(v string) (ApprovalPolicy, error) {
	switch ApprovalPolicy(v) {
	case ApprovalUntrusted, ApprovalOnRequest, ApprovalNever:
		return ApprovalPolicy(v), nil
	default:
		return "", fmt.Errorf("unsupported approvalPolicy %q", v)
	}
}

// ParseThreadSandbox rejects unknown values rather than ignoring them.
func ParseThreadSandbox(v string) (ThreadSandbox, error) {
	switch ThreadSandbox(v) {
	case SandboxReadOnly, SandboxWorkspaceWrite:
		return ThreadSandbox(v), nil
	default:
		return "", fmt.Errorf("unsupported sandbox %q", v)
	}
}

// MapStopReason folds an ACP session/prompt stopReason into a subset TurnStatus.
func MapStopReason(stopReason string) TurnStatus {
	switch stopReason {
	case "end_turn":
		return TurnCompleted
	case "cancelled":
		return TurnInterrupted
	default:
		return TurnFailed
	}
}

// KnownItemType reports whether t is public or excluded-fatal.
func KnownItemType(t ItemType) (public, excludedFatal bool) {
	for _, p := range PublicItemTypes {
		if p == t {
			return true, false
		}
	}
	for _, e := range ExcludedFatalItemTypes {
		if e == t {
			return false, true
		}
	}
	return false, false
}
