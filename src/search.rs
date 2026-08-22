//! # Pure search subsystem.
//!
//! Builds a per-entry search index from the static timezone catalogue,
//! scores user queries against it, and returns ranked results. There is
//! no I/O, no [`App`] state, and no mutable global — the module exists
//! purely so [`crate::app::App::apply_filter`] can be a thin glue
//! function instead of a 60-line method that rebuilds normalized
//! haystacks on every keystroke.
//!
//! ## Pattern: precomputed normalized haystacks
//!
//! Each [`TimezoneEntry`] is normalized **once** at startup
//! (lowercased, punctuation stripped) and cached inside a
//! [`SearchIndex`]. The same goes for the per-offset alias list (e.g.
//! `["utc-5", "gmt-05:00", "utc-0500", ...]`): we cache one [`Vec<String>`]
//! per distinct offset seconds value and re-use it for every entry
//! sharing that offset.
//!
//! Searches walk the cached index, score each candidate, and return the
//! ranked `(index, display_name, score)` tuples; the caller does the
//! split into `filtered_indices` / `filtered_display_names`.
//!
//! ## Scoring model
//!
//! Two complementary passes feed a single sum-then-tiebreak rank:
//!
//! 1. **Phrase pass** ([`PHRASE_WEIGHTS`]) scores the entire normalized
//!    query against each field of an entry and keeps the single best
//!    field score. Rewards "looks like one of our labels" matches —
//!    e.g. `"new york"` exactly hits the city haystack `"new york"`.
//! 2. **Per-term pass** ([`TERM_WEIGHTS`]) runs the same field walk
//!    once per whitespace-split term and **requires every term to find
//!    a non-zero match somewhere**. That enforces AND semantics across
//!    terms (e.g. `"asia tokyo"` only matches entries that score on
//!    both `asia` and `tokyo`). Per-term hits across all terms are
//!    summed.
//!
//! The final entry score is `phrase_best + sum_of_term_bests`, with a
//! small [`FAVORITE_TIEBREAKER_BONUS`] added for favourited entries so
//! that two entries scoring identically rank favourites first. The bonus
//! is intentionally tiny — it breaks ties only, it does not override
//! genuine relevance differences (a non-favourite with even one extra
//! contains-match still ranks above a favourite).

use std::collections::HashMap;
use std::collections::HashSet;

use chrono::DateTime;
use chrono::Utc;
use chrono::offset::Offset;
use chrono_tz::Tz;

use crate::timezone::{
    SupplementalSearchTerm, TimezoneEntry, country_search_aliases, format_utc_offset,
    supplemental_search_terms,
};

/// Minimum query length required for a contains-style (substring) match.
///
/// Shorter terms only count if they exactly equal or prefix a candidate
/// — otherwise a single letter would match almost every entry and the
/// substring pass would dominate the ranking.
const CONTAINS_MIN_CHARS: usize = 3;

/// Score bonus added to favourited entries.
///
/// Tiebreaker only: a non-favourite with any extra field-score (e.g. a
/// single additional contains-match worth 25–105 points) still
/// outranks a favourite. See module-level docs.
const FAVORITE_TIEBREAKER_BONUS: u32 = 10;

/// Pre-normalized search metadata for the entire timezone catalogue.
///
/// Built once in [`crate::app::App::new`] after the catalogue is loaded
/// and then queried on every keystroke via [`SearchIndex::search`].
pub struct SearchIndex {
    entries: Vec<TimezoneSearchData>,
}

impl SearchIndex {
    /// Builds the index by normalizing every searchable field of every
    /// entry in the catalogue. Linear in the number of timezone
    /// entries and runs once at startup.
    pub fn build(entries: &[TimezoneEntry]) -> Self {
        Self {
            entries: entries.iter().map(TimezoneSearchData::new).collect(),
        }
    }

    /// Returns ranked results for `query_text` against the catalogue
    /// `entries`, computed at the wall-clock instant `now`.
    ///
    /// Returns `None` when the query normalizes to no terms (empty
    /// string or pure punctuation) — the caller should treat this
    /// identically to an empty search box and fall back to the
    /// unfiltered, favourites-first sort. `Some(vec)` always reflects
    /// an actually-issued query, even if zero candidates matched.
    ///
    /// Each result is `(entry_index, display_name, score)` sorted by
    /// score descending, with display-name and city-name ties broken
    /// alphabetically. Favorites receive a
    /// [`FAVORITE_TIEBREAKER_BONUS`] score nudge so that they outrank
    /// otherwise-equivalent matches (sum-only tiebreaker, never a
    /// rank override — see module-level docs).
    ///
    /// `base_indices` is the candidate set — typically `0..entries.len()`
    /// for the unfiltered catalogue, or just the favorites' indices when
    /// the favorites-only filter is active.
    ///
    /// `favorite_positions` maps each favorite [`Tz`] to its
    /// user-ordered position; only `contains_key` is used here (the
    /// position itself drives the unfiltered sort, not the scored
    /// search).
    pub fn search(
        &self,
        query_text: &str,
        entries: &[TimezoneEntry],
        base_indices: &[usize],
        now: &DateTime<Utc>,
        favorite_positions: &HashMap<Tz, usize>,
    ) -> Option<Vec<(usize, &'static str, u32)>> {
        let query = SearchQuery::new(query_text);
        if query.terms.is_empty() {
            return None;
        }

        // One offset → terms list per distinct offset_seconds. Most
        // catalogue entries share an offset, so this cuts the alias
        // expansion work by ~50×.
        let mut offset_cache: HashMap<i32, Vec<String>> = HashMap::new();
        let mut scored: Vec<(usize, &'static str, u32)> = base_indices
            .iter()
            .copied()
            .filter_map(|i| {
                let entry = &entries[i];
                let offset_secs = now
                    .with_timezone(&entry.tz)
                    .offset()
                    .fix()
                    .local_minus_utc();
                let offset_terms = offset_cache
                    .entry(offset_secs)
                    .or_insert_with(|| offset_search_terms(offset_secs));
                let mut score =
                    score_phrase_match(&query, &self.entries[i], offset_terms.as_slice());
                for term in &query.terms {
                    let term_score =
                        score_search_term(term, &self.entries[i], offset_terms.as_slice());
                    if term_score == 0 {
                        return None;
                    }
                    score += term_score;
                }
                if favorite_positions.contains_key(&entry.tz) {
                    score += FAVORITE_TIEBREAKER_BONUS;
                }
                let display_name = best_display_name(entry, &self.entries[i], &query);
                Some((i, display_name, score))
            })
            .collect();

        scored.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| a.1.cmp(b.1))
                .then_with(|| entries[a.0].city.cmp(entries[b.0].city))
        });
        Some(scored)
    }
}

// ── Internal types ──────────────────────────────────────────────────

struct SearchText {
    raw: &'static str,
    normalized: String,
}

impl SearchText {
    fn new(raw: &'static str) -> Self {
        Self {
            raw,
            normalized: normalize_search_text(raw),
        }
    }
}

struct SearchKeyword {
    text: SearchText,
    display_in_results: bool,
}

impl SearchKeyword {
    fn new(term: SupplementalSearchTerm) -> Self {
        Self {
            text: SearchText::new(term.raw),
            display_in_results: term.display_in_results,
        }
    }
}

struct TimezoneSearchData {
    city: SearchText,
    country: String,
    region: String,
    timezone_words: String,
    aliases: Vec<SearchText>,
    country_aliases: Vec<String>,
    keywords: Vec<SearchKeyword>,
}

impl TimezoneSearchData {
    fn new(entry: &TimezoneEntry) -> Self {
        // TODO: non-ASCII chars currently normalize to whitespace
        // (e.g. "São Paulo" → "s o paulo"). Both sides of the comparison
        // are normalized identically so search still works, but
        // diacritic-typers rank lower than ASCII-typers for the same
        // city. Fix would be Unicode-aware folding (e.g. via the
        // unicode-normalization crate).
        Self {
            city: SearchText::new(entry.city),
            country: normalize_search_text(entry.country),
            region: normalize_search_text(entry.region),
            timezone_words: normalize_search_text(&entry.tz.to_string()),
            aliases: entry
                .aliases
                .iter()
                .map(|alias| SearchText::new(alias))
                .filter(|alias| !alias.normalized.is_empty())
                .collect(),
            country_aliases: country_search_aliases(entry.country)
                .iter()
                .map(|alias| normalize_search_text(alias))
                .filter(|alias| !alias.is_empty())
                .collect(),
            keywords: supplemental_search_terms(entry)
                .iter()
                .copied()
                .map(SearchKeyword::new)
                .filter(|term| !term.text.normalized.is_empty())
                .collect(),
        }
    }
}

struct SearchQuery {
    normalized: String,
    terms: Vec<String>,
}

impl SearchQuery {
    fn new(query: &str) -> Self {
        let normalized = normalize_search_text(query);
        let mut terms: Vec<String> = normalized.split_whitespace().map(String::from).collect();
        rewrite_bare_offset_aliases(&mut terms);
        Self { normalized, terms }
    }
}

/// Rewrites a lone bare `utc` or `gmt` term to `utc+0`.
///
/// Without this rewrite, typing just `utc` would match every entry whose
/// haystack contains the literal substring `utc` — which is almost every
/// entry, because the cached offset aliases include UTC-prefixed forms.
/// The rewrite only fires when the entire query is a single bare term,
/// so compound queries like `utc tokyo` keep their literal `utc` term.
fn rewrite_bare_offset_aliases(terms: &mut Vec<String>) {
    if matches!(terms.as_slice(), [single] if matches!(single.as_str(), "utc" | "gmt")) {
        *terms = vec!["utc+0".to_string()];
    }
}

// ── Scoring weights ─────────────────────────────────────────────────

/// Per-field exact/prefix/contains weights.
///
/// Two separate weight tables ([`PHRASE_WEIGHTS`] and [`TERM_WEIGHTS`])
/// hold the actual numbers; this struct just gives them names instead
/// of leaving three magic integers at every call site.
#[derive(Copy, Clone, Debug)]
struct FieldWeights {
    exact: u32,
    prefix: u32,
    contains: u32,
}

impl FieldWeights {
    /// Returns the weight that applies to a given match kind.
    fn weight_for(&self, kind: MatchKind) -> u32 {
        match kind {
            MatchKind::Exact => self.exact,
            MatchKind::Prefix => self.prefix,
            MatchKind::Contains => self.contains,
        }
    }
}

/// Which field of an entry is being scored.
///
/// The same eight fields are scored in both passes — only the weights
/// differ — so a single enum drives both [`PHRASE_WEIGHTS`] and
/// [`TERM_WEIGHTS`]. The order of the table entries doubles as the
/// human-readable priority order: city outranks aliases outranks
/// keywords, and so on.
#[derive(Copy, Clone, Debug)]
enum Field {
    City,
    Aliases,
    Keywords,
    TzWords,
    Country,
    CountryAliases,
    Region,
    Offset,
}

/// Weights applied when matching the entire normalized query against
/// each field (phrase-match pass).
const PHRASE_WEIGHTS: [(Field, FieldWeights); 8] = [
    (
        Field::City,
        FieldWeights {
            exact: 220,
            prefix: 165,
            contains: 105,
        },
    ),
    (
        Field::Aliases,
        FieldWeights {
            exact: 180,
            prefix: 135,
            contains: 85,
        },
    ),
    (
        Field::Keywords,
        FieldWeights {
            exact: 175,
            prefix: 130,
            contains: 85,
        },
    ),
    (
        Field::TzWords,
        FieldWeights {
            exact: 170,
            prefix: 125,
            contains: 85,
        },
    ),
    (
        Field::Country,
        FieldWeights {
            exact: 135,
            prefix: 105,
            contains: 70,
        },
    ),
    (
        Field::CountryAliases,
        FieldWeights {
            exact: 125,
            prefix: 95,
            contains: 65,
        },
    ),
    (
        Field::Region,
        FieldWeights {
            exact: 110,
            prefix: 80,
            contains: 55,
        },
    ),
    (
        Field::Offset,
        FieldWeights {
            exact: 120,
            prefix: 95,
            contains: 70,
        },
    ),
];

/// Weights applied per-term during the AND-across-terms pass.
const TERM_WEIGHTS: [(Field, FieldWeights); 8] = [
    (
        Field::City,
        FieldWeights {
            exact: 100,
            prefix: 75,
            contains: 50,
        },
    ),
    (
        Field::Aliases,
        FieldWeights {
            exact: 90,
            prefix: 68,
            contains: 45,
        },
    ),
    (
        Field::Keywords,
        FieldWeights {
            exact: 85,
            prefix: 64,
            contains: 42,
        },
    ),
    (
        Field::TzWords,
        FieldWeights {
            exact: 85,
            prefix: 65,
            contains: 45,
        },
    ),
    (
        Field::Country,
        FieldWeights {
            exact: 60,
            prefix: 45,
            contains: 30,
        },
    ),
    (
        Field::CountryAliases,
        FieldWeights {
            exact: 55,
            prefix: 42,
            contains: 28,
        },
    ),
    (
        Field::Region,
        FieldWeights {
            exact: 50,
            prefix: 38,
            contains: 25,
        },
    ),
    (
        Field::Offset,
        FieldWeights {
            exact: 70,
            prefix: 55,
            contains: 40,
        },
    ),
];

/// Weights used when picking the alias label to render in the table.
///
/// Mirrors the structure of [`PHRASE_WEIGHTS`]/[`TERM_WEIGHTS`] so the
/// "which label matched best" decision uses the same scale as the
/// outer ranking — the numbers happen to equal `City` from each table
/// because the label scorer only ever looks at one normalized field at
/// a time.
const DISPLAY_PHRASE_WEIGHTS: FieldWeights = FieldWeights {
    exact: 220,
    prefix: 165,
    contains: 105,
};
const DISPLAY_TERM_WEIGHTS: FieldWeights = FieldWeights {
    exact: 100,
    prefix: 75,
    contains: 50,
};

// ── Scoring ─────────────────────────────────────────────────────────

fn score_phrase_match(
    query: &SearchQuery,
    search: &TimezoneSearchData,
    offset_terms: &[String],
) -> u32 {
    if query.normalized.is_empty() {
        return 0;
    }
    score_against_haystack(&query.normalized, search, offset_terms, &PHRASE_WEIGHTS)
}

fn score_search_term(term: &str, search: &TimezoneSearchData, offset_terms: &[String]) -> u32 {
    score_against_haystack(term, search, offset_terms, &TERM_WEIGHTS)
}

/// Walks every field in `weights` (in declaration order, which is the
/// human-readable priority order), scores `query` against that field's
/// candidate strings, and returns the single best score.
///
/// Multi-candidate fields (aliases, keywords, country_aliases, offset)
/// expand to "best score across their candidates"; single-string fields
/// score directly. Both pathways funnel through [`score_field`] so the
/// exact/prefix/contains classifier is shared.
fn score_against_haystack(
    query: &str,
    search: &TimezoneSearchData,
    offset_terms: &[String],
    weights: &[(Field, FieldWeights); 8],
) -> u32 {
    let mut best = 0;
    for (field, w) in weights {
        let candidate_best = match field {
            Field::City => score_field(&search.city.normalized, query, *w),
            Field::Aliases => best_score(
                search.aliases.iter().map(|alias| alias.normalized.as_str()),
                query,
                *w,
            ),
            Field::Keywords => best_score(
                search
                    .keywords
                    .iter()
                    .map(|keyword| keyword.text.normalized.as_str()),
                query,
                *w,
            ),
            Field::TzWords => score_field(&search.timezone_words, query, *w),
            Field::Country => score_field(&search.country, query, *w),
            Field::CountryAliases => {
                best_score(search.country_aliases.iter().map(String::as_str), query, *w)
            }
            Field::Region => score_field(&search.region, query, *w),
            Field::Offset => best_score(offset_terms.iter().map(String::as_str), query, *w),
        };
        best = best.max(candidate_best);
    }
    best
}

fn best_display_name(
    entry: &TimezoneEntry,
    search: &TimezoneSearchData,
    query: &SearchQuery,
) -> &'static str {
    let city_score = display_match_score(&search.city.normalized, query);
    let alias_match = best_search_text_match(search.aliases.iter(), query);
    let keyword_match = best_search_text_match(
        search
            .keywords
            .iter()
            .filter(|keyword| keyword.display_in_results)
            .map(|keyword| &keyword.text),
        query,
    );
    let best_non_city = alias_match
        .into_iter()
        .chain(keyword_match)
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));

    match best_non_city {
        Some((label, score)) if score > city_score && score > 0 => label,
        _ => entry.city,
    }
}

fn best_search_text_match<'a>(
    candidates: impl IntoIterator<Item = &'a SearchText>,
    query: &SearchQuery,
) -> Option<(&'static str, u32)> {
    candidates
        .into_iter()
        .map(|candidate| {
            (
                candidate.raw,
                display_match_score(&candidate.normalized, query),
            )
        })
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)))
}

fn display_match_score(field: &str, query: &SearchQuery) -> u32 {
    let mut score = score_field(field, &query.normalized, DISPLAY_PHRASE_WEIGHTS);
    for term in &query.terms {
        score += score_field(field, term, DISPLAY_TERM_WEIGHTS);
    }
    score
}

fn best_score<'a>(
    fields: impl IntoIterator<Item = &'a str>,
    term: &str,
    weights: FieldWeights,
) -> u32 {
    fields
        .into_iter()
        .map(|field| score_field(field, term, weights))
        .max()
        .unwrap_or(0)
}

/// Scores how well `term` matches `field`, returning the weight that
/// corresponds to the strongest match kind ([`MatchKind`]) — or zero
/// when neither exact, prefix, nor contains rules apply.
fn score_field(field: &str, term: &str, weights: FieldWeights) -> u32 {
    MatchKind::classify(field, term)
        .map(|kind| weights.weight_for(kind))
        .unwrap_or(0)
}

/// Categorical outcome of comparing a query `term` against a candidate
/// `field`.
///
/// Separating "what kind of match was it?" from "how much is that
/// match worth?" means the rules can be unit-tested independently of
/// the weight tables — and adding a fourth match kind (e.g. fuzzy)
/// later doesn't require touching every call site.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum MatchKind {
    /// `field == term`.
    Exact,
    /// `field` starts with `term`, OR some whitespace-split word
    /// inside `field` starts with `term` (with optional leading
    /// `+`/`-` for offset shorthand).
    Prefix,
    /// `field` contains `term` as a substring, and `term` is at least
    /// [`CONTAINS_MIN_CHARS`] chars long.
    Contains,
}

impl MatchKind {
    /// Returns the strongest match kind that applies, or `None` if
    /// neither rule fires (including the empty-input case).
    fn classify(field: &str, term: &str) -> Option<Self> {
        if field.is_empty() || term.is_empty() {
            return None;
        }
        if field == term {
            return Some(MatchKind::Exact);
        }
        if field.starts_with(term)
            || field
                .split_whitespace()
                .any(|word| word_matches_term(word, term))
        {
            return Some(MatchKind::Prefix);
        }
        if term.len() >= CONTAINS_MIN_CHARS && field.contains(term) {
            return Some(MatchKind::Contains);
        }
        None
    }
}

/// Returns `true` when `word` is the same as `term`, starts with `term`,
/// or matches `term` after stripping a leading offset sign.
///
/// The sign-strip exists so that bare digit queries (`5`) match offset
/// tokens like `+5` and `-5` inside the cached offset haystacks — the
/// user expects `gmt 5` to find UTC+5 cities without typing the plus.
fn word_matches_term(word: &str, term: &str) -> bool {
    if word == term || word.starts_with(term) {
        return true;
    }
    let stripped = word.trim_start_matches(['+', '-']);
    stripped != word && (stripped == term || stripped.starts_with(term))
}

// ── Normalization & offset alias expansion ─────────────────────────

fn offset_search_terms(total_secs: i32) -> Vec<String> {
    let sign = if total_secs >= 0 { '+' } else { '-' };
    let abs = total_secs.unsigned_abs();
    let hours = abs / 3600;
    let mins = (abs % 3600) / 60;

    let bare_canonical = if mins == 0 {
        format!("{sign}{hours}")
    } else {
        format!("{sign}{hours}:{mins:02}")
    };
    let bare_full = format!("{sign}{hours}:{mins:02}");
    let bare_padded = format!("{sign}{hours:02}:{mins:02}");
    let bare_compact = format!("{sign}{hours:02}{mins:02}");

    let variants = [
        bare_canonical,
        bare_full.clone(),
        bare_padded.clone(),
        bare_compact.clone(),
        format_utc_offset(total_secs),
        format!(
            "GMT{}",
            if mins == 0 {
                format!("{sign}{hours}")
            } else {
                format!("{sign}{hours}:{mins:02}")
            }
        ),
        format!("UTC{bare_full}"),
        format!("UTC{bare_padded}"),
        format!("UTC{bare_compact}"),
        format!("GMT{bare_full}"),
        format!("GMT{bare_padded}"),
        format!("GMT{bare_compact}"),
    ];

    let mut seen: HashSet<String> = HashSet::new();
    variants
        .into_iter()
        .map(|v| normalize_search_text(&v))
        .filter(|v| !v.is_empty() && seen.insert(v.clone()))
        .collect()
}

fn normalize_search_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut last_was_space = true;
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        let next_is_digit = chars.peek().is_some_and(|next| next.is_ascii_digit());
        for lower in ch.to_lowercase() {
            match lower {
                'a'..='z' | '0'..='9' | '+' => {
                    normalized.push(lower);
                    last_was_space = false;
                }
                '-' if next_is_digit => {
                    normalized.push(lower);
                    last_was_space = false;
                }
                '\'' | '’' | '.' => {}
                _ => {
                    if !last_was_space {
                        normalized.push(' ');
                        last_was_space = true;
                    }
                }
            }
        }
    }

    while normalized.ends_with(' ') {
        normalized.pop();
    }
    normalized
}

#[cfg(test)]
mod tests {
    // Tests panic on failure by design — see src/app.rs for the
    // rationale on why production lints are relaxed inside test modules.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    // ── normalize_search_text ──────────────────────────────────────

    #[test]
    fn normalize_lowercases_and_collapses_whitespace() {
        assert_eq!(normalize_search_text("Asia/Tokyo  "), "asia tokyo");
        // Non-ASCII letters (like 'ã') are not in the allow-list and so
        // collapse to a space — São Paulo searches as "s o paulo".
        // Diacritic-stripping would be a separate, intentional change.
        assert_eq!(normalize_search_text("  São Paulo  "), "s o paulo");
        assert_eq!(normalize_search_text("UTC"), "utc");
        assert_eq!(normalize_search_text(""), "");
    }

    #[test]
    fn normalize_strips_apostrophes_and_periods() {
        assert_eq!(normalize_search_text("St. John's"), "st johns");
    }

    #[test]
    fn normalize_preserves_signed_offsets() {
        // The `-` is kept when followed by a digit (offset notation),
        // dropped otherwise (becomes a word separator).
        assert_eq!(normalize_search_text("UTC-5"), "utc-5");
        assert_eq!(normalize_search_text("GMT+05:30"), "gmt+05 30");
        assert_eq!(normalize_search_text("south-east"), "south east");
    }

    // ── offset_search_terms ────────────────────────────────────────

    #[test]
    fn offset_terms_for_utc_contain_canonical_aliases() {
        let terms = offset_search_terms(0);
        assert!(terms.iter().any(|t| t == "+0"));
        assert!(terms.iter().any(|t| t == "utc+0"));
        assert!(terms.iter().any(|t| t == "gmt+0"));
    }

    #[test]
    fn offset_terms_for_plus_one_hour() {
        let terms = offset_search_terms(3600);
        assert!(terms.iter().any(|t| t == "+1"));
        assert!(terms.iter().any(|t| t == "utc+1"));
        assert!(terms.iter().any(|t| t == "gmt+1"));
    }

    #[test]
    fn offset_terms_for_minus_five_hours() {
        let terms = offset_search_terms(-5 * 3600);
        assert!(terms.iter().any(|t| t == "utc-5"));
        assert!(terms.iter().any(|t| t == "gmt-5"));
        assert!(terms.iter().any(|t| t == "-5"));
    }

    #[test]
    fn offset_terms_are_deduplicated() {
        // For whole-hour offsets, several of the format permutations
        // normalize to identical strings — the dedup pass should keep
        // each unique form exactly once.
        let terms = offset_search_terms(0);
        let mut sorted = terms.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), terms.len(), "found duplicates: {terms:?}");
    }

    // ── format_utc_offset ──────────────────────────────────────────

    #[test]
    fn format_utc_offset_zero_and_whole_hours() {
        assert_eq!(format_utc_offset(0), "UTC+0");
        assert_eq!(format_utc_offset(3600), "UTC+1");
        assert_eq!(format_utc_offset(-5 * 3600), "UTC-5");
    }

    #[test]
    fn format_utc_offset_half_hours() {
        // India is +5:30, Newfoundland is -3:30 (the canonical
        // examples for fractional offsets).
        assert_eq!(format_utc_offset(5 * 3600 + 30 * 60), "UTC+5:30");
        assert_eq!(format_utc_offset(-(3 * 3600 + 30 * 60)), "UTC-3:30");
    }

    // ── SearchQuery::new ───────────────────────────────────────────

    #[test]
    fn search_query_rewrites_bare_utc_to_plus_zero() {
        let q = SearchQuery::new("utc");
        assert_eq!(q.terms, vec!["utc+0".to_string()]);
    }

    #[test]
    fn search_query_rewrites_bare_gmt_to_plus_zero() {
        let q = SearchQuery::new("gmt");
        assert_eq!(q.terms, vec!["utc+0".to_string()]);
    }

    #[test]
    fn search_query_leaves_compound_queries_alone() {
        // "utc tokyo" should NOT rewrite — the rewrite only fires for
        // a single bare term.
        let q = SearchQuery::new("utc tokyo");
        assert_eq!(q.terms, vec!["utc".to_string(), "tokyo".to_string()]);
    }

    // ── MatchKind::classify ────────────────────────────────────────

    #[test]
    fn match_kind_classify_exact_prefix_contains_none() {
        // Exact: identical strings.
        assert_eq!(
            MatchKind::classify("tokyo", "tokyo"),
            Some(MatchKind::Exact)
        );
        // Prefix: field starts with term.
        assert_eq!(MatchKind::classify("tokyo", "tok"), Some(MatchKind::Prefix));
        // Prefix via word boundary: term is the start of a word inside field.
        assert_eq!(
            MatchKind::classify("new york", "york"),
            Some(MatchKind::Prefix)
        );
        // Contains: substring in the middle, term ≥ 3 chars.
        assert_eq!(
            MatchKind::classify("tokyo", "kyo"),
            Some(MatchKind::Contains)
        );
        // Short non-prefix substring: no match.
        assert_eq!(MatchKind::classify("tokyo", "ky"), None);
    }

    #[test]
    fn match_kind_classify_empty_inputs_are_none() {
        assert_eq!(MatchKind::classify("", "tokyo"), None);
        assert_eq!(MatchKind::classify("tokyo", ""), None);
        assert_eq!(MatchKind::classify("", ""), None);
    }

    // ── score_field: regression locks against app-level tests ──────

    #[test]
    fn score_field_prefers_exact_over_prefix_over_contains() {
        let w = FieldWeights {
            exact: 100,
            prefix: 75,
            contains: 50,
        };
        // "tokyo" exact-matches the city haystack "tokyo".
        assert_eq!(score_field("tokyo", "tokyo", w), 100);
        // "tok" is a prefix of "tokyo".
        assert_eq!(score_field("tokyo", "tok", w), 75);
        // "kyo" is a 3+ char substring but neither a prefix nor a
        // word boundary match.
        assert_eq!(score_field("tokyo", "kyo", w), 50);
        // "ky" is < 3 chars and not a prefix → no match.
        assert_eq!(score_field("tokyo", "ky", w), 0);
    }

    // ── End-to-end SearchIndex::search ─────────────────────────────

    /// Builds a real `SearchIndex` from the full catalogue. End-to-end
    /// tests need the live catalogue because `TimezoneEntry` has
    /// `&'static str` fields that the supplemental-search-term lookup
    /// hashes against — hand-rolling synthetic entries would
    /// short-circuit half the index.
    fn fixture() -> (&'static [TimezoneEntry], SearchIndex) {
        let entries = crate::timezone::all_timezones();
        let index = SearchIndex::build(entries);
        (entries, index)
    }

    fn run_search(
        index: &SearchIndex,
        entries: &[TimezoneEntry],
        query: &str,
        favorites: &HashMap<Tz, usize>,
    ) -> Option<Vec<(usize, &'static str, u32)>> {
        let base: Vec<usize> = (0..entries.len()).collect();
        let now = Utc::now();
        index.search(query, entries, &base, &now, favorites)
    }

    #[test]
    fn search_and_logic_rejects_entry_missing_any_term() {
        let (entries, index) = fixture();
        let favorites = HashMap::new();
        // "tokyo paris" cannot match a single entry — Tokyo's haystack
        // has no "paris" and vice versa. AND logic across terms means
        // the result set must be empty.
        let results =
            run_search(&index, entries, "tokyo paris", &favorites).expect("non-empty query");
        assert!(
            results.is_empty(),
            "expected no AND-matches for 'tokyo paris', got {} hits",
            results.len()
        );
    }

    #[test]
    fn search_favorite_bonus_is_tiebreaker_only() {
        let (entries, index) = fixture();

        // Pick Tokyo's catalogue index and make it a favourite.
        let tokyo_idx = entries
            .iter()
            .position(|e| e.tz == Tz::Asia__Tokyo)
            .expect("Tokyo in catalogue");
        let new_york_idx = entries
            .iter()
            .position(|e| e.tz == Tz::America__New_York)
            .expect("New York in catalogue");

        // Favourite Tokyo only.
        let mut favorites = HashMap::new();
        favorites.insert(Tz::Asia__Tokyo, 0usize);

        // Query "new york" — Tokyo gets no match, New York gets a strong
        // exact-city match. Despite the favourite bonus, New York wins:
        // the bonus is a tiebreaker, not a rank override.
        let results = run_search(&index, entries, "new york", &favorites).expect("non-empty");
        assert!(!results.is_empty(), "expected matches for 'new york'");
        assert_eq!(
            results[0].0, new_york_idx,
            "non-favourite New York must outrank favourite Tokyo when it has a strictly higher base score"
        );
        // Tokyo should not even be in the result set (no AND match).
        assert!(
            !results.iter().any(|(i, _, _)| *i == tokyo_idx),
            "Tokyo should not match 'new york' at all"
        );

        // Now flip the favourite onto Sydney and query something where
        // Sydney and Melbourne would tie. They both live in Australia
        // with similar haystacks; "australia" should match both, but
        // marking Sydney as favourite lifts it above Melbourne. We
        // verify that the favourite-bonus path actually fires.
        let sydney_idx = entries
            .iter()
            .position(|e| e.tz == Tz::Australia__Sydney)
            .expect("Sydney in catalogue");
        let mut favs2 = HashMap::new();
        favs2.insert(Tz::Australia__Sydney, 0usize);

        let with_bonus = run_search(&index, entries, "australia", &favs2).expect("non-empty");
        let without_bonus =
            run_search(&index, entries, "australia", &HashMap::new()).expect("non-empty");

        let sydney_with = with_bonus
            .iter()
            .find(|(i, _, _)| *i == sydney_idx)
            .expect("Sydney should match 'australia'")
            .2;
        let sydney_without = without_bonus
            .iter()
            .find(|(i, _, _)| *i == sydney_idx)
            .expect("Sydney should match 'australia'")
            .2;
        assert_eq!(
            sydney_with - sydney_without,
            FAVORITE_TIEBREAKER_BONUS,
            "favourite bonus should add exactly FAVORITE_TIEBREAKER_BONUS to the score"
        );
    }

    #[test]
    fn search_returns_none_for_pure_punctuation() {
        let (entries, index) = fixture();
        let favorites = HashMap::new();
        // Pure punctuation normalizes to an empty term list — the
        // public contract is to return None so the caller can fall
        // back to the unfiltered view.
        assert!(run_search(&index, entries, "...", &favorites).is_none());
        assert!(run_search(&index, entries, "!!!", &favorites).is_none());
        assert!(run_search(&index, entries, "   ", &favorites).is_none());
    }
}
