package views

import (
	"fmt"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/styles"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/components"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/model"
)

// DashboardDataMsg is kept for backward compat but individual msgs are preferred.
type DashboardDataMsg struct {
	Stats       model.DashboardStats
	Maturity    []model.MaturityBucket
	Polarity    []model.PolarityBucket
	Activity    []model.ActivityBucket
	Obligations []model.ObligationSummary
	PgOK        bool
	SrvOK       bool
}

type DashStatsMsg struct{ Stats model.DashboardStats }
type DashMaturityMsg struct{ Maturity []model.MaturityBucket }
type DashPolarityMsg struct{ Polarity []model.PolarityBucket }
type DashActivityMsg struct{ Activity []model.ActivityBucket }
type DashObligationsMsg struct{ Obligations []model.ObligationSummary }

type dashboardData struct {
	stats       model.DashboardStats
	maturity    []model.MaturityBucket
	polarity    []model.PolarityBucket
	activity    []model.ActivityBucket
	obligations []model.ObligationSummary
	hasStats    bool
}

// Dashboard is the Tab 1 view model.
type Dashboard struct {
	width, height int
	data          dashboardData
}

func NewDashboard() Dashboard {
	return Dashboard{}
}

func (d Dashboard) Init() tea.Cmd { return nil }

// The app sends individual messages per query so they arrive independently.
// We also handle the legacy all-in-one DashboardDataMsg.
func (d Dashboard) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case DashboardDataMsg:
		d.data = dashboardData{
			stats: msg.Stats, maturity: msg.Maturity, polarity: msg.Polarity,
			activity: msg.Activity, obligations: msg.Obligations, hasStats: true,
		}
	case DashStatsMsg:
		d.data.stats = msg.Stats
		d.data.hasStats = true
	case DashMaturityMsg:
		d.data.maturity = msg.Maturity
	case DashPolarityMsg:
		d.data.polarity = msg.Polarity
	case DashActivityMsg:
		d.data.activity = msg.Activity
	case DashObligationsMsg:
		d.data.obligations = msg.Obligations
	case tea.WindowSizeMsg:
		d.width = msg.Width
		d.height = msg.Height
	}
	return d, nil
}

func (d Dashboard) View() string {

	colW := d.width/2 - 2
	if colW < 30 {
		colW = 30
	}

	left := lipgloss.JoinVertical(lipgloss.Left,
		d.renderHealth(colW),
		d.renderTotals(colW),
		d.renderObligations(colW),
	)

	right := lipgloss.JoinVertical(lipgloss.Left,
		d.renderMaturity(colW),
		d.renderPolarity(colW),
		d.renderActivity(colW),
	)

	return lipgloss.JoinHorizontal(lipgloss.Top, left, " ", right)
}

func (d Dashboard) renderHealth(w int) string {
	pgStatus := styles.ActionAssertStyle.Render("OK")
	if !d.data.hasStats {
		pgStatus = styles.ActionRetractStyle.Render("DOWN")
	}
	srvStatus := styles.ActionAssertStyle.Render("OK")
	if !d.data.hasStats {
		srvStatus = styles.ActionRetractStyle.Render("DOWN")
	}

	content := fmt.Sprintf(
		"%s %s    %s %s",
		styles.StatLabelStyle.Render("Postgres:"), pgStatus,
		styles.StatLabelStyle.Render("Sidecar:"), srvStatus,
	)

	return styles.BoxStyle.Width(w).Render(
		styles.BoxTitleStyle.Render("Health") + "\n" + content,
	)
}

func (d Dashboard) renderTotals(w int) string {
	s := d.data.stats
	rows := []string{
		fmt.Sprintf("  %s %s", styles.StatLabelStyle.Render("Statements:"), styles.StatValueStyle.Render(fmt.Sprintf("%d", s.TotalStatements))),
		fmt.Sprintf("  %s %s", styles.StatLabelStyle.Render("Open:"), styles.StatValueStyle.Render(fmt.Sprintf("%d", s.OpenStatements))),
		fmt.Sprintf("  %s %s", styles.StatLabelStyle.Render("Retracted:"), styles.StatValueStyle.Render(fmt.Sprintf("%d", s.RetractedStatements))),
		fmt.Sprintf("  %s %s", styles.StatLabelStyle.Render("Contexts:"), styles.StatValueStyle.Render(fmt.Sprintf("%d", s.ContextCount))),
		fmt.Sprintf("  %s %s", styles.StatLabelStyle.Render("Predicates:"), styles.StatValueStyle.Render(fmt.Sprintf("%d", s.PredicateCount))),
		fmt.Sprintf("  %s %s", styles.StatLabelStyle.Render("Audit log:"), styles.StatValueStyle.Render(fmt.Sprintf("%d", s.AuditCount))),
	}

	return styles.BoxStyle.Width(w).Render(
		styles.BoxTitleStyle.Render("Totals") + "\n" + strings.Join(rows, "\n"),
	)
}

func (d Dashboard) renderMaturity(w int) string {
	if len(d.data.maturity) == 0 {
		return styles.BoxStyle.Width(w).Render(
			styles.BoxTitleStyle.Render("Maturity Distribution") + "\n" +
				styles.HelpStyle.Render("  No data"),
		)
	}

	// Aggregate by maturity level across all contexts
	agg := make(map[int]int64)
	var total int64
	for _, b := range d.data.maturity {
		agg[b.Maturity] += b.Count
		total += b.Count
	}

	labels := []string{"raw", "parsed", "linked", "reviewed", "certified"}
	colors := []lipgloss.Color{
		lipgloss.Color("#9CA3AF"),
		lipgloss.Color("#60A5FA"),
		lipgloss.Color("#34D399"),
		lipgloss.Color("#FBBF24"),
		lipgloss.Color("#A78BFA"),
	}

	barW := w - 28
	if barW < 10 {
		barW = 10
	}

	var rows []string
	for i := 0; i < 5; i++ {
		label := labels[i]
		count := agg[i]
		rows = append(rows, "  "+components.Gauge(label, count, total, barW, colors[i]))
	}

	return styles.BoxStyle.Width(w).Render(
		styles.BoxTitleStyle.Render("Maturity Distribution") + "\n" + strings.Join(rows, "\n"),
	)
}

func (d Dashboard) renderPolarity(w int) string {
	if len(d.data.polarity) == 0 {
		return styles.BoxStyle.Width(w).Render(
			styles.BoxTitleStyle.Render("Polarity") + "\n" + styles.HelpStyle.Render("  No data"),
		)
	}

	var total int64
	for _, b := range d.data.polarity {
		total += b.Count
	}

	barW := w - 28
	if barW < 10 {
		barW = 10
	}

	colorMap := map[string]lipgloss.Color{
		"asserted": lipgloss.Color("#22C55E"),
		"negated":  lipgloss.Color("#EF4444"),
		"absent":   lipgloss.Color("#6B7280"),
		"unknown":  lipgloss.Color("#EAB308"),
	}

	var rows []string
	for _, b := range d.data.polarity {
		c := colorMap[b.Polarity]
		if c == "" {
			c = styles.ColorFg
		}
		rows = append(rows, "  "+components.Gauge(b.Polarity, b.Count, total, barW, c))
	}

	return styles.BoxStyle.Width(w).Render(
		styles.BoxTitleStyle.Render("Polarity") + "\n" + strings.Join(rows, "\n"),
	)
}

func (d Dashboard) renderActivity(w int) string {
	sparkW := w - 6
	if sparkW < 10 {
		sparkW = 10
	}

	// Flatten activity buckets into hourly totals (24 hours)
	hourly := make([]int64, 24)
	for _, b := range d.data.activity {
		h := b.Bucket.Hour()
		hourly[h] += b.Count
	}

	spark := components.Sparkline(hourly[:], sparkW, styles.Highlight)

	return styles.BoxStyle.Width(w).Render(
		styles.BoxTitleStyle.Render("Activity (24h)") + "\n  " + spark,
	)
}

func (d Dashboard) renderObligations(w int) string {
	if len(d.data.obligations) == 0 {
		return styles.BoxStyle.Width(w).Render(
			styles.BoxTitleStyle.Render("Obligations") + "\n" +
				styles.HelpStyle.Render("  None"),
		)
	}

	var rows []string
	for _, o := range d.data.obligations {
		statusStyle := styles.StatValueStyle
		switch o.Status {
		case "pending":
			statusStyle = styles.ActionCorrectStyle
		case "discharged":
			statusStyle = styles.ActionAssertStyle
		case "failed":
			statusStyle = styles.ActionRetractStyle
		}
		rows = append(rows, fmt.Sprintf("  %-18s %s  %s",
			styles.StatLabelStyle.Render(o.Type),
			statusStyle.Render(o.Status),
			styles.StatValueStyle.Render(fmt.Sprintf("%d", o.Count)),
		))
	}

	return styles.BoxStyle.Width(w).Render(
		styles.BoxTitleStyle.Render("Obligations") + "\n" + strings.Join(rows, "\n"),
	)
}
