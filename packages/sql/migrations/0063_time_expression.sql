-- Temporal expressions: EDTF-compatible, with precision, uncertainty, and probability models.

CREATE TABLE IF NOT EXISTS donto_time_expression (
    time_expr_id        BIGSERIAL PRIMARY KEY,
    raw_text            TEXT,
    edtf                TEXT,
    earliest_start      DATE,
    latest_start        DATE,
    earliest_end        DATE,
    latest_end          DATE,
    canonical_range     DATERANGE,
    grain               TEXT NOT NULL DEFAULT 'unknown'
                        CHECK (grain IN ('day','month','year','decade','century','event_relative','unknown')),
    is_uncertain        BOOLEAN NOT NULL DEFAULT false,
    is_approximate      BOOLEAN NOT NULL DEFAULT false,
    probability_model   JSONB NOT NULL DEFAULT '{}',
    anchor_event_iri    TEXT,
    calendar            TEXT NOT NULL DEFAULT 'gregorian',
    parser_version      TEXT NOT NULL DEFAULT 'v1',
    created_tx          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS donto_time_expr_range ON donto_time_expression USING GIST (canonical_range)
    WHERE canonical_range IS NOT NULL;
CREATE INDEX IF NOT EXISTS donto_time_expr_grain ON donto_time_expression (grain);

-- Parse a raw date string into a time expression
CREATE OR REPLACE FUNCTION donto_parse_time_expression(
    p_raw_text      TEXT,
    p_grain         TEXT DEFAULT NULL
) RETURNS BIGINT
LANGUAGE plpgsql AS $$
DECLARE
    v_id BIGINT;
    v_grain TEXT;
    v_edtf TEXT;
    v_start DATE;
    v_end DATE;
    v_approx BOOLEAN := false;
    v_uncertain BOOLEAN := false;
    v_cleaned TEXT;
BEGIN
    v_cleaned := lower(trim(p_raw_text));

    -- Detect circa/approximate
    IF v_cleaned ~ '^(circa|c\.|ca\.|about|abt\.?|approximately|approx\.?)' THEN
        v_approx := true;
        v_cleaned := regexp_replace(v_cleaned, '^(circa|c\.|ca\.|about|abt\.?|approximately|approx\.?)\s*', '');
    END IF;

    -- Detect uncertainty
    IF v_cleaned ~ '\?' THEN
        v_uncertain := true;
        v_cleaned := replace(v_cleaned, '?', '');
    END IF;

    -- Try year-only
    IF v_cleaned ~ '^\d{4}$' THEN
        v_grain := coalesce(p_grain, 'year');
        v_edtf := v_cleaned;
        v_start := (v_cleaned || '-01-01')::date;
        v_end := ((v_cleaned::int + 1)::text || '-01-01')::date;
    -- Try full date
    ELSIF v_cleaned ~ '^\d{4}-\d{2}-\d{2}$' THEN
        v_grain := coalesce(p_grain, 'day');
        v_edtf := v_cleaned;
        v_start := v_cleaned::date;
        v_end := (v_cleaned::date + 1);
    -- Try year-month
    ELSIF v_cleaned ~ '^\d{4}-\d{2}$' THEN
        v_grain := coalesce(p_grain, 'month');
        v_edtf := v_cleaned;
        v_start := (v_cleaned || '-01')::date;
        v_end := ((v_cleaned || '-01')::date + interval '1 month')::date;
    ELSE
        v_grain := coalesce(p_grain, 'unknown');
        v_edtf := p_raw_text;
    END IF;

    IF v_approx THEN v_edtf := v_edtf || '~'; END IF;
    IF v_uncertain THEN v_edtf := v_edtf || '?'; END IF;

    INSERT INTO donto_time_expression
        (raw_text, edtf, earliest_start, latest_start, earliest_end, latest_end,
         canonical_range, grain, is_uncertain, is_approximate)
    VALUES (p_raw_text, v_edtf, v_start, v_start, v_end, v_end,
            CASE WHEN v_start IS NOT NULL AND v_end IS NOT NULL
                 THEN daterange(v_start, v_end, '[)')
                 ELSE NULL END,
            v_grain, v_uncertain, v_approx)
    RETURNING time_expr_id INTO v_id;
    RETURN v_id;
END;
$$;
