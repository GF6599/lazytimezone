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
//! ranked `(index, display_name, score)` tuples; the caller turns them
//! into the rows of its filtered view.
//!
//! ## Scoring model
//!
//! One weight table ([`FIELD_WEIGHTS`]) drives everything. Each field
//! carries exact/prefix/contains weights, and two passes share them:
//!
//! 1. **Phrase pass** scores the entire normalized query against each
//!    field and keeps the single best score, so "new york" exactly
//!    hitting a city haystack outranks scattered word hits.
//! 2. **Per-term pass** scores each whitespace-split term the same
//!    way, requires every term to hit somewhere (AND semantics), and
//!    contributes at half weight, so one strong term cannot outrank a
//!    whole-query phrase match.
//!
//! The sum then gains a log-scale population bonus
//! ([`population_score`]), which is what ranks New York City above
//! York for "york"-shaped queries, and a small
//! [`FAVORITE_TIEBREAKER_BONUS`] so equal scores favour favourites.
//! Ties break by population, then name.

use std::collections::HashMap;
use std::collections::HashSet;

use crate::timezone::{
    SupplementalSearchTerm, TimezoneEntry, city_search_aliases, country_search_aliases,
    format_utc_offset, region_of, zone_search_terms,
};
use chrono::DateTime;
use chrono::Utc;
use chrono::offset::Offset;

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
    /// `favorite_positions` maps each favorite catalogue index to its
    /// user-ordered position; only `contains_key` is used here (the
    /// position itself drives the unfiltered sort, not the scored
    /// search).
    pub fn search(
        &self,
        query_text: &str,
        entries: &[TimezoneEntry],
        base_indices: &[usize],
        now: &DateTime<Utc>,
        favorite_positions: &HashMap<usize, usize>,
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
                    // Halved so a single strong per-term hit cannot
                    // outrank a whole-query phrase match: "york" alone
                    // in "america/new_york" must not lift York past
                    // New York City.
                    score += term_score / 2;
                }
                score += self.entries[i].pop_score;
                if favorite_positions.contains_key(&i) {
                    score += FAVORITE_TIEBREAKER_BONUS;
                }
                let display_name = best_display_name(entry, &self.entries[i], &query);
                Some((i, display_name, score))
            })
            .collect();

        scored.sort_by(|a, b| {
            // Population separates the hundreds of rows a zone-wide
            // phrase like "eastern time" matches equally: the big
            // cities are what such a query means.
            b.2.cmp(&a.2)
                .then_with(|| entries[b.0].population.cmp(&entries[a.0].population))
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
    state: String,
    aliases: Vec<SearchText>,
    country_aliases: Vec<String>,
    keywords: Vec<SearchKeyword>,
    pop_score: u32,
}

impl TimezoneSearchData {
    fn new(entry: &TimezoneEntry) -> Self {
        Self {
            city: SearchText::new(entry.city),
            country: normalize_search_text(entry.country),
            region: normalize_search_text(region_of(entry.tz)),
            // The IANA name of a fixed offset inverts its sign: `Etc/GMT-10`
            // is UTC+10. Its offset terms find it instead.
            timezone_words: if entry.is_fixed_offset() {
                String::new()
            } else {
                normalize_search_text(&entry.tz.to_string())
            },
            state: normalize_search_text(entry.admin1),
            // Per-city aliases: the ASCII transliteration plus the
            // curated nicknames, exonyms, and landmarks. Other cities
            // in the zone are catalogue rows now, not aliases.
            aliases: (!entry.ascii.is_empty())
                .then(|| SearchText::new(entry.ascii))
                .into_iter()
                .chain(
                    city_search_aliases(entry)
                        .iter()
                        .copied()
                        .map(SearchText::new),
                )
                .filter(|alias| !alias.normalized.is_empty())
                .collect(),
            country_aliases: country_search_aliases(entry.country)
                .iter()
                .map(|alias| normalize_search_text(alias))
                .filter(|alias| !alias.is_empty())
                .collect(),
            keywords: zone_search_terms(entry.tz)
                .iter()
                .copied()
                .map(SearchKeyword::new)
                .filter(|term| !term.text.normalized.is_empty())
                .collect(),
            pop_score: population_score(entry.population),
        }
    }
}

/// Log-scale population bonus, so a metropolis with a prefix match
/// can outrank a town with an exact one: typing "york" without "new"
/// still means New York City to most people. 15k people score 18,
/// one million 54, and the scale caps at 84.
fn population_score(population: u64) -> u32 {
    (6 * population.max(1).ilog2().saturating_sub(10)).min(84)
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
/// One table drives the phrase pass, the per-term pass, and nothing
/// else. The order of the entries doubles as the human-readable
/// priority order: city outranks aliases outranks state, and so on.
#[derive(Copy, Clone, Debug)]
enum Field {
    City,
    Aliases,
    State,
    Keywords,
    TzWords,
    Country,
    CountryAliases,
    Region,
    Offset,
}

const FIELD_WEIGHTS: [(Field, FieldWeights); 9] = [
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
        Field::State,
        FieldWeights {
            exact: 150,
            prefix: 115,
            contains: 75,
        },
    ),
    (
        Field::Keywords,
        FieldWeights {
            exact: 140,
            prefix: 105,
            contains: 70,
        },
    ),
    (
        Field::TzWords,
        FieldWeights {
            exact: 130,
            prefix: 100,
            contains: 70,
        },
    ),
    (
        Field::Country,
        FieldWeights {
            exact: 120,
            prefix: 95,
            contains: 65,
        },
    ),
    (
        Field::CountryAliases,
        FieldWeights {
            exact: 110,
            prefix: 85,
            contains: 60,
        },
    ),
    (
        Field::Region,
        FieldWeights {
            exact: 100,
            prefix: 75,
            contains: 50,
        },
    ),
    (
        Field::Offset,
        FieldWeights {
            exact: 150,
            prefix: 120,
            contains: 80,
        },
    ),
];

// ── Scoring ─────────────────────────────────────────────────────────

fn score_phrase_match(
    query: &SearchQuery,
    search: &TimezoneSearchData,
    offset_terms: &[String],
) -> u32 {
    if query.normalized.is_empty() {
        return 0;
    }
    score_against_haystack(&query.normalized, search, offset_terms, &FIELD_WEIGHTS)
}

fn score_search_term(term: &str, search: &TimezoneSearchData, offset_terms: &[String]) -> u32 {
    score_against_haystack(term, search, offset_terms, &FIELD_WEIGHTS)
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
    weights: &[(Field, FieldWeights); 9],
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
            Field::State => score_field(&search.state, query, *w),
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

/// Chooses the label the table shows for a matched row: the city,
/// unless the whole query is an exact or prefix match of an alias or
/// a displayable zone phrase and the city itself matches no better.
/// Per-term hits never relabel a row, so "eastern time" keeps "New
/// York City" instead of surfacing "Times Square".
fn best_display_name(
    entry: &TimezoneEntry,
    search: &TimezoneSearchData,
    query: &SearchQuery,
) -> &'static str {
    let quality = |candidate: &str| match MatchKind::classify(candidate, &query.normalized) {
        Some(MatchKind::Exact) => 2,
        Some(MatchKind::Prefix) => 1,
        _ => 0,
    };
    let city_quality = quality(&search.city.normalized);
    let displayable = search.aliases.iter().chain(
        search
            .keywords
            .iter()
            .filter(|keyword| keyword.display_in_results)
            .map(|keyword| &keyword.text),
    );
    let best = displayable
        .map(|candidate| (candidate.raw, quality(&candidate.normalized)))
        .max_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(b.0)));
    match best {
        Some((label, label_quality)) if label_quality > city_quality && label_quality > 0 => label,
        _ => entry.city,
    }
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
        if starts_with_term(field, term)
            || field
                .split_whitespace()
                .any(|word| word_matches_term(word, term))
        {
            return Some(MatchKind::Prefix);
        }
        if term.len() >= CONTAINS_MIN_CHARS
            && field
                .match_indices(term)
                .any(|(at, _)| ends_a_number(term, &field[at + term.len()..]))
        {
            return Some(MatchKind::Contains);
        }
        None
    }
}

/// Whether `field` starts with `term` where the term's number, if it
/// ends in one, ends too.
fn starts_with_term(field: &str, term: &str) -> bool {
    field
        .strip_prefix(term)
        .is_some_and(|rest| ends_a_number(term, rest))
}

/// False when `term` ends in a digit and `rest`, the text after it,
/// carries the number on. So `1` matches the hour in `+1` and `+1 30`
/// but not in `+10`, and `utc+0` does not match `utc+0800`.
fn ends_a_number(term: &str, rest: &str) -> bool {
    !(term.ends_with(|c: char| c.is_ascii_digit())
        && rest.starts_with(|c: char| c.is_ascii_digit()))
}

/// Returns `true` when `word` is the same as `term`, starts with `term`,
/// or matches `term` after stripping a leading offset sign.
///
/// The sign-strip exists so that bare digit queries (`5`) match offset
/// tokens like `+5` and `-5` inside the cached offset haystacks — the
/// user expects `gmt 5` to find UTC+5 cities without typing the plus.
fn word_matches_term(word: &str, term: &str) -> bool {
    if starts_with_term(word, term) {
        return true;
    }
    let stripped = word.trim_start_matches(['+', '-']);
    stripped != word && starts_with_term(stripped, term)
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

/// Maps a lowercased Latin character with a diacritic to its ASCII
/// base letters, so "Z\u{fc}rich" and "zurich" normalize identically.
/// Characters outside the table keep the pre-existing behavior of
/// acting as separators.
fn fold_diacritic(ch: char) -> Option<&'static str> {
    Some(match ch {
        '\u{e0}'..='\u{e5}' | '\u{101}' | '\u{103}' | '\u{105}' => "a",
        '\u{e6}' => "ae",
        '\u{e7}' | '\u{107}' | '\u{109}' | '\u{10b}' | '\u{10d}' => "c",
        '\u{10f}' | '\u{111}' | '\u{f0}' => "d",
        '\u{e8}'..='\u{eb}' | '\u{113}' | '\u{115}' | '\u{117}' | '\u{119}' | '\u{11b}' => "e",
        '\u{11d}' | '\u{11f}' | '\u{121}' | '\u{123}' => "g",
        '\u{125}' | '\u{127}' => "h",
        '\u{ec}'..='\u{ef}' | '\u{129}' | '\u{12b}' | '\u{12d}' | '\u{12f}' | '\u{131}' => "i",
        '\u{135}' => "j",
        '\u{137}' => "k",
        '\u{13a}' | '\u{13c}' | '\u{13e}' | '\u{140}' | '\u{142}' => "l",
        '\u{f1}' | '\u{144}' | '\u{146}' | '\u{148}' => "n",
        '\u{f2}'..='\u{f6}' | '\u{f8}' | '\u{14d}' | '\u{14f}' | '\u{151}' => "o",
        '\u{153}' => "oe",
        '\u{155}' | '\u{157}' | '\u{159}' => "r",
        '\u{15b}' | '\u{15d}' | '\u{15f}' | '\u{161}' | '\u{219}' => "s",
        '\u{df}' => "ss",
        '\u{163}' | '\u{165}' | '\u{167}' | '\u{21b}' => "t",
        '\u{fe}' => "th",
        '\u{f9}'..='\u{fc}'
        | '\u{169}'
        | '\u{16b}'
        | '\u{16d}'
        | '\u{16f}'
        | '\u{171}'
        | '\u{173}' => "u",
        '\u{175}' => "w",
        '\u{fd}' | '\u{ff}' | '\u{177}' => "y",
        '\u{17a}' | '\u{17c}' | '\u{17e}' => "z",
        _ => return None,
    })
}

fn normalize_search_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut last_was_space = true;
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        let next_is_digit = chars.peek().is_some_and(|next| next.is_ascii_digit());
        for lower in ch.to_lowercase() {
            if let Some(folded) = fold_diacritic(lower) {
                normalized.push_str(folded);
                last_was_space = false;
                continue;
            }
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

    use chrono_tz::Tz;

    use super::*;

    // ── normalize_search_text ──────────────────────────────────────

    #[test]
    fn normalize_lowercases_and_collapses_whitespace() {
        assert_eq!(normalize_search_text("Asia/Tokyo  "), "asia tokyo");
        // Latin diacritics fold to their ASCII base letters, so typed
        // and stored forms meet on one spelling.
        assert_eq!(normalize_search_text("  São Paulo  "), "sao paulo");
        assert_eq!(normalize_search_text("Zürich"), "zurich");
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
        favorites: &HashMap<usize, usize>,
    ) -> Option<Vec<(usize, &'static str, u32)>> {
        let base: Vec<usize> = (0..entries.len()).collect();
        let now = Utc::now();
        index.search(query, entries, &base, &now, favorites)
    }

    /// Mid-January, when no zone in either hemisphere is changing its
    /// offset, so offsets read here are the standard ones.
    fn midwinter() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-01-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn offset_at(entry: &TimezoneEntry, now: &DateTime<Utc>) -> i32 {
        now.with_timezone(&entry.tz)
            .offset()
            .fix()
            .local_minus_utc()
    }

    /// The `(zone, offset seconds, label)` of every result for `query`
    /// in midwinter.
    fn midwinter_results(query: &str) -> Vec<(Tz, i32, &'static str)> {
        let (entries, index) = fixture();
        let base: Vec<usize> = (0..entries.len()).collect();
        let now = midwinter();
        index
            .search(query, entries, &base, &now, &HashMap::new())
            .expect("non-empty query")
            .into_iter()
            .map(|(i, label, _)| (entries[i].tz, offset_at(&entries[i], &now), label))
            .collect()
    }

    /// A bare `utc` or `gmt` lists the zones at UTC and nothing else.
    ///
    /// Given: midwinter, when London is at UTC and Shanghai at UTC+8.
    /// When: the query is `utc`, then `gmt`.
    /// Then: London is listed, and every result is at UTC.
    ///
    /// Why it matters: a user who types `utc` wants the zones on UTC,
    /// and a populous UTC+8 city must not rank among them.
    #[test]
    fn a_bare_utc_or_gmt_lists_only_zones_at_utc() {
        for query in ["utc", "gmt"] {
            let results = midwinter_results(query);

            assert!(
                results.iter().any(|(tz, ..)| *tz == Tz::Europe__London),
                "{query}: London missing"
            );
            let off: Vec<_> = results
                .iter()
                .filter(|(_, offset, _)| *offset != 0)
                .collect();
            assert!(
                off.is_empty(),
                "{query}: not at UTC: {:?}",
                &off[..off.len().min(5)]
            );
        }
    }

    /// A GMT-style offset lists no fixed offset of the opposite sign.
    ///
    /// Given: the fixed rows UTC-10 (zone `Etc/GMT+10`) and UTC+10
    /// (zone `Etc/GMT-10`).
    /// When: the query is `gmt-10`.
    /// Then: Honolulu and UTC-10 are listed, and nothing at UTC+10.
    ///
    /// Why it matters: the IANA name of a fixed offset inverts its sign,
    /// so matching that name lists the opposite offset.
    #[test]
    fn a_gmt_offset_lists_no_fixed_offset_of_the_opposite_sign() {
        let results = midwinter_results("gmt-10");

        assert!(results.iter().any(|(tz, ..)| *tz == Tz::Pacific__Honolulu));
        assert!(results.iter().any(|(_, _, label)| *label == "UTC-10"));
        let opposite: Vec<_> = results
            .iter()
            .filter(|(_, offset, _)| *offset == 36_000)
            .collect();
        assert!(opposite.is_empty(), "listed: {opposite:?}");
    }

    /// A bare hour matches that hour, not a longer offset that starts
    /// with the same digit.
    ///
    /// Given: midwinter, when Paris is at UTC+1 and Sydney at UTC+11.
    /// When: the query is `gmt 1`.
    /// Then: Paris is listed, and nothing ten or more hours from UTC.
    ///
    /// Why it matters: `1` is a whole hour, so UTC+10 and UTC+11 zones
    /// in the list bury the zones the user asked for.
    #[test]
    fn a_bare_hour_matches_that_hour_and_not_longer_offsets() {
        let results = midwinter_results("gmt 1");

        assert!(results.iter().any(|(tz, ..)| *tz == Tz::Europe__Paris));
        let longer: Vec<_> = results
            .iter()
            .filter(|(_, offset, _)| offset.abs() >= 36_000)
            .collect();
        assert!(
            longer.is_empty(),
            "listed: {:?}",
            &longer[..longer.len().min(5)]
        );
    }

    /// An alias names one city, not every city with the same name.
    ///
    /// Given: the alias `Washington DC` and three US cities named
    /// Washington, in DC, Utah and Illinois.
    /// When: the query is `washington dc`.
    /// Then: one row carries the alias, and it is the capital.
    ///
    /// Why it matters: three rows named Washington DC with three
    /// different clocks leave the user to guess which is right.
    #[test]
    fn an_alias_labels_only_the_city_it_names() {
        let results = midwinter_results("washington dc");

        let labelled: Vec<_> = results
            .iter()
            .filter(|(_, _, label)| *label == "Washington DC")
            .collect();
        assert_eq!(labelled.len(), 1, "labelled: {labelled:?}");
        assert_eq!(labelled[0].0, Tz::America__New_York);
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

    /// An offset is found in each form people write it.
    ///
    /// Given: midwinter, when New York is at UTC-5 and Kolkata at
    /// UTC+5:30.
    /// When: the offset is typed as `utc-5`, `gmt-05:00`, `-0500`,
    /// `utc+5:30` and `+0530`.
    /// Then: each lists the city at that offset.
    ///
    /// Why it matters: a user copies an offset from a calendar invite or
    /// a log line, and each source writes it differently.
    #[test]
    fn an_offset_is_found_in_each_form_people_write_it() {
        for (query, tz) in [
            ("utc-5", Tz::America__New_York),
            ("gmt-05:00", Tz::America__New_York),
            ("-0500", Tz::America__New_York),
            ("utc+5:30", Tz::Asia__Kolkata),
            ("+0530", Tz::Asia__Kolkata),
        ] {
            let results = midwinter_results(query);

            assert!(results.iter().any(|(found, ..)| *found == tz), "{query}");
        }
    }

    /// `utc` beside a city name is a word of the query, not a request
    /// for the zones at UTC.
    ///
    /// Given: Tokyo at UTC+9.
    /// When: the query is `utc tokyo`.
    /// Then: Tokyo is the first result.
    ///
    /// Why it matters: only a bare `utc` means UTC+0, and a longer query
    /// must still find its city.
    #[test]
    fn utc_beside_a_city_still_finds_the_city() {
        let results = midwinter_results("utc tokyo");

        assert_eq!(results.first().map(|(tz, ..)| *tz), Some(Tz::Asia__Tokyo));
    }

    /// A favourite wins a tie, and only a tie.
    ///
    /// Given: two rows that score the same for `eastern time`, and York
    /// ranked above New York City for `york`.
    /// When: the lower of the tied rows is a favourite, and then New
    /// York City.
    /// Then: the tied favourite moves ahead, and York stays above the
    /// favourite New York City.
    ///
    /// Why it matters: a favourite should come first among equals, but
    /// a stronger match must not drop below it.
    #[test]
    fn a_favourite_wins_a_tie_and_only_a_tie() {
        let (entries, index) = fixture();
        let base: Vec<usize> = (0..entries.len()).collect();
        let now = midwinter();
        let rank = |query: &str, favourite: Option<usize>, row: usize| {
            let favourites: HashMap<usize, usize> = favourite.map(|i| (i, 0)).into_iter().collect();
            index
                .search(query, entries, &base, &now, &favourites)
                .expect("non-empty query")
                .iter()
                .position(|(i, ..)| *i == row)
                .expect("row listed")
        };

        let plain =
            run_search(&index, entries, "eastern time", &HashMap::new()).expect("non-empty query");
        let tie = plain
            .windows(2)
            .position(|pair| pair[0].2 == pair[1].2)
            .expect("two rows with one score");
        let (ahead, behind) = (plain[tie].0, plain[tie + 1].0);
        assert!(
            rank("eastern time", Some(behind), behind) < rank("eastern time", Some(behind), ahead)
        );

        let new_york = entries
            .iter()
            .position(|e| e.city == "New York City")
            .expect("New York City");
        let york = entries
            .iter()
            .position(|e| e.city == "York" && e.cc == "GB")
            .expect("York");
        assert!(rank("york", Some(new_york), york) < rank("york", Some(new_york), new_york));
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
