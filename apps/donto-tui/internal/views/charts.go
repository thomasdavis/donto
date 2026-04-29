package views

import (
	"fmt"
	"math"
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
		"Statement Growth (14 days)",
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

// renderGrowth draws a stacked vertical bar chart for the last 14 days.
func (c Charts) renderGrowth() string {
	if len(c.growth) == 0 {
		return styles.HelpStyle.Render("  No growth data available.")
	}

	yAxisW := 8
	chartW := c.width - yAxisW - 2
	if chartW < 14 {
		chartW = 14
	}
	barW := chartW / 14
	if barW < 3 {
		barW = 3
	}
	chartW = barW * 14 // snap to exact multiple

	barH := c.height - 8 // title, nav, x-axis, hints, padding
	if barH < 5 {
		barH = 5
	}

	// Find max total for normalization.
	var maxTotal int64
	for _, d := range c.growth {
		total := d.Asserts + d.Retracts + d.Corrects
		if total > maxTotal {
			maxTotal = total
		}
	}
	if maxTotal == 0 {
		return styles.HelpStyle.Render("  All days have zero activity.")
	}

	greenStyle := lipgloss.NewStyle().Foreground(styles.ColorGreen)
	redStyle := lipgloss.NewStyle().Foreground(styles.ColorRed)
	yellowStyle := lipgloss.NewStyle().Foreground(styles.ColorAmber)
	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#313244"))

	// Build a 2D grid: rows (top=0) x cols
	type cell struct {
		char  string
		style lipgloss.Style
	}
	grid := make([][]cell, barH)
	for r := range grid {
		grid[r] = make([]cell, chartW)
		for col := range grid[r] {
			grid[r][col] = cell{char: " ", style: dimStyle}
		}
	}

	// For each day, compute segment heights and fill the grid bottom-up.
	for di, day := range c.growth {
		total := day.Asserts + day.Retracts + day.Corrects
		if total == 0 {
			continue
		}
		fullH := float64(barH) * float64(total) / float64(maxTotal)
		assertH := float64(barH) * float64(day.Asserts) / float64(maxTotal)
		retractH := float64(barH) * float64(day.Retracts) / float64(maxTotal)
		correctH := fullH - assertH - retractH

		// Segments drawn bottom-up: asserts, retracts, corrects
		segments := []struct {
			h     float64
			style lipgloss.Style
		}{
			{assertH, greenStyle},
			{retractH, redStyle},
			{correctH, yellowStyle},
		}

		colStart := di * barW
		colEnd := colStart + barW - 1 // leave 1 col gap between bars
		if colEnd >= chartW {
			colEnd = chartW - 1
		}

		row := barH - 1 // start from bottom
		for _, seg := range segments {
			full := int(seg.h)
			frac := seg.h - float64(full)

			for f := 0; f < full && row >= 0; f++ {
				for col := colStart; col <= colEnd; col++ {
					grid[row][col] = cell{char: "█", style: seg.style}
				}
				row--
			}
			// Half block for fractional part
			if frac >= 0.5 && row >= 0 {
				for col := colStart; col <= colEnd; col++ {
					grid[row][col] = cell{char: "▄", style: seg.style}
				}
				row--
			}
		}
	}

	// Render grid with y-axis labels.
	var sb strings.Builder
	for r := 0; r < barH; r++ {
		// Y-axis label: show value at this row level
		val := int64(math.Round(float64(maxTotal) * float64(barH-r) / float64(barH)))
		label := ""
		if r == 0 {
			label = fmt.Sprintf("%7d", val)
		} else if r == barH/2 {
			label = fmt.Sprintf("%7d", val)
		} else if r == barH-1 {
			label = fmt.Sprintf("%7d", int64(0))
		} else {
			label = "       "
		}
		sb.WriteString(styles.StatLabelStyle.Render(label) + " ")

		for col := 0; col < chartW; col++ {
			c := grid[r][col]
			sb.WriteString(c.style.Render(c.char))
		}
		sb.WriteString("\n")
	}

	// X-axis labels
	sb.WriteString(strings.Repeat(" ", yAxisW))
	for _, day := range c.growth {
		lbl := day.Day.Format("Jan 2")
		if len(lbl) > barW {
			lbl = day.Day.Format("1/2")
		}
		// Pad or truncate to barW
		if len(lbl) < barW {
			lbl = lbl + strings.Repeat(" ", barW-len(lbl))
		} else {
			lbl = lbl[:barW]
		}
		sb.WriteString(styles.StatLabelStyle.Render(lbl))
	}
	sb.WriteString("\n")

	// Legend
	legend := fmt.Sprintf("  %s assert  %s retract  %s correct",
		greenStyle.Render("█"),
		redStyle.Render("█"),
		yellowStyle.Render("█"),
	)
	sb.WriteString(legend)

	return sb.String()
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
