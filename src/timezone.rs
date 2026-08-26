//! # Timezone catalogue and solar arithmetic.
//!
//! The city catalogue is generated data: `data/cities.tsv` derives
//! from GeoNames through the `gen_cities` binary (the `gen-cities`
//! recipe) and is embedded at compile time. One row is one city with
//! its state, country, latitude, population, and IANA timezone. The
//! file is sorted by population, so the catalogue's index order is
//! already a relevance order.
//!
//! `data/zone_names.tsv` carries the curated phrases a zone is known
//! by ("Eastern Time", "AKST", "Hawaii"), which no dump provides.
//!
//! Every string field borrows from the embedded files (`include_str!`
//! data is `'static`), so the only startup allocation is the entry
//! vector itself.

use std::collections::HashMap;
use std::sync::OnceLock;

use chrono::{DateTime, Datelike, Timelike};
use chrono_tz::Tz;

const CITIES_TSV: &str = include_str!("../data/cities.tsv");
const ZONE_NAMES_TSV: &str = include_str!("../data/zone_names.tsv");

/// One city row of the generated catalogue.
#[derive(Debug)]
pub struct TimezoneEntry {
    /// Display name shown in the City column (e.g. "Boston").
    pub city: &'static str,
    /// ASCII transliteration when it differs from `city` (e.g.
    /// "Sao Paulo"), empty otherwise. Search-only.
    pub ascii: &'static str,
    /// State, province, or prefecture. Empty when GeoNames has no
    /// first-level division for the row.
    pub admin1: &'static str,
    /// Country or territory (e.g. "United States").
    pub country: &'static str,
    /// ISO 3166 two-letter country code.
    pub cc: &'static str,
    /// IANA timezone identifier from [`chrono_tz`].
    pub tz: Tz,
    /// Latitude in degrees (positive = north). Drives the day/night
    /// colouring via [`is_daytime_at_latitude`]: for high-latitude
    /// cities the daylight window varies dramatically by season.
    pub latitude: f64,
    /// GeoNames population: the ranking signal for search and for
    /// the one-representative-per-zone lookups.
    pub population: u64,
}

/// The full city catalogue, parsed from the embedded TSV once.
///
/// A row that fails to parse is skipped. The
/// `every_data_row_parses_into_the_catalogue` test turns any skip
/// into a loud failure, which the `panic` lint forbids expressing
/// here directly.
pub fn all_timezones() -> &'static [TimezoneEntry] {
    static CATALOGUE: OnceLock<Vec<TimezoneEntry>> = OnceLock::new();
    CATALOGUE.get_or_init(|| data_rows(CITIES_TSV).filter_map(parse_city_row).collect())
}

/// Data lines of an embedded TSV: comment and blank lines skipped.
fn data_rows(tsv: &'static str) -> impl Iterator<Item = &'static str> {
    tsv.lines().filter(|l| !l.starts_with('#') && !l.is_empty())
}

/// Parses one `name  ascii  admin1  country  cc  lat  pop  tz` row.
fn parse_city_row(line: &'static str) -> Option<TimezoneEntry> {
    let mut cols = line.split('\t');
    let city = cols.next()?;
    let ascii = cols.next()?;
    let admin1 = cols.next()?;
    let country = cols.next()?;
    let cc = cols.next()?;
    let latitude: f64 = cols.next()?.parse().ok()?;
    let population: u64 = cols.next()?.parse().ok()?;
    let tz: Tz = cols.next()?.parse().ok()?;
    Some(TimezoneEntry {
        city,
        ascii,
        admin1,
        country,
        cc,
        tz,
        latitude,
        population,
    })
}

/// A curated search phrase for a zone ("Eastern Time", "HST").
///
/// `display_in_results` marks area names ("Hawaii") that the table
/// may show in place of the city when the phrase is what matched.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SupplementalSearchTerm {
    pub raw: &'static str,
    pub display_in_results: bool,
}

/// Curated phrases for `tz` from the embedded `zone_names.tsv`.
/// Empty for a zone with no curated names.
pub(crate) fn zone_search_terms(tz: Tz) -> &'static [SupplementalSearchTerm] {
    static TERMS: OnceLock<HashMap<Tz, Vec<SupplementalSearchTerm>>> = OnceLock::new();
    let by_zone = TERMS.get_or_init(|| {
        let mut map: HashMap<Tz, Vec<SupplementalSearchTerm>> = HashMap::new();
        for line in data_rows(ZONE_NAMES_TSV) {
            let mut cols = line.split('\t');
            let (Some(zone), Some(raw), Some(display)) = (cols.next(), cols.next(), cols.next())
            else {
                continue;
            };
            let Ok(zone) = zone.parse::<Tz>() else {
                continue;
            };
            map.entry(zone).or_default().push(SupplementalSearchTerm {
                raw,
                display_in_results: display == "1",
            });
        }
        map
    });
    by_zone.get(&tz).map(Vec::as_slice).unwrap_or(&[])
}

const CITY_ALIASES_TSV: &str = include_str!("../data/city_aliases.tsv");

/// Curated aliases for one city: nicknames ("SF"), exonyms
/// ("Cologne"), renames ("Bangalore"), and nearby landmarks below the
/// catalogue's population threshold ("Petra"). Search-only.
pub(crate) fn city_search_aliases(city: &'static str, cc: &'static str) -> &'static [&'static str] {
    static ALIASES: OnceLock<HashMap<(&'static str, &'static str), Vec<&'static str>>> =
        OnceLock::new();
    let by_city = ALIASES.get_or_init(|| {
        let mut map: HashMap<(&'static str, &'static str), Vec<&'static str>> = HashMap::new();
        for line in data_rows(CITY_ALIASES_TSV) {
            let mut cols = line.split('\t');
            let (Some(alias), Some(name), Some(cc)) = (cols.next(), cols.next(), cols.next())
            else {
                continue;
            };
            map.entry((name, cc)).or_default().push(alias);
        }
        map
    });
    by_city.get(&(city, cc)).map(Vec::as_slice).unwrap_or(&[])
}

/// Search aliases for a country: abbreviations and endonyms that the
/// GeoNames canonical names do not carry. Search-only, the table
/// shows the canonical name.
pub(crate) fn country_search_aliases(country: &str) -> &'static [&'static str] {
    match country {
        "United States" => &["US", "USA", "United States of America", "America"],
        "United Kingdom" => &["UK", "GB", "Britain", "Great Britain"],
        "United Arab Emirates" => &["UAE", "Emirates"],
        "New Zealand" => &["NZ", "Aotearoa"],
        "South Africa" => &["SA", "RSA"],
        "China" => &["PRC", "People's Republic of China"],
        "South Korea" => &["ROK", "Korea", "Republic of Korea"],
        "North Korea" => &["DPRK", "Democratic People's Republic of Korea"],
        "Brazil" => &["Brasil"],
        "Spain" => &["Espana", "Espa\u{f1}a"],
        "Germany" => &["Deutschland"],
        "Japan" => &["Nippon", "Nihon"],
        "Russia" => &["Rossiya", "Russian Federation"],
        _ => &[],
    }
}

/// Catalogue index of the most populous city of each zone, in
/// catalogue (population) order. This is the unsearched browse list.
pub fn zone_representatives() -> &'static [usize] {
    static REPS: OnceLock<Vec<usize>> = OnceLock::new();
    REPS.get_or_init(|| {
        let mut seen = std::collections::HashSet::new();
        all_timezones()
            .iter()
            .enumerate()
            .filter(|(_, e)| seen.insert(e.tz))
            .map(|(i, _)| i)
            .collect()
    })
}

/// Continent-level region derived from the IANA identifier, e.g.
/// "America" for `America/New_York`. Search-only.
pub(crate) fn region_of(tz: Tz) -> &'static str {
    tz.name().split('/').next().unwrap_or("")
}

/// Linear over the catalogue. A caller that already holds the
/// [`TimezoneEntry`] should read `entry.latitude` and call
/// [`is_daytime_at_latitude`] instead of paying for the search.
pub(crate) fn latitude_for(tz: Tz) -> Option<f64> {
    all_timezones()
        .iter()
        .find(|e| e.tz == tz)
        .map(|e| e.latitude)
}

/// Returns `(sunrise, sunset)` as fractional hours of local clock
/// time using a simplified solar-position model.
///
/// Inputs:
/// - `latitude_deg` — positive = north, negative = south
/// - `day_of_year` — 1..=366 (e.g. from `chrono::Datelike::ordinal`)
///
/// The model assumes solar noon at 12:00 local time, which is off by
/// the equation of time (±15 min through the year) and the city's
/// longitude position within its timezone. For a "is it daytime?"
/// boolean the error is negligible except within minutes of sunrise
/// or sunset, where the question itself is ambiguous.
///
/// Polar regions:
/// - Polar day → returns `(0.0, 24.0)` (sun never sets)
/// - Polar night → returns `(12.0, 12.0)` (sun never rises — empty window)
pub(crate) fn sun_window(latitude_deg: f64, day_of_year: u32) -> (f64, f64) {
    use std::f64::consts::PI;

    let lat = latitude_deg.to_radians();
    // Cooper's formula for solar declination — accurate to within ~0.5°.
    // The 365.0 denominator ignores leap years, introducing a ~1-day phase
    // error that's negligible for a binary day/night decision.
    let declination =
        (-23.44_f64).to_radians() * (2.0 * PI / 365.0 * (day_of_year as f64 + 10.0)).cos();

    // Hour angle of sunrise/sunset, clamped for polar day/night.
    // The `.clamp(-1.0, 1.0)` is load-bearing, not defensive: above the
    // Arctic / below the Antarctic circle the argument to `acos` exceeds
    // [-1, 1] and clamping is what produces the polar day (-> 0) or
    // polar night (-> π) hour angle.
    let cos_omega = (-lat.tan() * declination.tan()).clamp(-1.0, 1.0);
    let omega = cos_omega.acos();
    let half_day_hours = omega * 12.0 / PI;

    (12.0 - half_day_hours, 12.0 + half_day_hours)
}

/// Returns true when `local` falls inside the daytime window for the
/// given timezone. Falls back to a 6..18 window when no curated
/// latitude exists for `tz`.
pub fn is_daytime_at(tz: Tz, local: &DateTime<Tz>) -> bool {
    match latitude_for(tz) {
        Some(lat) => is_daytime_at_latitude(lat, local),
        None => (6..18).contains(&local.hour()),
    }
}

pub fn is_daytime_at_latitude(latitude_deg: f64, local: &DateTime<Tz>) -> bool {
    let (sunrise, sunset) = sun_window(latitude_deg, local.ordinal());
    let hour = local.hour() as f64 + local.minute() as f64 / 60.0;
    hour >= sunrise && hour < sunset
}

/// Formats a UTC offset in seconds to a human-readable string like
/// `UTC+5` or `UTC+5:30`.
///
/// Used by both the table column ([`crate::ui`]) and the search scoring
/// haystack ([`crate::search`]) — kept here in [`timezone`] because UTC
/// offsets are a timezone concern, not a search concern.
pub fn format_utc_offset(total_secs: i32) -> String {
    let sign = if total_secs >= 0 { '+' } else { '-' };
    let abs = total_secs.unsigned_abs();
    let hours = abs / 3600;
    let mins = (abs % 3600) / 60;
    if mins == 0 {
        format!("UTC{}{}", sign, hours)
    } else {
        format!("UTC{}{}:{:02}", sign, hours, mins)
    }
}

#[cfg(test)]
mod tests {
    // Tests panic on failure by design — see src/app.rs for the
    // rationale on why production lints are relaxed inside test modules.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;
    use chrono::TimeZone;

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn the_catalogue_is_city_scaled_not_zone_scaled() {
        let count = all_timezones().len();

        assert!(count > 30_000, "got {count} entries");
    }

    #[test]
    fn boston_massachusetts_is_a_real_row_not_an_alias() {
        let boston = all_timezones()
            .iter()
            .find(|e| e.city == "Boston" && e.country == "United States");

        assert!(boston.is_some());
    }

    #[test]
    fn every_latitude_is_in_range() {
        assert!(
            all_timezones()
                .iter()
                .all(|e| (-90.0..=90.0).contains(&e.latitude))
        );
    }

    #[test]
    fn equator_equinox_is_twelve_hour_day() {
        // Day 80 ≈ March 21 (vernal equinox)
        let (sunrise, sunset) = sun_window(0.0, 80);
        assert!(approx(sunrise, 6.0, 0.5));
        assert!(approx(sunset, 18.0, 0.5));
    }

    #[test]
    fn high_latitude_summer_is_long_day() {
        // Reykjavík (64°N) in late June — sun barely sets
        let (sunrise, sunset) = sun_window(64.13, 172);
        assert!(sunset - sunrise > 20.0);
    }

    #[test]
    fn high_latitude_winter_is_short_day() {
        // Reykjavík in late December — only ~4 hours of daylight
        let (sunrise, sunset) = sun_window(64.13, 355);
        assert!(sunset - sunrise < 6.0);
    }

    #[test]
    fn polar_summer_is_polar_day() {
        // 80°N in mid-summer — sun never sets
        let (sunrise, sunset) = sun_window(80.0, 172);
        assert_eq!(sunrise, 0.0);
        assert_eq!(sunset, 24.0);
    }

    #[test]
    fn both_daytime_entry_points_agree_across_the_catalogue_and_the_year() {
        // The clocks colour day/night through the zone lookup, which
        // reads the zone's first (most populous) row. The table reads
        // each row's own latitude. The shared row is where both paths
        // must agree, or one frame colours it two ways.
        let midyear = chrono::Utc
            .with_ymd_and_hms(2026, 6, 21, 12, 0, 0)
            .single()
            .expect("valid instant");
        let midwinter = chrono::Utc
            .with_ymd_and_hms(2026, 12, 21, 12, 0, 0)
            .single()
            .expect("valid instant");

        let mut seen = std::collections::HashSet::new();
        for entry in all_timezones() {
            if !seen.insert(entry.tz) {
                continue;
            }
            for instant in [midyear, midwinter] {
                let local = instant.with_timezone(&entry.tz);
                assert_eq!(
                    is_daytime_at(entry.tz, &local),
                    is_daytime_at_latitude(entry.latitude, &local),
                    "{} disagrees at {local}",
                    entry.city
                );
            }
        }
    }

    #[test]
    fn polar_winter_is_polar_night() {
        // 80°N in mid-winter — sun never rises
        let (sunrise, sunset) = sun_window(80.0, 355);
        assert_eq!(sunrise, 12.0);
        assert_eq!(sunset, 12.0);
    }

    #[test]
    fn major_cities_have_curated_latitudes() {
        for tz in [
            Tz::Europe__London,
            Tz::Asia__Tokyo,
            Tz::America__New_York,
            Tz::Australia__Sydney,
            Tz::Atlantic__Reykjavik,
        ] {
            assert!(latitude_for(tz).is_some(), "missing latitude for {tz:?}");
        }
    }

    /// Spot-checks for well-known city latitudes. These are deliberately tight
    /// (±0.05°) so they catch any misalignment between catalogue entries and
    /// their latitude data. If you add a new city, please add a row here.
    #[test]
    fn known_city_latitudes_within_tolerance() {
        use chrono_tz::Tz;
        let cases: &[(Tz, f64, &str)] = &[
            (Tz::Europe__London, 51.51, "London"),
            (Tz::Asia__Tokyo, 35.69, "Tokyo"),
            (Tz::America__New_York, 40.71, "New York"),
            (Tz::Australia__Sydney, -33.87, "Sydney"),
            (Tz::Atlantic__Reykjavik, 64.13, "Reykjavík"),
            (Tz::America__Argentina__Buenos_Aires, -34.60, "Buenos Aires"),
            (Tz::Asia__Singapore, 1.29, "Singapore"),
            (Tz::Africa__Cairo, 30.04, "Cairo"),
            (Tz::Pacific__Auckland, -36.85, "Auckland"),
            (Tz::America__Los_Angeles, 34.05, "Los Angeles"),
        ];
        for (tz, expected, name) in cases {
            let actual =
                latitude_for(*tz).unwrap_or_else(|| panic!("no latitude for {} ({:?})", name, tz));
            assert!(
                (actual - expected).abs() < 0.05,
                "{} latitude mismatch: expected ~{}, got {}",
                name,
                expected,
                actual
            );
        }
    }

    #[test]
    fn every_data_row_parses_into_the_catalogue() {
        // The loader skips a malformed row silently (the panic lint
        // forbids anything louder there), so this count comparison is
        // where a skip becomes a failure.
        assert_eq!(all_timezones().len(), data_rows(CITIES_TSV).count());
    }

    #[test]
    fn every_zone_name_targets_a_catalogue_zone() {
        for line in data_rows(ZONE_NAMES_TSV) {
            let zone = line.split('\t').next().unwrap_or("");
            let tz: Tz = zone
                .parse()
                .unwrap_or_else(|_| panic!("zone_names.tsv has a bad zone {zone:?}"));
            assert!(
                all_timezones().iter().any(|e| e.tz == tz),
                "zone_names.tsv names {zone}, which has no catalogue row"
            );
        }
    }

    #[test]
    fn every_city_alias_targets_a_catalogue_row() {
        for line in data_rows(CITY_ALIASES_TSV) {
            let mut cols = line.split('\t');
            let (Some(alias), Some(city), Some(cc)) = (cols.next(), cols.next(), cols.next())
            else {
                panic!("city_aliases.tsv has a short row: {line:?}");
            };
            assert!(
                all_timezones().iter().any(|e| e.city == city && e.cc == cc),
                "alias {alias:?} targets {city:?} {cc:?}, which is not a catalogue row"
            );
        }
    }

    /// Helper for the findability tests below: assert that some
    /// catalogue row in `tz` carries `term` as its name, its ASCII
    /// form, a curated city alias, or a curated zone phrase.
    /// Case-insensitive, because short codes are stored upper-case
    /// but typed in any case.
    fn assert_findable(tz: Tz, term: &str) {
        let found = all_timezones().iter().filter(|e| e.tz == tz).any(|e| {
            e.city.eq_ignore_ascii_case(term)
                || e.ascii.eq_ignore_ascii_case(term)
                || city_search_aliases(e.city, e.cc)
                    .iter()
                    .any(|a| a.eq_ignore_ascii_case(term))
                || zone_search_terms(e.tz)
                    .iter()
                    .any(|s| s.raw.eq_ignore_ascii_case(term))
        });
        assert!(found, "{tz:?} should be findable as {term:?}");
    }

    /// Section A — two- and three-letter city shortcuts. These were the
    /// biggest losses in the findability review because `score_field`'s
    /// 3-char minimum on contains-mode meant they never matched.
    #[test]
    fn nyc_la_sf_kl_resolve_to_their_cities() {
        assert_findable(Tz::America__New_York, "NYC");
        assert_findable(Tz::America__New_York, "NY");
        assert_findable(Tz::America__Los_Angeles, "LA");
        assert_findable(Tz::America__Los_Angeles, "SF");
        assert_findable(Tz::Asia__Kuala_Lumpur, "KL");
        assert_findable(Tz::Asia__Hong_Kong, "HK");
        assert_findable(Tz::Asia__Singapore, "SG");
    }

    /// Section B — IATA airport codes route to the city whose timezone the
    /// airport actually sits in. The Houston/Dallas case is the most
    /// surprising one (Central Time → America/Chicago), so it's pinned here.
    #[test]
    fn airport_codes_lhr_jfk_hnd_route_to_expected_tz() {
        assert_findable(Tz::Europe__London, "LHR");
        assert_findable(Tz::America__New_York, "JFK");
        assert_findable(Tz::America__New_York, "ATL"); // Atlanta is Eastern.
        assert_findable(Tz::America__Chicago, "DFW"); // Dallas is Central.
        assert_findable(Tz::America__Los_Angeles, "SFO"); // SF is Pacific.
        assert_findable(Tz::Asia__Tokyo, "HND");
        assert_findable(Tz::Asia__Tokyo, "NRT");
        assert_findable(Tz::Asia__Singapore, "SIN");
        assert_findable(Tz::Pacific__Auckland, "AKL");
    }

    /// Section C — historical / colloquial city and country names route to
    /// the modern catalogue entry without overriding its display label.
    #[test]
    fn historical_names_bombay_kiev_route_correctly() {
        assert_findable(Tz::Asia__Kolkata, "Bombay");
        assert_findable(Tz::Asia__Kolkata, "Calcutta");
        assert_findable(Tz::Asia__Kolkata, "Madras");
        assert_findable(Tz::Europe__Kyiv, "Kiev");
        assert_findable(Tz::Asia__Yangon, "Burma");
        assert_findable(Tz::Asia__Colombo, "Ceylon");
        assert_findable(Tz::Asia__Tehran, "Persia");
        assert_findable(Tz::Asia__Bangkok, "Siam");
        assert_findable(Tz::Europe__Amsterdam, "Holland");
        // Saigon was already aliased before this change — verify it's still
        // resolvable so a future cleanup doesn't accidentally drop it.
        assert_findable(Tz::Asia__Ho_Chi_Minh, "Saigon");
    }

    /// Section D — country abbreviations and endonyms resolve via
    /// `country_search_aliases`. Spot-checks the most common cases plus a
    /// pre-existing one to lock the contract in.
    #[test]
    fn country_aliases_cover_major_economies() {
        let cases: &[(&str, &str)] = &[
            ("United States", "America"), // pre-existing
            ("New Zealand", "NZ"),
            ("South Africa", "SA"),
            ("China", "PRC"),
            ("South Korea", "ROK"),
            ("Brazil", "Brasil"),
            ("Germany", "Deutschland"),
            ("Japan", "Nippon"),
            ("Russia", "Rossiya"),
            ("Spain", "Espana"),
        ];
        for (country, alias) in cases {
            let aliases = country_search_aliases(country);
            assert!(
                aliases.iter().any(|a| a.eq_ignore_ascii_case(alias)),
                "country {country:?} should expose alias {alias:?}, got {aliases:?}",
            );
        }
    }
}
