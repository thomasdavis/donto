package views

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/model"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/styles"
)

type ExplorerResultsMsg struct {
	Statements []model.Statement
	Total      int
}

type ExplorerSearchMsg struct {
	Subject   string
	Predicate string
	Context   string
}

type SelectStatementMsg struct {
	StatementID string
}

const (
	fieldSubject = iota
	fieldPredicate
	fieldContext
	fieldCount
)

type Explorer struct {
	width, height int
	inputs        [fieldCount]textinput.Model
	focusIdx      int
	inFilterPane  bool
	statements    []model.Statement
	total         int
	cursor        int
	scrollOffset  int
	loading       bool
	err           string
}

func NewExplorer() Explorer {
	var inputs [fieldCount]textinput.Model
	labels := [fieldCount]string{"Subject", "Predicate", "Context"}
	placeholders := [fieldCount]string{"ex:alice", "ex:birthYear", "ctx:tui-test"}
	for i := range inputs {
		t := textinput.New()
		t.Placeholder = placeholders[i]
		t.Prompt = labels[i] + ": "
		t.PromptStyle = styles.StatLabelStyle
		t.TextStyle = styles.StatValueStyle
		t.CharLimit = 256
		t.Width = 34
		inputs[i] = t
	}
	return Explorer{loading: true}
}

func (e Explorer) Init() tea.Cmd { return nil }

func (e Explorer) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case ExplorerResultsMsg:
		e.statements = msg.Statements
		e.total = msg.Total
		e.cursor = 0
		e.scrollOffset = 0
		e.loading = false
		e.err = ""
	case tea.WindowSizeMsg:
		e.width = msg.Width
		e.height = msg.Height
	case tea.KeyMsg:
		key := msg.String()

		if key == "/" && !e.inFilterPane {
			e.inFilterPane = true
			e.focusIdx = 0
			e.inputs[0].Focus()
			return e, nil
		}

		if key == "esc" {
			if e.inFilterPane {
				e.inFilterPane = false
				for i := range e.inputs {
					e.inputs[i].Blur()
				}
				return e, nil
			}
		}

		if e.inFilterPane {
			switch key {
			case "enter":
				e.inFilterPane = false
				for i := range e.inputs {
					e.inputs[i].Blur()
				}
				e.loading = true
				return e, e.search()
			case "up":
				e.inputs[e.focusIdx].Blur()
				e.focusIdx = (e.focusIdx - 1 + fieldCount) % fieldCount
				e.inputs[e.focusIdx].Focus()
				return e, nil
			case "down":
				e.inputs[e.focusIdx].Blur()
				e.focusIdx = (e.focusIdx + 1) % fieldCount
				e.inputs[e.focusIdx].Focus()
				return e, nil
			case "ctrl+u":
				e.inputs[e.focusIdx].SetValue("")
				return e, nil
			}
			var cmds []tea.Cmd
			for i := range e.inputs {
				var cmd tea.Cmd
				e.inputs[i], cmd = e.inputs[i].Update(msg)
				cmds = append(cmds, cmd)
			}
			return e, tea.Batch(cmds...)
		}

		switch key {
		case "enter":
			if len(e.statements) > 0 && e.cursor < len(e.statements) {
				return e, func() tea.Msg {
					return SelectStatementMsg{StatementID: e.statements[e.cursor].StatementID}
				}
			}
		case "up", "k":
			if e.cursor > 0 {
				e.cursor--
				if e.cursor < e.scrollOffset {
					e.scrollOffset = e.cursor
				}
			}
		case "down", "j":
			if e.cursor < len(e.statements)-1 {
				e.cursor++
				maxV := e.maxVisibleRows()
				if e.cursor >= e.scrollOffset+maxV {
					e.scrollOffset = e.cursor - maxV + 1
				}
			}
		case "g", "home":
			e.cursor = 0
			e.scrollOffset = 0
		case "G", "end":
			e.cursor = len(e.statements) - 1
			if e.cursor < 0 {
				e.cursor = 0
			}
			maxV := e.maxVisibleRows()
			if e.cursor >= maxV {
				e.scrollOffset = e.cursor - maxV + 1
			}
		}
	}
	return e, nil
}

func (e Explorer) search() tea.Cmd {
	return func() tea.Msg {
		return ExplorerSearchMsg{
			Subject:   e.inputs[fieldSubject].Value(),
			Predicate: e.inputs[fieldPredicate].Value(),
			Context:   e.inputs[fieldContext].Value(),
		}
	}
}

func (e Explorer) maxVisibleRows() int {
	r := e.height - 6
	if r < 5 {
		r = 15
	}
	return r
}

func (e Explorer) View() string {
	w := e.width
	if w < 60 {
		w = 80
	}

	// Filter bar at top (single line)
	var filterParts []string
	for i, inp := range e.inputs {
		v := inp.Value()
		if e.inFilterPane && i == e.focusIdx {
			filterParts = append(filterParts, inp.View())
		} else if v != "" {
			filterParts = append(filterParts, styles.StatLabelStyle.Render(inp.Prompt)+styles.StatValueStyle.Render(v))
		} else if e.inFilterPane {
			filterParts = append(filterParts, inp.View())
		}
	}

	var filterBar string
	if e.inFilterPane {
		filterBar = strings.Join(filterParts, "  ")
		filterBar += "  " + styles.HelpStyle.Render("[enter] search  [esc] close  [ctrl+u] clear  [up/down] fields")
	} else if len(filterParts) > 0 {
		filterBar = strings.Join(filterParts, "  ") + "  " + styles.HelpStyle.Render("[/] filter")
	} else {
		filterBar = styles.HelpStyle.Render("  [/] open filter    [j/k] navigate    [enter] claim card    showing recent statements")
	}

	// Header
	header := fmt.Sprintf(" %-36s  %-28s  %-22s  %-14s  %-8s  %s",
		"SUBJECT", "PREDICATE", "OBJECT", "CONTEXT", "POL", "MAT")
	headerLine := styles.TableHeaderStyle.Width(w).Render(header)

	// Results
	var rows []string
	if e.loading {
		rows = append(rows, styles.HelpStyle.Render("  Loading..."))
	} else if len(e.statements) == 0 {
		rows = append(rows, styles.HelpStyle.Render("  No results. Press / to filter."))
	} else {
		maxRows := e.maxVisibleRows()
		end := e.scrollOffset + maxRows
		if end > len(e.statements) {
			end = len(e.statements)
		}

		for i := e.scrollOffset; i < end; i++ {
			s := e.statements[i]

			obj := ""
			if s.ObjectIRI != nil && *s.ObjectIRI != "" {
				obj = *s.ObjectIRI
			} else if s.ObjectLit != nil && *s.ObjectLit != "" {
				obj = *s.ObjectLit
				if len(obj) > 20 {
					obj = obj[:20] + "..."
				}
			}

			ctx := s.Context
			if len(ctx) > 14 {
				// Show last part after last /
				parts := strings.Split(ctx, "/")
				ctx = "…/" + parts[len(parts)-1]
				if len(ctx) > 14 {
					ctx = ctx[:11] + "..."
				}
			}

			matStr := fmt.Sprintf("L%d", s.Maturity)
			matColor := styles.MaturityColor(s.Maturity)

			row := fmt.Sprintf(" %-36s  %-28s  %-22s  %-14s  %-8s  %s",
				truncate(s.Subject, 36),
				truncate(s.Predicate, 28),
				truncate(obj, 22),
				ctx,
				polarityStyled(s.Polarity),
				lipgloss.NewStyle().Foreground(matColor).Render(matStr),
			)

			if i == e.cursor {
				row = styles.SelectedRowStyle.Width(w).Render(row)
			} else if i%2 == 1 {
				row = styles.TableRowAltStyle.Width(w).Render(row)
			} else {
				row = styles.TableRowStyle.Width(w).Render(row)
			}
			rows = append(rows, row)
		}

		// Scroll indicator
		if len(e.statements) > maxRows {
			pct := 0
			if len(e.statements) > 1 {
				pct = e.cursor * 100 / (len(e.statements) - 1)
			}
			rows = append(rows, styles.HelpStyle.Render(
				fmt.Sprintf("  %d/%d statements  (%d%%)", e.cursor+1, len(e.statements), pct),
			))
		}
	}

	title := styles.BoxTitleStyle.Render(fmt.Sprintf("Explorer (%d results)", e.total))
	content := lipgloss.JoinVertical(lipgloss.Left,
		title,
		filterBar,
		headerLine,
		strings.Join(rows, "\n"),
	)

	return content
}

func polarityStyled(p string) string {
	switch p {
	case "asserted":
		return styles.PolarityAsserted.Render(p)
	case "negated":
		return styles.PolarityNegated.Render(p)
	case "absent":
		return styles.PolarityAbsent.Render(p)
	case "unknown":
		return styles.PolarityUnknown.Render(p)
	default:
		return p
	}
}
