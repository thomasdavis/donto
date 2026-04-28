package components

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

// Gauge renders a horizontal bar gauge with a label and percentage.
// Width is the total character width of the bar (excluding label and suffix).
// Example output: "L0             ████████████░░░░  120 ( 78%)".
func Gauge(label string, value, max int64, width int, color lipgloss.Color) string {
	if max == 0 {
		max = 1
	}
	pct := float64(value) / float64(max)
	if pct > 1 {
		pct = 1
	}

	filled := int(pct * float64(width))
	if filled < 0 {
		filled = 0
	}
	empty := width - filled

	barStyle := lipgloss.NewStyle().Foreground(color)
	dimStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#45475A"))

	bar := barStyle.Render(strings.Repeat("█", filled)) +
		dimStyle.Render(strings.Repeat("░", empty))

	return fmt.Sprintf("%-14s %s %4d (%3.0f%%)", label, bar, value, pct*100)
}
