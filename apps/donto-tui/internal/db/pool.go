package db

import (
	"context"
	"encoding/json"
	"net/http"
	"time"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/thomasdavis/donto/apps/donto-tui/internal/model"
)

const queryTimeout = 5 * time.Second

func withTimeout(parent context.Context) (context.Context, context.CancelFunc) {
	return context.WithTimeout(parent, queryTimeout)
}

func NewPool(ctx context.Context, dsn string) (*pgxpool.Pool, error) {
	cfg, err := pgxpool.ParseConfig(dsn)
	if err != nil {
		return nil, err
	}
	cfg.MaxConns = 5
	cfg.ConnConfig.ConnectTimeout = 5 * time.Second
	return pgxpool.NewWithConfig(ctx, cfg)
}

type AuditNotification struct {
	Entry model.AuditEntry
}

func ListenAudit(ctx context.Context, dsn string, p *tea.Program) {
	for {
		if ctx.Err() != nil {
			return
		}
		if err := listenLoop(ctx, dsn, p); err != nil {
			time.Sleep(5 * time.Second)
			continue
		}
		return
	}
}

func listenLoop(ctx context.Context, dsn string, p *tea.Program) error {
	connCtx, cancel := context.WithTimeout(ctx, 5*time.Second)
	defer cancel()

	conn, err := pgx.Connect(connCtx, dsn)
	if err != nil {
		return err
	}
	defer conn.Close(ctx)

	_, err = conn.Exec(ctx, "LISTEN donto_audit")
	if err != nil {
		return err
	}
	conn.Exec(ctx, "LISTEN donto_firehose")

	for {
		notification, err := conn.WaitForNotification(ctx)
		if err != nil {
			return err
		}
		var entry model.AuditEntry
		if err := json.Unmarshal([]byte(notification.Payload), &entry); err != nil {
			continue
		}
		p.Send(AuditNotification{Entry: entry})
	}
}

func InstallNotifyTrigger(ctx context.Context, pool *pgxpool.Pool) error {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	_, err := pool.Exec(qctx, `
		CREATE OR REPLACE FUNCTION donto_audit_notify() RETURNS trigger
		LANGUAGE plpgsql AS $$
		BEGIN
			PERFORM pg_notify('donto_audit', json_build_object(
				'audit_id', NEW.audit_id,
				'at', NEW.at,
				'actor', NEW.actor,
				'action', NEW.action,
				'statement_id', NEW.statement_id,
				'detail', NEW.detail
			)::text);
			RETURN NEW;
		END;
		$$;
		DROP TRIGGER IF EXISTS donto_audit_notify_trg ON donto_audit;
		CREATE TRIGGER donto_audit_notify_trg
			AFTER INSERT ON donto_audit
			FOR EACH ROW EXECUTE FUNCTION donto_audit_notify();
	`)
	return err
}

func FetchDashboard(ctx context.Context, pool *pgxpool.Pool) (*model.DashboardStats, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	stats := &model.DashboardStats{}

	// n_live_tup is updated in real-time by Postgres on every insert/delete
	pool.QueryRow(qctx, `
		SELECT n_live_tup FROM pg_stat_user_tables WHERE relname = 'donto_statement'
	`).Scan(&stats.TotalStatements)
	stats.OpenStatements = stats.TotalStatements
	// n_tup_del approximates retractions (closed tx_time = UPDATE, but
	// ON CONFLICT DO NOTHING also increments dead tuples)
	pool.QueryRow(qctx, `
		SELECT n_dead_tup FROM pg_stat_user_tables WHERE relname = 'donto_statement'
	`).Scan(&stats.RetractedStatements)

	pool.QueryRow(qctx, `SELECT count(*) FROM donto_context`).Scan(&stats.ContextCount)
	pool.QueryRow(qctx, `SELECT count(*) FROM donto_predicate`).Scan(&stats.PredicateCount)
	pool.QueryRow(qctx, `
		SELECT reltuples::bigint FROM pg_class WHERE relname = 'donto_audit'
	`).Scan(&stats.AuditCount)

	return stats, nil
}

func FetchMaturity(ctx context.Context, pool *pgxpool.Pool) ([]model.MaturityBucket, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	// Sample 100k rows for fast approximate distribution
	rows, err := pool.Query(qctx, `
		SELECT '_all_', donto_maturity(flags), count(*)
		FROM donto_statement TABLESAMPLE SYSTEM (0.01)
		WHERE upper(tx_time) IS NULL
		GROUP BY donto_maturity(flags)
		ORDER BY 2
	`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var buckets []model.MaturityBucket
	for rows.Next() {
		var b model.MaturityBucket
		if err := rows.Scan(&b.Context, &b.Maturity, &b.Count); err != nil {
			return nil, err
		}
		buckets = append(buckets, b)
	}
	return buckets, nil
}

func FetchPolarity(ctx context.Context, pool *pgxpool.Pool) ([]model.PolarityBucket, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	// Sample for fast approximate polarity distribution
	rows, err := pool.Query(qctx, `
		SELECT donto_polarity(flags), count(*)
		FROM donto_statement TABLESAMPLE SYSTEM (0.01)
		WHERE upper(tx_time) IS NULL
		GROUP BY donto_polarity(flags)
		ORDER BY count(*) DESC
	`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var buckets []model.PolarityBucket
	for rows.Next() {
		var b model.PolarityBucket
		if err := rows.Scan(&b.Polarity, &b.Count); err != nil {
			return nil, err
		}
		buckets = append(buckets, b)
	}
	return buckets, nil
}

func FetchActivity(ctx context.Context, pool *pgxpool.Pool) ([]model.ActivityBucket, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT date_trunc('hour', at) AS bucket, action, count(*)
		FROM donto_audit
		WHERE at > now() - interval '24 hours'
		GROUP BY bucket, action
		ORDER BY bucket
	`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var buckets []model.ActivityBucket
	for rows.Next() {
		var b model.ActivityBucket
		if err := rows.Scan(&b.Bucket, &b.Action, &b.Count); err != nil {
			return nil, err
		}
		buckets = append(buckets, b)
	}
	return buckets, nil
}

func FetchContexts(ctx context.Context, pool *pgxpool.Pool) ([]model.ContextStat, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	// Use the context table directly with a fast indexed count
	rows, err := pool.Query(qctx, `
		SELECT c.iri, c.kind, c.parent, 0::bigint, NULL::timestamptz
		FROM donto_context c
		ORDER BY c.iri
	`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var contexts []model.ContextStat
	for rows.Next() {
		var c model.ContextStat
		if err := rows.Scan(&c.IRI, &c.Kind, &c.Parent, &c.StatementCount, &c.LastAssert); err != nil {
			return nil, err
		}
		contexts = append(contexts, c)
	}
	return contexts, nil
}

func FetchRecentStatements(ctx context.Context, pool *pgxpool.Pool, limit int) ([]model.Statement, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT s.statement_id::text, s.subject, s.predicate,
		       s.object_iri, s.object_lit::text, s.context,
		       donto_polarity(s.flags), donto_maturity(s.flags),
		       lower(s.tx_time), upper(s.tx_time),
		       lower(s.valid_time)::text, upper(s.valid_time)::text
		FROM donto_audit a
		JOIN donto_statement s ON s.statement_id = a.statement_id
		WHERE a.action = 'assert'
		ORDER BY a.audit_id DESC
		LIMIT $1
	`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanStatements(rows)
}

func FetchStatementsBySubject(ctx context.Context, pool *pgxpool.Pool, subject string, limit int) ([]model.Statement, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT statement_id::text, subject, predicate,
		       object_iri, object_lit::text, context,
		       donto_polarity(flags), donto_maturity(flags),
		       lower(tx_time), upper(tx_time),
		       lower(valid_time)::text, upper(valid_time)::text
		FROM donto_statement
		WHERE subject = $1 AND upper(tx_time) IS NULL
		LIMIT $2
	`, subject, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanStatements(rows)
}

func FetchStatementsByPredicate(ctx context.Context, pool *pgxpool.Pool, predicate string, limit int) ([]model.Statement, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT statement_id::text, subject, predicate,
		       object_iri, object_lit::text, context,
		       donto_polarity(flags), donto_maturity(flags),
		       lower(tx_time), upper(tx_time),
		       lower(valid_time)::text, upper(valid_time)::text
		FROM donto_statement
		WHERE predicate = $1 AND upper(tx_time) IS NULL
		LIMIT $2
	`, predicate, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanStatements(rows)
}

func FetchStatementsByContext(ctx context.Context, pool *pgxpool.Pool, contextIRI string, limit int) ([]model.Statement, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT statement_id::text, subject, predicate,
		       object_iri, object_lit::text, context,
		       donto_polarity(flags), donto_maturity(flags),
		       lower(tx_time), upper(tx_time),
		       lower(valid_time)::text, upper(valid_time)::text
		FROM donto_statement
		WHERE context = $1 AND upper(tx_time) IS NULL
		LIMIT $2
	`, contextIRI, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	return scanStatements(rows)
}

func scanStatements(rows interface {
	Next() bool
	Scan(dest ...interface{}) error
}) ([]model.Statement, error) {
	var stmts []model.Statement
	for rows.Next() {
		var s model.Statement
		if err := rows.Scan(
			&s.StatementID, &s.Subject, &s.Predicate,
			&s.ObjectIRI, &s.ObjectLit,
			&s.Context, &s.Polarity, &s.Maturity,
			&s.TxLo, &s.TxHi, &s.ValidLo, &s.ValidHi,
		); err != nil {
			return stmts, err
		}
		stmts = append(stmts, s)
	}
	return stmts, nil
}

func FetchClaimCard(ctx context.Context, pool *pgxpool.Pool, stmtID string) (string, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	var result string
	err := pool.QueryRow(qctx, `SELECT donto_claim_card($1::uuid)::text`, stmtID).Scan(&result)
	return result, err
}

func FetchObligations(ctx context.Context, pool *pgxpool.Pool) ([]model.ObligationSummary, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT obligation_type, status, count(*)
		FROM donto_proof_obligation
		GROUP BY obligation_type, status
		ORDER BY count(*) DESC
	`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var obs []model.ObligationSummary
	for rows.Next() {
		var o model.ObligationSummary
		if err := rows.Scan(&o.Type, &o.Status, &o.Count); err != nil {
			return nil, err
		}
		obs = append(obs, o)
	}
	return obs, nil
}

func FetchRecentAudit(ctx context.Context, pool *pgxpool.Pool, afterID int64, limit int) ([]model.AuditEntry, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT audit_id, at, coalesce(actor,''), action,
		       coalesce(statement_id::text,''), coalesce(detail::text,'{}')
		FROM donto_audit
		WHERE audit_id > $1
		ORDER BY audit_id DESC
		LIMIT $2
	`, afterID, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var entries []model.AuditEntry
	for rows.Next() {
		var e model.AuditEntry
		if err := rows.Scan(&e.AuditID, &e.At, &e.Actor, &e.Action, &e.StatementID, &e.Detail); err != nil {
			return nil, err
		}
		entries = append(entries, e)
	}
	return entries, nil
}

func Ping(ctx context.Context, pool *pgxpool.Pool) error {
	qctx, cancel := context.WithTimeout(ctx, 2*time.Second)
	defer cancel()
	return pool.Ping(qctx)
}

func CheckSidecar(srvURL string) bool {
	client := &http.Client{Timeout: 2 * time.Second}
	resp, err := client.Get(srvURL + "/health")
	if err != nil {
		return false
	}
	resp.Body.Close()
	return resp.StatusCode == 200
}

func FetchActivity_PG(ctx context.Context, pool *pgxpool.Pool) ([]model.PgActivity, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT pid, coalesce(state,'unknown'),
		       coalesce(left(query, 200), ''),
		       query_start,
		       coalesce(application_name, '')
		FROM pg_stat_activity
		WHERE datname = current_database()
		  AND pid != pg_backend_pid()
		  AND state != 'idle'
		  AND query NOT LIKE '%pg_stat_activity%'
		ORDER BY query_start DESC NULLS LAST
	`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var acts []model.PgActivity
	for rows.Next() {
		var a model.PgActivity
		if err := rows.Scan(&a.PID, &a.State, &a.Query, &a.QueryStart, &a.AppName); err != nil {
			return nil, err
		}
		acts = append(acts, a)
	}
	return acts, nil
}

func QueryVal(ctx context.Context, pool *pgxpool.Pool, sql string, dest interface{}) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	pool.QueryRow(qctx, sql).Scan(dest)
}

func FetchGrowth(ctx context.Context, pool *pgxpool.Pool) ([]model.GrowthDay, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT date_trunc('day', at)::date AS day, action, count(*)
		FROM donto_audit
		WHERE at > now() - interval '14 days'
		GROUP BY day, action
		ORDER BY day
	`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	// Aggregate into per-day buckets.
	dayMap := make(map[time.Time]*model.GrowthDay)
	for rows.Next() {
		var day time.Time
		var action string
		var count int64
		if err := rows.Scan(&day, &action, &count); err != nil {
			return nil, err
		}
		g, ok := dayMap[day]
		if !ok {
			g = &model.GrowthDay{Day: day}
			dayMap[day] = g
		}
		switch action {
		case "assert":
			g.Asserts += count
		case "retract":
			g.Retracts += count
		case "correct":
			g.Corrects += count
		}
	}

	// Fill in all 14 days so the chart always has 14 bars.
	now := time.Now().UTC().Truncate(24 * time.Hour)
	result := make([]model.GrowthDay, 14)
	for i := 0; i < 14; i++ {
		day := now.AddDate(0, 0, i-13)
		if g, ok := dayMap[day]; ok {
			result[i] = *g
		} else {
			result[i] = model.GrowthDay{Day: day}
		}
	}
	return result, nil
}

func FetchTopContexts(ctx context.Context, pool *pgxpool.Pool, limit int) ([]model.ContextBar, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT c.iri, c.kind, coalesce(s.cnt, 0)
		FROM donto_context c
		LEFT JOIN (
			SELECT context, count(*) AS cnt
			FROM donto_statement
			WHERE upper(tx_time) IS NULL
			GROUP BY context
		) s ON s.context = c.iri
		ORDER BY coalesce(s.cnt, 0) DESC
		LIMIT $1
	`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var bars []model.ContextBar
	for rows.Next() {
		var b model.ContextBar
		if err := rows.Scan(&b.IRI, &b.Kind, &b.Count); err != nil {
			return nil, err
		}
		bars = append(bars, b)
	}
	return bars, nil
}

func FetchTopPredicates(ctx context.Context, pool *pgxpool.Pool, limit int) ([]model.PredicateBar, error) {
	qctx, cancel := withTimeout(ctx)
	defer cancel()
	rows, err := pool.Query(qctx, `
		SELECT predicate, count(*) AS cnt
		FROM (
			SELECT predicate
			FROM donto_statement TABLESAMPLE SYSTEM(0.1)
			WHERE upper(tx_time) IS NULL
		) t
		GROUP BY predicate
		ORDER BY cnt DESC
		LIMIT $1
	`, limit)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var bars []model.PredicateBar
	for rows.Next() {
		var b model.PredicateBar
		if err := rows.Scan(&b.Predicate, &b.Count); err != nil {
			return nil, err
		}
		bars = append(bars, b)
	}
	return bars, nil
}
