package components

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/lipgloss"
)

var (
	statusBarBg = lipgloss.Color("#2D1B69")
	statusOK    = lipgloss.Color("#10B981")
	statusFail  = lipgloss.Color("#EF4444")
	statusMuted = lipgloss.Color("#6B7280")
	statusAmber = lipgloss.Color("#F59E0B")
)

// StatusBar renders a full-width bottom status bar.
type StatusBar struct {
	Connected    bool
	SidecarUp    bool
	PollInterval time.Duration
	LastPoll     time.Time
	ActiveTab    int
}

// tabNames is the ordered list of tab labels. Add entries here when new
// views are introduced.
var tabNames = []string{"Dashboard", "Contexts", "Statements", "Audit"}

// View renders the status bar stretched to the given terminal width.
func (s StatusBar) View(width int) string {
	// ── Left: connection indicators ──
	pgIcon := lipgloss.NewStyle().Foreground(statusOK).Render("● PG")
	if !s.Connected {
		pgIcon = lipgloss.NewStyle().Foreground(statusFail).Render("✗ PG")
	}
	scIcon := lipgloss.NewStyle().Foreground(statusOK).Render("● Sidecar")
	if !s.SidecarUp {
		scIcon = lipgloss.NewStyle().Foreground(statusFail).Render("✗ Sidecar")
	}
	left := pgIcon + "  " + scIcon

	// ── Center: tab hints ──
	var tabs []string
	for i, name := range tabNames {
		label := fmt.Sprintf("[%d] %s", i+1, name)
		if i == s.ActiveTab {
			label = lipgloss.NewStyle().Foreground(statusAmber).Bold(true).Render(label)
		} else {
			label = lipgloss.NewStyle().Foreground(statusMuted).Render(label)
		}
		tabs = append(tabs, label)
	}
	center := strings.Join(tabs, "  ")

	// ── Right: poll info + help ──
	ago := "never"
	if !s.LastPoll.IsZero() {
		d := time.Since(s.LastPoll).Truncate(time.Second)
		ago = d.String() + " ago"
	}
	right := lipgloss.NewStyle().Foreground(statusMuted).Render(
		fmt.Sprintf("poll %s  last %s  q:quit ?:help", s.PollInterval, ago),
	)

	// ── Compose ──
	// We compute the visible (un-styled) widths to figure out padding.
	leftPlain := plainLen(left)
	centerPlain := plainLen(center)
	rightPlain := plainLen(right)

	padTotal := width - leftPlain - centerPlain - rightPlain
	if padTotal < 2 {
		padTotal = 2
	}
	padLeft := padTotal / 2
	padRight := padTotal - padLeft

	bar := left + strings.Repeat(" ", padLeft) + center + strings.Repeat(" ", padRight) + right

	return lipgloss.NewStyle().
		Width(width).
		Background(statusBarBg).
		Foreground(lipgloss.Color("#E5E7EB")).
		Render(bar)
}

// plainLen returns the visible length of a string after stripping ANSI
// escape sequences.
func plainLen(s string) int {
	n := 0
	inEsc := false
	for _, r := range s {
		if inEsc {
			if (r >= 'A' && r <= 'Z') || (r >= 'a' && r <= 'z') || r == '~' {
				inEsc = false
			}
			continue
		}
		if r == '\x1b' {
			inEsc = true
			continue
		}
		n++
	}
	return n
}
