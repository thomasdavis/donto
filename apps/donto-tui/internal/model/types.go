package model

import (
	"encoding/json"
	"time"
)

type AuditEntry struct {
	AuditID     int64           `json:"audit_id"`
	At          time.Time       `json:"at"`
	Actor       string          `json:"actor"`
	Action      string          `json:"action"`
	StatementID string          `json:"statement_id"`
	Detail      json.RawMessage `json:"detail"`
}

func (e AuditEntry) DetailString() string {
	if len(e.Detail) == 0 {
		return ""
	}
	var m map[string]interface{}
	if json.Unmarshal(e.Detail, &m) == nil {
		if ctx, ok := m["context"]; ok {
			return ctx.(string)
		}
	}
	return string(e.Detail)
}

type DashboardStats struct {
	TotalStatements    int64
	OpenStatements     int64
	RetractedStatements int64
	ContextCount       int64
	PredicateCount     int64
	AuditCount         int64
}

type MaturityBucket struct {
	Context  string
	Maturity int
	Count    int64
}

type PolarityBucket struct {
	Polarity string
	Count    int64
}

type ActivityBucket struct {
	Bucket time.Time
	Action string
	Count  int64
}

type ContextStat struct {
	IRI            string
	Kind           string
	Parent         *string
	StatementCount int64
	LastAssert     *time.Time
}

type Statement struct {
	StatementID string
	Subject     string
	Predicate   string
	ObjectIRI   *string
	ObjectLit   *string
	Context     string
	Polarity    string
	Maturity    int
	TxLo        time.Time
	TxHi        *time.Time
	ValidLo     *string
	ValidHi     *string
}

type ObligationSummary struct {
	Type   string
	Status string
	Count  int64
}

type PgActivity struct {
	PID        int
	State      string
	Query      string
	QueryStart *time.Time
	AppName    string
}

type GrowthDay struct {
	Day      time.Time
	Asserts  int64
	Retracts int64
	Corrects int64
}

type ContextBar struct {
	IRI   string
	Kind  string
	Count int64
}

type PredicateBar struct {
	Predicate string
	Count     int64
}
