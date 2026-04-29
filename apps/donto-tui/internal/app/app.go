package app

import (
	"context"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/db"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/model"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/styles"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/views"
)

const (
	tabDashboard = iota
	tabFirehose
	tabExplorer
	tabContexts
	tabClaimCard
	tabCharts
	tabCount
)

var tabNames = [tabCount]string{"Dashboard", "Firehose", "Explorer", "Contexts", "Card", "Charts"}

type Model struct {
	pool         *pgxpool.Pool
	pollInterval time.Duration
	srvURL       string

	activeTab int
	width     int
	height    int

	connected bool
	sidecarUp bool
	lastPoll  time.Time
	showHelp  bool

	dashboard views.Dashboard
	firehose  views.Firehose
	explorer  views.Explorer
	contexts  views.Contexts
	claimcard views.ClaimCard
	charts    views.Charts
}

func New(pool *pgxpool.Pool, poll time.Duration, srvURL string) Model {
	return Model{
		pool:         pool,
		pollInterval: poll,
		srvURL:       srvURL,
		dashboard:    views.NewDashboard(),
		firehose:     views.NewFirehose(),
		explorer:     views.NewExplorer(),
		contexts:     views.NewContexts(),
		claimcard:    views.NewClaimCard(),
		charts:       views.NewCharts(),
	}
}

type tickMsg time.Time
type healthMsg struct{ pgOK, srvOK bool }

var lastAuditID int64 = -1 // -1 means "not initialized yet"

func (m Model) tick() tea.Cmd {
	return tea.Tick(m.pollInterval, func(t time.Time) tea.Msg { return tickMsg(t) })
}

func (m Model) Init() tea.Cmd {
	return tea.Batch(m.tick(), m.pollActiveTab(), m.explorer.Init())
}

func (m Model) updateView(msg tea.Msg) (Model, tea.Cmd) {
	var cmd tea.Cmd
	var updated tea.Model
	switch m.activeTab {
	case tabDashboard:
		updated, cmd = m.dashboard.Update(msg)
		m.dashboard = updated.(views.Dashboard)
	case tabFirehose:
		updated, cmd = m.firehose.Update(msg)
		m.firehose = updated.(views.Firehose)
	case tabExplorer:
		updated, cmd = m.explorer.Update(msg)
		m.explorer = updated.(views.Explorer)
	case tabContexts:
		updated, cmd = m.contexts.Update(msg)
		m.contexts = updated.(views.Contexts)
	case tabClaimCard:
		updated, cmd = m.claimcard.Update(msg)
		m.claimcard = updated.(views.ClaimCard)
	case tabCharts:
		updated, cmd = m.charts.Update(msg)
		m.charts = updated.(views.Charts)
	}
	return m, cmd
}

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.KeyMsg:
		if m.showHelp {
			m.showHelp = false
			return m, nil
		}
		switch msg.String() {
		case "q", "ctrl+c":
			return m, tea.Quit
		case "?":
			m.showHelp = !m.showHelp
			return m, nil
		case "1":
			m.activeTab = tabDashboard
			return m, m.pollActiveTab()
		case "2":
			m.activeTab = tabFirehose
			return m, nil
		case "3":
			m.activeTab = tabExplorer
			return m, m.pollActiveTab()
		case "4":
			m.activeTab = tabContexts
			return m, m.pollActiveTab()
		case "5":
			m.activeTab = tabClaimCard
			return m, nil
		case "6":
			m.activeTab = tabCharts
			return m, m.pollActiveTab()
		case "tab":
			m.activeTab = (m.activeTab + 1) % tabCount
			return m, m.pollActiveTab()
		case "shift+tab":
			m.activeTab = (m.activeTab - 1 + tabCount) % tabCount
			return m, m.pollActiveTab()
		}

	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		// Forward to all views so they can store dimensions
		for _, tab := range []int{tabDashboard, tabFirehose, tabExplorer, tabContexts, tabClaimCard, tabCharts} {
			prev := m.activeTab
			m.activeTab = tab
			m, _ = m.updateView(msg)
			m.activeTab = prev
		}
		return m, nil

	case tickMsg:
		m.lastPoll = time.Time(msg)
		cmds := []tea.Cmd{m.tick(), m.pollActiveTab(), m.checkHealth()}
		// Always poll audit + live queries for firehose
		cmds = append(cmds, m.pollAudit(), m.pollLiveQueries())
		return m, tea.Batch(cmds...)

	case healthMsg:
		m.connected = msg.pgOK
		m.sidecarUp = msg.srvOK
		return m, nil

	case db.AuditNotification:
		updated, cmd := m.firehose.Update(views.FirehoseEntryMsg{Entry: msg.Entry})
		m.firehose = updated.(views.Firehose)
		if msg.Entry.AuditID > lastAuditID {
			lastAuditID = msg.Entry.AuditID
		}
		return m, cmd

	case views.FirehoseActivityMsg:
		updated, _ := m.firehose.Update(msg)
		m.firehose = updated.(views.Firehose)
		return m, nil

	case views.FirehoseBatchMsg:
		updated, cmd := m.firehose.Update(msg)
		m.firehose = updated.(views.Firehose)
		for _, e := range msg.Entries {
			if e.AuditID > lastAuditID {
				lastAuditID = e.AuditID
			}
		}
		return m, cmd

	case views.SelectStatementMsg:
		m.activeTab = tabClaimCard
		return m, m.fetchClaimCard(msg.StatementID)

	case views.ExplorerSearchMsg:
		return m, m.fetchStatements(msg)

	case views.ChartsGrowthMsg:
		updated, cmd := m.charts.Update(msg)
		m.charts = updated.(views.Charts)
		return m, cmd
	case views.ChartsContextsMsg:
		updated, cmd := m.charts.Update(msg)
		m.charts = updated.(views.Charts)
		return m, cmd
	case views.ChartsPredicatesMsg:
		updated, cmd := m.charts.Update(msg)
		m.charts = updated.(views.Charts)
		return m, cmd
	}

	m, cmd := m.updateView(msg)
	return m, cmd
}

func (m Model) View() string {
	if m.width == 0 {
		return "loading..."
	}

	tabs := m.renderTabs()

	var content string
	switch m.activeTab {
	case tabDashboard:
		content = m.dashboard.View()
	case tabFirehose:
		content = m.firehose.View()
	case tabExplorer:
		content = m.explorer.View()
	case tabContexts:
		content = m.contexts.View()
	case tabClaimCard:
		content = m.claimcard.View()
	case tabCharts:
		content = m.charts.View()
	}

	status := m.renderStatusBar()

	if m.showHelp {
		contentHeight := m.height - 4
		content = m.renderHelp(m.width, contentHeight)
	}

	return lipgloss.JoinVertical(lipgloss.Left, tabs, content, status)
}

func (m Model) renderTabs() string {
	var tabs []string
	for i, name := range tabNames {
		if i == m.activeTab {
			tabs = append(tabs, styles.ActiveTabStyle.Render(name))
		} else {
			tabs = append(tabs, styles.TabStyle.Render(name))
		}
	}
	row := lipgloss.JoinHorizontal(lipgloss.Top, tabs...)
	return styles.TitleStyle.Width(m.width).Render(
		lipgloss.NewStyle().Bold(true).Foreground(styles.Highlight).Render(" donto ") + "  " + row,
	)
}

func (m Model) renderStatusBar() string {
	pgStatus := "CONNECTED"
	if !m.connected {
		pgStatus = "DISCONNECTED"
	}
	srvStatus := "UP"
	if !m.sidecarUp {
		srvStatus = "DOWN"
	}
	ago := time.Since(m.lastPoll).Truncate(time.Second)

	left := lipgloss.NewStyle().Foreground(styles.Subtle).Render(
		"  pg:" + pgStatus + "  srv:" + srvStatus + "  poll:" + ago.String() + " ago",
	)
	right := lipgloss.NewStyle().Foreground(styles.Subtle).Render(
		"q:quit  ?:help  1-6:tabs  ",
	)
	gap := m.width - lipgloss.Width(left) - lipgloss.Width(right)
	if gap < 0 {
		gap = 0
	}
	return styles.StatusBarStyle.Width(m.width).Render(
		left + lipgloss.NewStyle().Width(gap).Render("") + right,
	)
}

func (m Model) renderHelp(w, h int) string {
	help := `
  Keybindings

  1-6          Switch tabs
  Tab          Next tab
  Shift+Tab    Previous tab
  q / Ctrl+C   Quit
  ?            Toggle help

  Firehose:
  a            Filter by action
  p            Pause / resume

  Explorer:
  Tab          Switch pane
  Enter        Search / select
  j/k          Navigate results

  Charts:
  ←/→ or h/l   Cycle charts
`
	return lipgloss.NewStyle().
		Width(w).Height(h).
		Align(lipgloss.Center, lipgloss.Center).
		Foreground(styles.Highlight).
		Render(help)
}

func (m Model) checkHealth() tea.Cmd {
	pool := m.pool
	srvURL := m.srvURL
	return func() tea.Msg {
		pgOK := db.Ping(context.Background(), pool) == nil
		srvOK := db.CheckSidecar(srvURL)
		return healthMsg{pgOK: pgOK, srvOK: srvOK}
	}
}

func (m Model) pollActiveTab() tea.Cmd {
	pool := m.pool
	switch m.activeTab {
	case tabDashboard:
		return tea.Batch(
			func() tea.Msg {
				stats, _ := db.FetchDashboard(context.Background(), pool)
				var s model.DashboardStats
				if stats != nil {
					s = *stats
				}
				return views.DashStatsMsg{Stats: s}
			},
			func() tea.Msg {
				m, _ := db.FetchMaturity(context.Background(), pool)
				return views.DashMaturityMsg{Maturity: m}
			},
			func() tea.Msg {
				p, _ := db.FetchPolarity(context.Background(), pool)
				return views.DashPolarityMsg{Polarity: p}
			},
			func() tea.Msg {
				a, _ := db.FetchActivity(context.Background(), pool)
				return views.DashActivityMsg{Activity: a}
			},
			func() tea.Msg {
				o, _ := db.FetchObligations(context.Background(), pool)
				return views.DashObligationsMsg{Obligations: o}
			},
		)
	case tabExplorer:
		return func() tea.Msg {
			stmts, _ := db.FetchRecentStatements(context.Background(), pool, 200)
			return views.ExplorerResultsMsg{Statements: stmts, Total: len(stmts)}
		}
	case tabContexts:
		return func() tea.Msg {
			contexts, _ := db.FetchContexts(context.Background(), pool)
			return views.ContextsDataMsg{Contexts: contexts}
		}
	case tabCharts:
		return tea.Batch(
			func() tea.Msg {
				days, _ := db.FetchGrowth(context.Background(), pool)
				return views.ChartsGrowthMsg{Days: days}
			},
			func() tea.Msg {
				ctxs, _ := db.FetchTopContexts(context.Background(), pool, 10)
				return views.ChartsContextsMsg{Contexts: ctxs}
			},
			func() tea.Msg {
				preds, _ := db.FetchTopPredicates(context.Background(), pool, 15)
				return views.ChartsPredicatesMsg{Predicates: preds}
			},
		)
	}
	return nil
}

func (m Model) pollLiveQueries() tea.Cmd {
	pool := m.pool
	return func() tea.Msg {
		acts, _ := db.FetchActivity_PG(context.Background(), pool)
		return views.FirehoseActivityMsg{Active: acts}
	}
}

func (m Model) pollAudit() tea.Cmd {
	pool := m.pool
	after := lastAuditID
	return func() tea.Msg {
		if after == -1 {
			// First poll: just learn the current max, don't show backlog
			var maxID int64
			db.QueryVal(context.Background(), pool, `SELECT coalesce(max(audit_id),0) FROM donto_audit`, &maxID)
			lastAuditID = maxID
			return nil
		}
		entries, _ := db.FetchRecentAudit(context.Background(), pool, after, 100)
		if len(entries) == 0 {
			return nil
		}
		return views.FirehoseBatchMsg{Entries: entries}
	}
}

func (m Model) fetchStatements(search views.ExplorerSearchMsg) tea.Cmd {
	pool := m.pool
	return func() tea.Msg {
		ctx := context.Background()
		var stmts []model.Statement
		switch {
		case search.Subject != "":
			stmts, _ = db.FetchStatementsBySubject(ctx, pool, search.Subject, 200)
		case search.Predicate != "":
			stmts, _ = db.FetchStatementsByPredicate(ctx, pool, search.Predicate, 200)
		case search.Context != "":
			stmts, _ = db.FetchStatementsByContext(ctx, pool, search.Context, 200)
		default:
			stmts, _ = db.FetchRecentStatements(ctx, pool, 200)
		}
		return views.ExplorerResultsMsg{Statements: stmts, Total: len(stmts)}
	}
}

func (m Model) fetchClaimCard(stmtID string) tea.Cmd {
	pool := m.pool
	return func() tea.Msg {
		j, _ := db.FetchClaimCard(context.Background(), pool, stmtID)
		return views.ClaimCardDataMsg{JSON: j, StmtID: stmtID}
	}
}
