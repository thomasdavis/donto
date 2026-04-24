-- References: citations between documents.
--
-- A reference is a structured link from one document to another,
-- with optional label ("[27]"), context within the citing document
-- (which section cites it), and the specific claim the citation
-- supports.

create table if not exists donto_reference (
    reference_id    uuid primary key default gen_random_uuid(),
    citing_doc      uuid not null references donto_document(document_id),
    cited_doc       uuid references donto_document(document_id),
    cited_iri       text,
    label           text,
    title           text,
    authors         text,
    year            text,
    venue           text,
    section_id      uuid references donto_document_section(section_id),
    span_id         uuid references donto_span(span_id),
    metadata        jsonb not null default '{}'::jsonb,
    constraint donto_reference_has_target
        check (cited_doc is not null or cited_iri is not null)
);

create index if not exists donto_reference_citing_idx
    on donto_reference (citing_doc);
create index if not exists donto_reference_cited_idx
    on donto_reference (cited_doc) where cited_doc is not null;
create index if not exists donto_reference_cited_iri_idx
    on donto_reference (cited_iri) where cited_iri is not null;
create index if not exists donto_reference_label_idx
    on donto_reference (citing_doc, label) where label is not null;

create or replace function donto_add_reference(
    p_citing_doc  uuid,
    p_label       text default null,
    p_title       text default null,
    p_authors     text default null,
    p_year        text default null,
    p_venue       text default null,
    p_cited_iri   text default null,
    p_cited_doc   uuid default null
) returns uuid
language plpgsql as $$
declare v_id uuid;
begin
    insert into donto_reference
        (citing_doc, label, title, authors, year, venue, cited_iri, cited_doc)
    values (p_citing_doc, p_label, p_title, p_authors, p_year, p_venue,
            p_cited_iri, p_cited_doc)
    returning reference_id into v_id;
    return v_id;
end;
$$;

-- References for a document, ordered by label
create or replace function donto_references_for(p_doc_id uuid)
returns table(
    reference_id uuid, label text, title text, authors text,
    year text, venue text, cited_iri text, cited_doc uuid
)
language sql stable as $$
    select reference_id, label, title, authors, year, venue,
           cited_iri, cited_doc
    from donto_reference
    where citing_doc = p_doc_id
    order by label
$$;

-- Link a reference to the claim it supports
create or replace function donto_reference_supports(
    p_reference_id uuid,
    p_statement_id uuid,
    p_context      text default 'donto:anonymous'
) returns uuid
language plpgsql as $$
declare
    v_ref donto_reference;
    v_link_id uuid;
begin
    select * into v_ref from donto_reference where reference_id = p_reference_id;
    if v_ref.cited_doc is not null then
        insert into donto_evidence_link
            (statement_id, link_type, target_document_id, context)
        values (p_statement_id, 'cited_in', v_ref.cited_doc, p_context)
        returning link_id into v_link_id;
    end if;
    return v_link_id;
end;
$$;
