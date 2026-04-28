package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"os"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/app"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/db"
)

func main() {
	dsn := flag.String("dsn", "", "Postgres DSN (default: $DONTO_DSN or postgres://donto:donto@127.0.0.1:55432/donto)")
	poll := flag.Duration("poll", 5*time.Second, "Dashboard poll interval")
	srvURL := flag.String("srv", "http://127.0.0.1:7878", "dontosrv URL for health checks")
	installTriggers := flag.Bool("install-triggers", false, "Install LISTEN/NOTIFY triggers on connect")
	flag.Parse()

	if *dsn == "" {
		*dsn = os.Getenv("DONTO_DSN")
	}
	if *dsn == "" {
		*dsn = "postgres://donto:donto@127.0.0.1:55432/donto"
	}

	ctx := context.Background()
	pool, err := db.NewPool(ctx, *dsn)
	if err != nil {
		log.Fatalf("connect: %v", err)
	}
	defer pool.Close()

	if *installTriggers {
		if err := db.InstallNotifyTrigger(ctx, pool); err != nil {
			log.Fatalf("install triggers: %v", err)
		}
	}

	m := app.New(pool, *poll, *srvURL)
	p := tea.NewProgram(m, tea.WithAltScreen(), tea.WithMouseCellMotion())

	go db.ListenAudit(ctx, *dsn, p)

	if _, err := p.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "error: %v\n", err)
		os.Exit(1)
	}
}
