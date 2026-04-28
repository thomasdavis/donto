package styles

import "github.com/charmbracelet/lipgloss"

// ── Color palette ──────────────────────────────────────────────────────

var (
	ColorIndigo  = lipgloss.Color("#2D1B69") // borders
	ColorAmber   = lipgloss.Color("#F59E0B") // highlights / selected
	ColorGreen   = lipgloss.Color("#10B981") // asserted polarity
	ColorRed     = lipgloss.Color("#EF4444") // negated polarity
	ColorGray    = lipgloss.Color("#6B7280") // absent polarity / muted
	ColorPurple  = lipgloss.Color("#8B5CF6") // unknown polarity
	ColorPrimary = lipgloss.Color("#7C3AED")
	ColorFg      = lipgloss.Color("#CDD6F4")
	ColorBorder  = lipgloss.Color("#45475A")

	Highlight = ColorAmber
	Subtle    = ColorGray

	// Maturity gradient: L0 gray → L1 blue → L2 teal → L3 gold → L4 bright gold
	colorMaturityL0 = lipgloss.Color("#6B7280")
	colorMaturityL1 = lipgloss.Color("#3B82F6")
	colorMaturityL2 = lipgloss.Color("#14B8A6")
	colorMaturityL3 = lipgloss.Color("#D97706")
	colorMaturityL4 = lipgloss.Color("#FBBF24")
)

// ── Polarity / maturity helpers ────────────────────────────────────────

// PolarityColor returns the palette color for a polarity string.
func PolarityColor(pol string) lipgloss.Color {
	switch pol {
	case "asserted":
		return ColorGreen
	case "negated":
		return ColorRed
	case "absent":
		return ColorGray
	case "unknown":
		return ColorPurple
	default:
		return ColorGray
	}
}

// MaturityColor returns the palette color for a maturity level (0-4).
func MaturityColor(level int) lipgloss.Color {
	switch level {
	case 0:
		return colorMaturityL0
	case 1:
		return colorMaturityL1
	case 2:
		return colorMaturityL2
	case 3:
		return colorMaturityL3
	case 4:
		return colorMaturityL4
	default:
		if level < 0 {
			return colorMaturityL0
		}
		return colorMaturityL4
	}
}

// ── Shared styles ──────────────────────────────────────────────────────

var (
	// BorderStyle is used for panel frames.
	BorderStyle = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(ColorIndigo)

	// BoxStyle is a bordered box with padding.
	BoxStyle = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(ColorBorder).
			Padding(0, 1)

	// TitleStyle is used for section headings.
	TitleStyle = lipgloss.NewStyle().
			Bold(true).
			Foreground(ColorAmber).
			PaddingLeft(1).
			PaddingRight(1)

	// BoxTitleStyle is used for box headers.
	BoxTitleStyle = lipgloss.NewStyle().
			Bold(true).
			Foreground(ColorPrimary).
			MarginBottom(1)

	// TabStyle is the default (inactive) tab.
	TabStyle = lipgloss.NewStyle().
			Padding(0, 2).
			Foreground(ColorGray)

	// ActiveTabStyle is the currently-selected tab.
	ActiveTabStyle = lipgloss.NewStyle().
			Padding(0, 2).
			Bold(true).
			Foreground(ColorAmber).
			Underline(true)

	// StatusBarStyle is the background strip for the bottom bar.
	StatusBarStyle = lipgloss.NewStyle().
			Background(ColorIndigo).
			Foreground(lipgloss.Color("#E5E7EB")).
			Padding(0, 1)

	// HelpStyle is used for key-binding hints.
	HelpStyle = lipgloss.NewStyle().
			Foreground(ColorGray).
			Italic(true)

	// Table styles.
	TableHeaderStyle = lipgloss.NewStyle().
				Bold(true).
				Foreground(lipgloss.Color("#06B6D4")).
				BorderBottom(true).
				BorderStyle(lipgloss.NormalBorder()).
				BorderForeground(ColorBorder)

	TableRowStyle    = lipgloss.NewStyle().Foreground(ColorFg)
	TableRowAltStyle = lipgloss.NewStyle().Foreground(ColorGray)

	// Stat styles.
	StatLabelStyle = lipgloss.NewStyle().Foreground(ColorGray)
	StatValueStyle = lipgloss.NewStyle().Bold(true).Foreground(ColorFg)

	// Action styles.
	ActionAssertStyle  = lipgloss.NewStyle().Foreground(ColorGreen)
	ActionRetractStyle = lipgloss.NewStyle().Foreground(ColorRed)
	ActionCorrectStyle = lipgloss.NewStyle().Foreground(ColorAmber)

	// Polarity label styles.
	PolarityAsserted = lipgloss.NewStyle().Foreground(ColorGreen)
	PolarityNegated  = lipgloss.NewStyle().Foreground(ColorRed)
	PolarityAbsent   = lipgloss.NewStyle().Foreground(ColorGray)
	PolarityUnknown  = lipgloss.NewStyle().Foreground(ColorPurple)

	// SelectedRowStyle highlights the focused row.
	SelectedRowStyle = lipgloss.NewStyle().
				Background(lipgloss.Color("#313244")).
				Foreground(lipgloss.Color("#F5C2E7"))
)
