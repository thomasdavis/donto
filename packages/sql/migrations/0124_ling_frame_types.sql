-- M6 / PRD §13 — register 18 language-specific frame types in
-- donto_frame_type so claim-frames emitted by the linguistic
-- importers (donto-ling-cldf / -ud / -unimorph / -lift / -eaf) use
-- a stable native registry rather than ad-hoc strings.
--
-- The 18 frame types span the proving-domain phenomena PRD §13
-- enumerates: phonological / morphological / syntactic
-- description, lexical sense, pragmatic context, sociolinguistic
-- attribution, multimodal grounding, and historical reconstruction.
--
-- Idempotent: `on conflict (frame_type) do nothing` so re-running
-- the migration after rows already exist is a no-op.

insert into donto_frame_type (frame_type, domain, description, required_roles, optional_roles)
values
    -- phonological / morphological
    ('phonological-process',  'linguistics',
     'A regular alternation between phonological forms (e.g. lenition, palatalisation).',
     array['input-form','output-form'],
     array['environment','language','source']),
    ('phoneme-inventory-item','linguistics',
     'One phoneme of a language with its features.',
     array['phoneme','language'],
     array['ipa','allophones','source']),
    ('morpheme-segmentation', 'linguistics',
     'A surface form decomposed into morphemes with their boundaries.',
     array['surface-form','segmentation'],
     array['gloss','language','analyst']),
    ('inflection-paradigm',   'linguistics',
     'A lexeme with its inflected word forms keyed by morphosyntactic features.',
     array['lemma','inflected-form','features'],
     array['language','source']),

    -- syntactic / dependency
    ('grammatical-relation', 'linguistics',
     'A syntactic relation between two tokens (subj, obj, det, ...).',
     array['head','dependent','relation'],
     array['sentence','language','annotator']),
    ('constituent',          'linguistics',
     'A continuous span of tokens that forms a syntactic constituent.',
     array['span-start','span-end','category'],
     array['sentence','language','tree-id']),
    ('argument-structure',   'linguistics',
     'A predicate with its semantic argument roles.',
     array['predicate','agent','patient'],
     array['theme','recipient','instrument','source']),

    -- lexical / semantic
    ('lexical-sense',        'linguistics',
     'One sense of a lexeme with its gloss and definition.',
     array['lexeme','sense-id'],
     array['gloss-lang','gloss','definition','part-of-speech','source']),
    ('semantic-frame',       'linguistics',
     'A FrameNet-style frame evoked by a lexical unit.',
     array['frame-name','lexical-unit'],
     array['core-elements','peripheral-elements','source']),
    ('translation-equivalent','linguistics',
     'A cross-lingual sense alignment between two lexemes.',
     array['source-lexeme','target-lexeme','source-language','target-language'],
     array['confidence','source']),

    -- typological / comparative
    ('typological-feature',  'linguistics',
     'A CLDF-style cross-linguistic feature with its value.',
     array['language','parameter','value'],
     array['code','source','dataset']),
    ('cognate-set',          'linguistics',
     'A set of forms in related languages reconstructed from a common ancestor.',
     array['set-id','members'],
     array['proto-form','family','source']),

    -- pragmatic / sociolinguistic
    ('utterance-context',    'linguistics',
     'A speech-act with pragmatic context (speaker, addressee, register).',
     array['utterance'],
     array['speaker','addressee','register','setting','source']),
    ('register-attribution', 'linguistics',
     'A claim that a form belongs to a particular register or sociolect.',
     array['form','register'],
     array['variety','community','source']),

    -- documentation / multimodal
    ('multimodal-annotation','linguistics',
     'A time-aligned annotation over audio or video media.',
     array['media-iri','start-ms','end-ms'],
     array['tier','participant','annotation','source']),
    ('elicited-form',        'linguistics',
     'A form produced in an elicitation session, with the stimulus.',
     array['form','stimulus'],
     array['speaker','session','language','source']),

    -- historical / restricted
    ('reconstructed-form',   'linguistics',
     'A historically reconstructed (proto-)form with its evidence.',
     array['proto-form','language-family'],
     array['evidence-set','reconstructor','source']),
    ('restricted-cultural-claim','linguistics',
     'A claim about culturally restricted material (kinship terms, sacred names, ceremonial forms) requiring community-protocol policy.',
     array['claim','community'],
     array['protocol','authority','source'])

on conflict (frame_type) do nothing;
