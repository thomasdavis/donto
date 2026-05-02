package views

import (
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/components"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/model"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/styles"
)

// Message types for chart data delivery.
type ChartsGrowthMsg struct{ Days []model.GrowthDay }
type ChartsContextsMsg struct{ Contexts []model.ContextBar }
type ChartsPredicatesMsg struct{ Predicates []model.PredicateBar }

const chartCount = 3

// Charts is the Tab 6 multi-chart view.
type Charts struct {
	width, height int
	chartIdx      int
	growth        []model.GrowthDay
	contexts      []model.ContextBar
	predicates    []model.PredicateBar
}

func NewCharts() Charts {
	return Charts{}
}

func (c Charts) Init() tea.Cmd { return nil }

func (c Charts) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case ChartsGrowthMsg:
		c.growth = msg.Days
	case ChartsContextsMsg:
		c.contexts = msg.Contexts
	case ChartsPredicatesMsg:
		c.predicates = msg.Predicates
	case tea.WindowSizeMsg:
		c.width = msg.Width
		c.height = msg.Height
	case tea.KeyMsg:
		switch msg.String() {
		case "left", "h":
			c.chartIdx = (c.chartIdx - 1 + chartCount) % chartCount
		case "right", "l":
			c.chartIdx = (c.chartIdx + 1) % chartCount
		}
	}
	return c, nil
}

func (c Charts) View() string {
	if c.width == 0 {
		return "loading..."
	}

	titles := [chartCount]string{
		"Statement Growth (7 days)",
		"Top 10 Contexts by Size",
		"Predicate Usage (Top 15)",
	}

	// Navigation indicator
	nav := fmt.Sprintf("  %s  %d/%d  %s",
		styles.HelpStyle.Render("◀"),
		c.chartIdx+1, chartCount,
		styles.HelpStyle.Render("▶"),
	)
	title := styles.BoxTitleStyle.Render("  "+titles[c.chartIdx]) + "    " + nav
	hint := styles.HelpStyle.Render("  ←/→ or h/l: cycle charts")

	var body string
	switch c.chartIdx {
	case 0:
		body = c.renderGrowth()
	case 1:
		body = c.renderContexts()
	case 2:
		body = c.renderPredicates()
	}

	return lipgloss.JoinVertical(lipgloss.Left, title, "", body, "", hint)
}

// renderGrowth draws a cumulative total growth sparkline over the last 14 days.
func (c Charts) renderGrowth() string {
	if len(c.growth) == 0 {
		return styles.HelpStyle.Render("  No growth data available.")
	}

	// Build cumulative totals
	cumulative := make([]int64, len(c.growth))
	var running int64
	for i, d := range c.growth {
		running += d.Asserts + d.Retracts + d.Corrects
		cumulative[i] = running
	}

	if running == 0 {
		return styles.HelpStyle.Render("  All days have zero activity.")
	}

	chartW := c.width - 4
	if chartW < 20 {
		chartW = 20
	}
	chartH := c.height - 10
	if chartH < 5 {
		chartH = 5
	}

	lineColor := styles.ColorGreen
	dotColor := lipgloss.Color("#F59E0B")
	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#45475A"))
	lineStyle := lipgloss.NewStyle().Foreground(lineColor)
	fillStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#1E3A2F"))

	maxVal := cumulative[len(cumulative)-1]
	if maxVal == 0 {
		maxVal = 1
	}

	// Build grid
	type cell struct {
		ch    string
		style lipgloss.Style
	}
	grid := make([][]cell, chartH)
	for r := range grid {
		grid[r] = make([]cell, chartW)
		for col := range grid[r] {
			grid[r][col] = cell{ch: " ", style: dimStyle}
		}
	}

	// Plot the line and fill area below it
	colStep := float64(chartW) / float64(len(cumulative))
	for i, val := range cumulative {
		col := int(float64(i)*colStep + colStep/2)
		if col >= chartW {
			col = chartW - 1
		}
		lineRow := chartH - 1 - int(float64(chartH-1)*float64(val)/float64(maxVal))
		if lineRow < 0 {
			lineRow = 0
		}

		// Fill below the line
		for r := lineRow + 1; r < chartH; r++ {
			if grid[r][col].ch == " " {
				grid[r][col] = cell{ch: "░", style: fillStyle}
			}
		}
		// Draw the line point
		grid[lineRow][col] = cell{ch: "█", style: lineStyle}

		// Connect to next point
		if i < len(cumulative)-1 {
			nextCol := int(float64(i+1)*colStep + colStep/2)
			if nextCol >= chartW {
				nextCol = chartW - 1
			}
			nextVal := cumulative[i+1]
			nextRow := chartH - 1 - int(float64(chartH-1)*float64(nextVal)/float64(maxVal))
			if nextRow < 0 {
				nextRow = 0
			}
			// Draw connecting segments between columns
			for cc := col + 1; cc < nextCol; cc++ {
				frac := float64(cc-col) / float64(nextCol-col)
				interpRow := lineRow + int(frac*float64(nextRow-lineRow))
				if interpRow >= 0 && interpRow < chartH {
					grid[interpRow][cc] = cell{ch: "─", style: lineStyle}
					for r := interpRow + 1; r < chartH; r++ {
						if grid[r][cc].ch == " " {
							grid[r][cc] = cell{ch: "░", style: fillStyle}
						}
					}
				}
			}
		}
	}

	// Render
	var sb strings.Builder

	for r := 0; r < chartH; r++ {
		// Y-axis
		label := "       "
		if r == 0 {
			label = fmtCount(maxVal)
		} else if r == chartH/2 {
			label = fmtCount(maxVal / 2)
		} else if r == chartH-1 {
			label = fmtCount(0)
		}
		sb.WriteString(styles.StatLabelStyle.Render(label) + " ")
		for col := 0; col < chartW; col++ {
			c := grid[r][col]
			sb.WriteString(c.style.Render(c.ch))
		}
		sb.WriteString("\n")
	}

	// X-axis dates
	sb.WriteString("        ")
	step := len(c.growth) / 7
	if step < 1 {
		step = 1
	}
	labelW := chartW / len(c.growth)
	for i, d := range c.growth {
		if i%step == 0 {
			lbl := d.Day.Format("Jan 2")
			pad := labelW*step - len(lbl)
			if pad < 1 {
				pad = 1
			}
			sb.WriteString(styles.StatLabelStyle.Render(lbl) + strings.Repeat(" ", pad))
		}
	}
	sb.WriteString("\n\n")

	// Summary line
	var totalA, totalR, totalC int64
	for _, d := range c.growth {
		totalA += d.Asserts
		totalR += d.Retracts
		totalC += d.Corrects
	}
	summary := fmt.Sprintf("  Total: %s   %s asserts  %s retracts  %s corrects",
		lipgloss.NewStyle().Bold(true).Foreground(dotColor).Render(fmtCount(running)),
		lipgloss.NewStyle().Foreground(styles.ColorGreen).Render(fmtCount(totalA)),
		lipgloss.NewStyle().Foreground(styles.ColorRed).Render(fmtCount(totalR)),
		lipgloss.NewStyle().Foreground(styles.ColorAmber).Render(fmtCount(totalC)),
	)
	sb.WriteString(summary)

	return sb.String()
}

func fmtCount(n int64) string {
	switch {
	case n >= 1_000_000:
		return fmt.Sprintf("%5.1fM", float64(n)/1_000_000)
	case n >= 1_000:
		return fmt.Sprintf("%5.1fK", float64(n)/1_000)
	default:
		return fmt.Sprintf("%6d", n)
	}
}

// renderContexts draws horizontal bars for top 10 contexts.
func (c Charts) renderContexts() string {
	if len(c.contexts) == 0 {
		return styles.HelpStyle.Render("  No context data available.")
	}

	var maxCount int64
	for _, ctx := range c.contexts {
		if ctx.Count > maxCount {
			maxCount = ctx.Count
		}
	}

	barW := c.width - 36
	if barW < 10 {
		barW = 10
	}

	kindColor := func(kind string) lipgloss.Color {
		switch kind {
		case "source":
			return styles.ColorGreen
		case "derivation":
			return lipgloss.Color("#3B82F6")
		case "hypothesis":
			return styles.ColorPurple
		case "reference":
			return lipgloss.Color("#06B6D4")
		default:
			return styles.ColorGray
		}
	}

	var sb strings.Builder
	for _, ctx := range c.contexts {
		iri := ctx.IRI
		if len(iri) > 14 {
			iri = iri[len(iri)-14:]
		}
		sb.WriteString("  " + components.Gauge(iri, ctx.Count, maxCount, barW, kindColor(ctx.Kind)) + "\n")
	}

	// Legend
	sb.WriteString("\n")
	sb.WriteString(fmt.Sprintf("  %s source  %s derivation  %s hypothesis  %s reference  %s other",
		lipgloss.NewStyle().Foreground(styles.ColorGreen).Render("█"),
		lipgloss.NewStyle().Foreground(lipgloss.Color("#3B82F6")).Render("█"),
		lipgloss.NewStyle().Foreground(styles.ColorPurple).Render("█"),
		lipgloss.NewStyle().Foreground(lipgloss.Color("#06B6D4")).Render("█"),
		lipgloss.NewStyle().Foreground(styles.ColorGray).Render("█"),
	))

	return sb.String()
}

// renderPredicates draws horizontal bars for top 15 predicates.
func (c Charts) renderPredicates() string {
	if len(c.predicates) == 0 {
		return styles.HelpStyle.Render("  No predicate data available.")
	}

	var maxCount int64
	for _, p := range c.predicates {
		if p.Count > maxCount {
			maxCount = p.Count
		}
	}

	barW := c.width - 36
	if barW < 10 {
		barW = 10
	}

	colors := []lipgloss.Color{
		lipgloss.Color("#06B6D4"),
		lipgloss.Color("#3B82F6"),
		lipgloss.Color("#8B5CF6"),
		lipgloss.Color("#A78BFA"),
		lipgloss.Color("#C4B5FD"),
	}

	var sb strings.Builder
	for i, p := range c.predicates {
		pred := p.Predicate
		if len(pred) > 14 {
			pred = pred[len(pred)-14:]
		}
		color := colors[i%len(colors)]
		sb.WriteString("  " + components.Gauge(pred, p.Count, maxCount, barW, color) + "\n")
	}

	return sb.String()
}
