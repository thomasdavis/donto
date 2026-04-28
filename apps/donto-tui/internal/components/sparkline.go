package components

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
)

// blocks maps a quantized level (0-7) to a Unicode block character.
var blocks = []rune{'▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'}

// Sparkline renders a single-line sparkline from a slice of int64 values.
// Width controls the number of columns; data is resampled to fit. The
// rendered bar is colored with the given lipgloss Color.
func Sparkline(values []int64, width int, color lipgloss.Color) string {
	if len(values) == 0 || width == 0 {
		return ""
	}

	data := resample(values, width)

	var maxVal int64
	for _, v := range data {
		if v > maxVal {
			maxVal = v
		}
	}
	if maxVal == 0 {
		maxVal = 1
	}

	style := lipgloss.NewStyle().Foreground(color)
	var b strings.Builder
	for _, v := range data {
		idx := int(v * int64(len(blocks)-1) / maxVal)
		if idx < 0 {
			idx = 0
		}
		if idx >= len(blocks) {
			idx = len(blocks) - 1
		}
		b.WriteRune(blocks[idx])
	}
	return style.Render(b.String())
}

// resample maps values into exactly width buckets, averaging when shrinking
// or left-padding with zeros when growing.
func resample(values []int64, width int) []int64 {
	n := len(values)
	if n <= width {
		out := make([]int64, width)
		copy(out[width-n:], values)
		return out
	}
	out := make([]int64, width)
	for i := range width {
		start := i * n / width
		end := (i + 1) * n / width
		if end > n {
			end = n
		}
		var sum int64
		for j := start; j < end; j++ {
			sum += values[j]
		}
		if end > start {
			out[i] = sum / int64(end-start)
		}
	}
	return out
}
