package views

import (
	"encoding/json"
	"fmt"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/model"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/styles"
)

const firehoseCap = 10000

type FirehoseEntryMsg struct {
	Entry model.AuditEntry
}

type FirehoseBatchMsg struct {
	Entries []model.AuditEntry
}

type FirehoseActivityMsg struct {
	Active []model.PgActivity
}

type Firehose struct {
	width, height int
	entries       []model.AuditEntry
	active        []model.PgActivity
	offset        int
	paused        bool
	filterAction  string
	totalRecv     int64
	firstSeen     time.Time
}

func NewFirehose() Firehose {
	return Firehose{firstSeen: time.Now()}
}

func (f Firehose) Init() tea.Cmd { return nil }

func (f Firehose) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case FirehoseEntryMsg:
		if !f.paused {
			f.entries = prepend(f.entries, msg.Entry)
		}
		f.totalRecv++
	case FirehoseBatchMsg:
		for _, e := range msg.Entries {
			f.entries = prepend(f.entries, e)
		}
		f.totalRecv += int64(len(msg.Entries))
	case FirehoseActivityMsg:
		f.active = msg.Active
	case tea.WindowSizeMsg:
		f.width = msg.Width
		f.height = msg.Height
	case tea.KeyMsg:
		switch msg.String() {
		case "p":
			f.paused = !f.paused
		case "a":
			switch f.filterAction {
			case "":
				f.filterAction = "assert"
			case "assert":
				f.filterAction = "retract"
			case "retract":
				f.filterAction = "correct"
			default:
				f.filterAction = ""
			}
		case "up", "k":
			if f.offset > 0 {
				f.offset--
			}
		case "down", "j":
			f.offset++
		case "home", "g":
			f.offset = 0
		case "enter":
			visible := f.filtered()
			if f.offset < len(visible) {
				sid := visible[f.offset].StatementID
				if sid != "" {
					return f, func() tea.Msg {
						return SelectStatementMsg{StatementID: sid}
					}
				}
			}
		}
	}
	return f, nil
}

func prepend(entries []model.AuditEntry, e model.AuditEntry) []model.AuditEntry {
	entries = append([]model.AuditEntry{e}, entries...)
	if len(entries) > firehoseCap {
		entries = entries[:firehoseCap]
	}
	return entries
}

func (f Firehose) View() string {
	w := f.width
	if w < 40 {
		w = 80
	}

	var b strings.Builder

	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#585B70"))
	detailKeyStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#89B4FA"))
	detailValStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#CDD6F4"))
	activeStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#94E2D5"))
	pidStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#F9E2AF"))
	queryStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#A6ADC8"))

	// ── Live queries panel ──
	if len(f.active) > 0 {
		b.WriteString(styles.BoxTitleStyle.Render(fmt.Sprintf("  Live Queries (%d)", len(f.active))) + "\n")
		for _, a := range f.active {
			query := strings.TrimSpace(a.Query)
			query = strings.ReplaceAll(query, "\n", " ")
			query = strings.Join(strings.Fields(query), " ")
			if len(query) > w-30 {
				query = query[:w-33] + "..."
			}

			state := activeStyle.Render(a.State)
			pid := pidStyle.Render(fmt.Sprintf("pid:%d", a.PID))

			dur := ""
			if a.QueryStart != nil {
				dur = dimStyle.Render(fmt.Sprintf(" %s", time.Since(*a.QueryStart).Truncate(time.Millisecond)))
			}

			app := ""
			if a.AppName != "" {
				app = dimStyle.Render(" [" + a.AppName + "]")
			}

			b.WriteString(fmt.Sprintf("  %s %s%s%s\n", pid, state, dur, app))
			b.WriteString("    " + queryStyle.Render(query) + "\n")
		}
		b.WriteString(dimStyle.Render(strings.Repeat("─", w)) + "\n")
	}

	// ── Status bar ──
	elapsed := time.Since(f.firstSeen).Seconds()
	eps := float64(0)
	if elapsed > 1 {
		eps = float64(f.totalRecv) / elapsed
	}

	statusParts := []string{
		fmt.Sprintf(" Audit: %d", f.totalRecv),
		fmt.Sprintf("%.1f evt/s", eps),
	}
	if f.paused {
		statusParts = append(statusParts, styles.ActionCorrectStyle.Render("PAUSED"))
	}
	if f.filterAction != "" {
		statusParts = append(statusParts, "filter: "+f.filterAction)
	}
	statusLeft := strings.Join(statusParts, "  |  ")
	statusRight := styles.HelpStyle.Render("[p]ause  [a]ction  [j/k]scroll")
	gap := w - lipgloss.Width(statusLeft) - lipgloss.Width(statusRight) - 2
	if gap < 1 {
		gap = 1
	}
	b.WriteString(statusLeft + strings.Repeat(" ", gap) + statusRight + "\n")
	b.WriteString(dimStyle.Render(strings.Repeat("─", w)) + "\n")

	// ── Audit log ──
	visible := f.filtered()

	linesPerEntry := 3
	activeLines := 0
	if len(f.active) > 0 {
		activeLines = len(f.active)*2 + 2
	}
	maxEntries := (f.height - 6 - activeLines) / linesPerEntry
	if maxEntries < 3 {
		maxEntries = 3
	}

	if f.offset >= len(visible) {
		f.offset = len(visible) - 1
	}
	if f.offset < 0 {
		f.offset = 0
	}

	end := f.offset + maxEntries
	if end > len(visible) {
		end = len(visible)
	}

	if len(visible) == 0 {
		b.WriteString(styles.HelpStyle.Render("  Waiting for audit events...") + "\n")
	}

	for i := f.offset; i < end; i++ {
		e := visible[i]

		ts := e.At.Format("15:04:05.000")
		action := colorAction(e.Action)
		actor := e.Actor
		if actor == "" {
			actor = "-"
		}
		sid := e.StatementID
		if len(sid) > 36 {
			sid = sid[:36]
		}

		b.WriteString(fmt.Sprintf(" %s  %s  %s  %s\n",
			dimStyle.Render(ts),
			action,
			lipgloss.NewStyle().Foreground(styles.Highlight).Render(truncate(actor, 20)),
			dimStyle.Render(sid),
		))

		detail := renderDetail(e.Detail, w-4, detailKeyStyle, detailValStyle)
		b.WriteString("   " + detail + "\n")

		if i < end-1 {
			b.WriteString(dimStyle.Render(" "+strings.Repeat("·", w-2)) + "\n")
		}
	}

	return b.String()
}

func renderDetail(raw json.RawMessage, maxW int, keyStyle, valStyle lipgloss.Style) string {
	if len(raw) == 0 {
		return lipgloss.NewStyle().Foreground(lipgloss.Color("#585B70")).Render("(no detail)")
	}

	var m map[string]interface{}
	if err := json.Unmarshal(raw, &m); err != nil {
		return valStyle.Render(truncate(string(raw), maxW))
	}

	var parts []string
	for k, v := range m {
		vs := fmt.Sprintf("%v", v)
		parts = append(parts, keyStyle.Render(k)+"="+valStyle.Render(vs))
	}

	line := strings.Join(parts, "  ")
	if lipgloss.Width(line) > maxW {
		line = truncate(line, maxW)
	}
	return line
}

func (f Firehose) filtered() []model.AuditEntry {
	if f.filterAction == "" {
		return f.entries
	}
	var out []model.AuditEntry
	for _, e := range f.entries {
		if e.Action == f.filterAction {
			out = append(out, e)
		}
	}
	return out
}

func colorAction(action string) string {
	switch action {
	case "assert":
		return styles.ActionAssertStyle.Render(action)
	case "retract":
		return styles.ActionRetractStyle.Render(action)
	case "correct":
		return styles.ActionCorrectStyle.Render(action)
	default:
		return action
	}
}

func truncate(s string, max int) string {
	if max <= 0 {
		return ""
	}
	if len(s) <= max {
		return s
	}
	if max <= 3 {
		return s[:max]
	}
	return s[:max-3] + "..."
}
