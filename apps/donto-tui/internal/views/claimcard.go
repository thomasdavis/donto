package views

import (
	"bytes"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/styles"
)

// ClaimCardDataMsg delivers claim card JSON from donto_claim_card().
type ClaimCardDataMsg struct {
	JSON   string
	StmtID string
}

// ClaimCard is the Tab 5 claim card detail view.
type ClaimCard struct {
	width, height int
	stmtID        string
	rawJSON       string
	parsed        map[string]any
	viewport      viewport.Model
	ready         bool
}

func NewClaimCard() ClaimCard {
	return ClaimCard{}
}

func (c ClaimCard) Init() tea.Cmd { return nil }

func (c ClaimCard) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case ClaimCardDataMsg:
		c.stmtID = msg.StmtID
		c.rawJSON = msg.JSON
		c.parsed = nil
		_ = json.Unmarshal([]byte(msg.JSON), &c.parsed)
		c.viewport.SetContent(c.renderCard())
		c.viewport.GotoTop()
	case tea.WindowSizeMsg:
		c.width = msg.Width
		c.height = msg.Height
		if !c.ready {
			c.viewport = viewport.New(msg.Width-2, msg.Height-3)
			c.viewport.SetContent(c.renderCard())
			c.ready = true
		} else {
			c.viewport.Width = msg.Width - 2
			c.viewport.Height = msg.Height - 3
		}
	}

	var cmd tea.Cmd
	c.viewport, cmd = c.viewport.Update(msg)
	return c, cmd
}

func (c ClaimCard) View() string {
	if c.stmtID == "" {
		return styles.HelpStyle.Render("  No claim card loaded. Select a statement from Explorer (tab 3) and press Enter.")
	}

	header := fmt.Sprintf(" Claim Card: %s", c.stmtID)
	headerLine := styles.BoxTitleStyle.Render(header)
	scrollInfo := styles.HelpStyle.Render(
		fmt.Sprintf(" %.0f%% | scroll: arrows/pgup/pgdn", c.viewport.ScrollPercent()*100))

	return lipgloss.JoinVertical(lipgloss.Left,
		headerLine,
		c.viewport.View(),
		scrollInfo,
	)
}

func (c ClaimCard) renderCard() string {
	if c.parsed == nil {
		if c.rawJSON == "" {
			return styles.HelpStyle.Render("No data")
		}
		// Fallback: pretty-print raw JSON
		var pretty bytes.Buffer
		if err := json.Indent(&pretty, []byte(c.rawJSON), "", "  "); err != nil {
			return c.rawJSON
		}
		return pretty.String()
	}

	var b strings.Builder

	// Statement fields
	b.WriteString(c.section("Statement"))
	for _, key := range []string{
		"statement_id", "subject", "predicate", "object_iri", "object_lit",
		"context", "polarity", "maturity", "tx_lo", "tx_hi", "valid_lo", "valid_hi",
	} {
		if v, ok := c.parsed[key]; ok && v != nil {
			b.WriteString(c.field(key, fmt.Sprintf("%v", v)))
		}
	}

	// Evidence
	if ev, ok := c.parsed["evidence"]; ok {
		b.WriteString("\n")
		b.WriteString(c.section("Evidence"))
		c.renderList(&b, ev)
	}

	// Arguments
	if args, ok := c.parsed["arguments"]; ok {
		b.WriteString("\n")
		b.WriteString(c.section("Arguments"))
		c.renderList(&b, args)
	}

	// Obligations
	if obs, ok := c.parsed["obligations"]; ok {
		b.WriteString("\n")
		b.WriteString(c.section("Obligations"))
		c.renderList(&b, obs)
	}

	// Shape annotations
	if shapes, ok := c.parsed["shapes"]; ok {
		b.WriteString("\n")
		b.WriteString(c.section("Shape Annotations"))
		c.renderList(&b, shapes)
	}

	// Any remaining top-level keys
	known := map[string]bool{
		"statement_id": true, "subject": true, "predicate": true,
		"object_iri": true, "object_lit": true, "context": true,
		"polarity": true, "maturity": true, "tx_lo": true, "tx_hi": true,
		"valid_lo": true, "valid_hi": true, "evidence": true,
		"arguments": true, "obligations": true, "shapes": true,
	}
	for k, v := range c.parsed {
		if !known[k] {
			b.WriteString("\n")
			b.WriteString(c.section(k))
			c.renderAny(&b, v, "  ")
		}
	}

	return b.String()
}

func (c ClaimCard) section(name string) string {
	return styles.BoxTitleStyle.Render("  "+name) + "\n"
}

func (c ClaimCard) field(key, value string) string {
	return fmt.Sprintf("  %s %s\n",
		styles.StatLabelStyle.Width(16).Render(key+":"),
		styles.StatValueStyle.Render(value))
}

func (c ClaimCard) renderList(b *strings.Builder, v any) {
	list, ok := v.([]any)
	if !ok {
		c.renderAny(b, v, "  ")
		return
	}
	if len(list) == 0 {
		b.WriteString(styles.HelpStyle.Render("  (none)") + "\n")
		return
	}
	for i, item := range list {
		b.WriteString(fmt.Sprintf("  %s\n", styles.StatLabelStyle.Render(fmt.Sprintf("[%d]", i))))
		c.renderAny(b, item, "    ")
	}
}

func (c ClaimCard) renderAny(b *strings.Builder, v any, indent string) {
	switch val := v.(type) {
	case map[string]any:
		for k, inner := range val {
			b.WriteString(fmt.Sprintf("%s%s %v\n", indent,
				styles.StatLabelStyle.Render(k+":"),
				inner))
		}
	case []any:
		for _, item := range val {
			c.renderAny(b, item, indent+"  ")
		}
	default:
		b.WriteString(fmt.Sprintf("%s%v\n", indent, val))
	}
}
