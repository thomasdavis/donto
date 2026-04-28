package views

import (
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/styles"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/model"
)

// ContextsDataMsg delivers context statistics to the contexts view.
type ContextsDataMsg struct {
	Contexts []model.ContextStat
}

// Contexts is the Tab 4 context list view.
type Contexts struct {
	width, height int
	contexts      []model.ContextStat
	cursor        int
	scrollOffset  int
}

func NewContexts() Contexts {
	return Contexts{}
}

func (c Contexts) Init() tea.Cmd { return nil }

func (c Contexts) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case ContextsDataMsg:
		c.contexts = msg.Contexts
		if c.cursor >= len(c.contexts) {
			c.cursor = 0
		}
	case tea.WindowSizeMsg:
		c.width = msg.Width
		c.height = msg.Height
	case tea.KeyMsg:
		switch msg.String() {
		case "up", "k":
			if c.cursor > 0 {
				c.cursor--
				if c.cursor < c.scrollOffset {
					c.scrollOffset = c.cursor
				}
			}
		case "down", "j":
			if c.cursor < len(c.contexts)-1 {
				c.cursor++
				listH := c.listHeight()
				if c.cursor >= c.scrollOffset+listH {
					c.scrollOffset = c.cursor - listH + 1
				}
			}
		case "home", "g":
			c.cursor = 0
			c.scrollOffset = 0
		case "end", "G":
			if len(c.contexts) > 0 {
				c.cursor = len(c.contexts) - 1
			}
		}
	}
	return c, nil
}

func (c Contexts) listHeight() int {
	h := c.height - 12 // leave room for header + detail panel
	if h < 5 {
		h = 5
	}
	return h
}

func (c Contexts) View() string {
	if len(c.contexts) == 0 {
		return styles.HelpStyle.Render("  No contexts loaded. Waiting for data...")
	}

	var b strings.Builder

	// Header
	header := fmt.Sprintf(" %-44s %-10s %8s  %s",
		"IRI", "KIND", "STMTS", "LAST ASSERT")
	b.WriteString(styles.TableHeaderStyle.Render(header) + "\n")

	// List
	listH := c.listHeight()
	end := c.scrollOffset + listH
	if end > len(c.contexts) {
		end = len(c.contexts)
	}

	for i := c.scrollOffset; i < end; i++ {
		ctx := c.contexts[i]
		iri := truncate(ctx.IRI, 42)
		kind := ctx.Kind
		count := fmt.Sprintf("%d", ctx.StatementCount)
		lastAssert := "-"
		if ctx.LastAssert != nil {
			lastAssert = ctx.LastAssert.Format("2006-01-02 15:04")
		}

		row := fmt.Sprintf(" %-44s %-10s %8s  %s", iri, kind, count, lastAssert)

		if i == c.cursor {
			row = styles.SelectedRowStyle.Render(row)
		} else if i%2 == 1 {
			row = styles.TableRowAltStyle.Render(row)
		} else {
			row = styles.TableRowStyle.Render(row)
		}
		b.WriteString(row + "\n")
	}

	b.WriteString("\n")

	// Detail panel for selected context
	if c.cursor < len(c.contexts) {
		b.WriteString(c.renderDetail(c.contexts[c.cursor]))
	}

	return b.String()
}

func (c Contexts) renderDetail(ctx model.ContextStat) string {
	var rows []string
	rows = append(rows, fmt.Sprintf("  %s %s",
		styles.StatLabelStyle.Render("IRI:"),
		styles.StatValueStyle.Render(ctx.IRI)))
	rows = append(rows, fmt.Sprintf("  %s %s",
		styles.StatLabelStyle.Render("Kind:"),
		styles.StatValueStyle.Render(ctx.Kind)))

	parent := "(none)"
	if ctx.Parent != nil {
		parent = *ctx.Parent
	}
	rows = append(rows, fmt.Sprintf("  %s %s",
		styles.StatLabelStyle.Render("Parent:"),
		styles.StatValueStyle.Render(parent)))
	rows = append(rows, fmt.Sprintf("  %s %s",
		styles.StatLabelStyle.Render("Statements:"),
		styles.StatValueStyle.Render(fmt.Sprintf("%d", ctx.StatementCount))))

	lastAssert := "(never)"
	if ctx.LastAssert != nil {
		lastAssert = ctx.LastAssert.Format("2006-01-02 15:04:05")
	}
	rows = append(rows, fmt.Sprintf("  %s %s",
		styles.StatLabelStyle.Render("Last Assert:"),
		styles.StatValueStyle.Render(lastAssert)))

	content := strings.Join(rows, "\n")

	return styles.BoxStyle.Width(c.width - 2).Render(
		lipgloss.JoinVertical(lipgloss.Left,
			styles.BoxTitleStyle.Render("Context Detail"),
			content,
		),
	)
}
