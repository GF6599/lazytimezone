//! # Timezone catalogue and solar arithmetic.
//!
//! The city catalogue is generated data: `data/cities.tsv` derives
//! from GeoNames through the `gen_cities` binary (the `gen-cities`
//! recipe) and is embedded at compile time. One row is one city with
//! its state, country, coordinates, population, and IANA timezone. The
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

use chrono::offset::Offset;
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
    /// Where the city is. Drives the day/night colouring via
    /// [`is_daytime_at_coordinates`]: for high-latitude cities the
    /// daylight window varies dramatically by season. `None` for a
    /// fixed offset, which is not a place and so has no sunrise.
    pub coordinates: Option<Coordinates>,
    /// GeoNames population: the ranking signal for search and for
    /// the one-representative-per-zone lookups.
    pub population: u64,
}

impl TimezoneEntry {
    /// True for a UTC offset with no place behind it. Such a row has no
    /// sunrise, and it never observes daylight saving.
    pub fn is_fixed_offset(&self) -> bool {
        self.coordinates.is_none()
    }
}

/// A position in degrees: latitude positive north, longitude positive
/// east.
#[derive(Debug, Clone, Copy)]
pub struct Coordinates {
    pub latitude: f64,
    pub longitude: f64,
}

/// The full city catalogue, parsed from the embedded TSV once.
///
/// A row that fails to parse is skipped. The
/// `every_data_row_parses_into_the_catalogue` test turns any skip
/// into a loud failure, which the `panic` lint forbids expressing
/// here directly.
pub fn all_timezones() -> &'static [TimezoneEntry] {
    static CATALOGUE: OnceLock<Vec<TimezoneEntry>> = OnceLock::new();
    CATALOGUE.get_or_init(|| {
        data_rows(CITIES_TSV)
            .filter_map(parse_city_row)
            .chain(fixed_offset_entries())
            .collect()
    })
}

/// Data lines of an embedded TSV: comment and blank lines skipped.
fn data_rows(tsv: &'static str) -> impl Iterator<Item = &'static str> {
    tsv.lines().filter(|l| !l.starts_with('#') && !l.is_empty())
}

/// Parses one `name  ascii  admin1  country  cc  lat  lon  pop  tz` row.
fn parse_city_row(line: &'static str) -> Option<TimezoneEntry> {
    let mut cols = line.split('\t');
    let city = cols.next()?;
    let ascii = cols.next()?;
    let admin1 = cols.next()?;
    let country = cols.next()?;
    let cc = cols.next()?;
    let latitude: f64 = cols.next()?.parse().ok()?;
    let longitude: f64 = cols.next()?.parse().ok()?;
    let coordinates = Some(Coordinates {
        latitude,
        longitude,
    });
    let population: u64 = cols.next()?.parse().ok()?;
    let tz: Tz = cols.next()?.parse().ok()?;
    Some(TimezoneEntry {
        city,
        ascii,
        admin1,
        country,
        cc,
        tz,
        coordinates,
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

/// The fixed UTC offsets, as display name and the zone that serves it.
///
/// The IANA names invert the sign, so `UTC-5` is served by `Etc/GMT+5`.
/// The name here is the one the user sees and the one the config file
/// stores, so the inverted form never leaves this table.
/// `a_fixed_offset_row_reports_the_offset_its_name_claims` checks every
/// pair against the real offset.
const FIXED_OFFSETS: &[(&str, Tz)] = &[
    ("UTC+14", Tz::Etc__GMTMinus14),
    ("UTC+13", Tz::Etc__GMTMinus13),
    ("UTC+12", Tz::Etc__GMTMinus12),
    ("UTC+11", Tz::Etc__GMTMinus11),
    ("UTC+10", Tz::Etc__GMTMinus10),
    ("UTC+9", Tz::Etc__GMTMinus9),
    ("UTC+8", Tz::Etc__GMTMinus8),
    ("UTC+7", Tz::Etc__GMTMinus7),
    ("UTC+6", Tz::Etc__GMTMinus6),
    ("UTC+5", Tz::Etc__GMTMinus5),
    ("UTC+4", Tz::Etc__GMTMinus4),
    ("UTC+3", Tz::Etc__GMTMinus3),
    ("UTC+2", Tz::Etc__GMTMinus2),
    ("UTC+1", Tz::Etc__GMTMinus1),
    ("UTC", Tz::UTC),
    ("UTC-1", Tz::Etc__GMTPlus1),
    ("UTC-2", Tz::Etc__GMTPlus2),
    ("UTC-3", Tz::Etc__GMTPlus3),
    ("UTC-4", Tz::Etc__GMTPlus4),
    ("UTC-5", Tz::Etc__GMTPlus5),
    ("UTC-6", Tz::Etc__GMTPlus6),
    ("UTC-7", Tz::Etc__GMTPlus7),
    ("UTC-8", Tz::Etc__GMTPlus8),
    ("UTC-9", Tz::Etc__GMTPlus9),
    ("UTC-10", Tz::Etc__GMTPlus10),
    ("UTC-11", Tz::Etc__GMTPlus11),
    ("UTC-12", Tz::Etc__GMTPlus12),
];

fn fixed_offset_entries() -> impl Iterator<Item = TimezoneEntry> {
    FIXED_OFFSETS.iter().map(|&(city, tz)| TimezoneEntry {
        city,
        ascii: "",
        admin1: "",
        country: "",
        cc: "",
        tz,
        coordinates: None,
        population: 0,
    })
}

/// Catalogue index of the most populous city of each zone, in
/// catalogue (population) order. This is the unsearched browse list.
///
/// A fixed offset is left out: browsing lists places, and 27 offsets
/// among the zones would bury them.
pub fn zone_representatives() -> &'static [usize] {
    static REPS: OnceLock<Vec<usize>> = OnceLock::new();
    REPS.get_or_init(|| {
        let mut seen = std::collections::HashSet::new();
        all_timezones()
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.is_fixed_offset())
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
/// [`TimezoneEntry`] should read `entry.coordinates` instead of paying
/// for the search.
pub(crate) fn coordinates_for(tz: Tz) -> Option<Coordinates> {
    all_timezones()
        .iter()
        .find(|e| e.tz == tz)
        .and_then(|e| e.coordinates)
}

/// Returns `(sunrise, sunset)` as fractional hours of solar time, a
/// window centred on solar noon at 12:00, using a simplified
/// solar-position model. [`sun_times`] moves it onto a local clock.
///
/// Inputs:
/// - `latitude_deg` — positive = north, negative = south
/// - `day_of_year` — 1..=366 (e.g. from `chrono::Datelike::ordinal`)
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

    // Hour angle of sunrise/sunset, with the sun's centre 0.833° below
    // the horizon: refraction lifts its image by about 0.567°, and its
    // upper edge sits 0.266° above its centre. Clamped for polar
    // day/night. The `.clamp(-1.0, 1.0)` is load-bearing, not
    // defensive: above the Arctic / below the Antarctic circle the
    // argument to `acos` exceeds [-1, 1] and clamping is what produces
    // the polar day (-> 0) or polar night (-> π) hour angle.
    let horizon = (-0.833_f64).to_radians();
    let cos_omega = ((horizon.sin() - lat.sin() * declination.sin())
        / (lat.cos() * declination.cos()))
    .clamp(-1.0, 1.0);
    let omega = cos_omega.acos();
    let half_day_hours = omega * 12.0 / PI;

    (12.0 - half_day_hours, 12.0 + half_day_hours)
}

/// Solar noon on `local`'s clock, in hours after midnight: 12, plus 4
/// minutes for each degree west, less the equation of time, plus the
/// zone's UTC offset on that date. It can fall past 24, as on
/// Kiritimati at 36.5.
fn solar_noon(longitude: f64, local: &DateTime<Tz>) -> f64 {
    let offset_hours = f64::from(local.offset().fix().local_minus_utc()) / 3600.0;
    12.0 - longitude / 15.0 - equation_of_time_minutes(local.ordinal()) / 60.0 + offset_hours
}

/// How far the sun runs ahead of a uniform clock, in minutes, from
/// about −14 in February to about +16 in November. The usual
/// three-term approximation, good to within a minute.
fn equation_of_time_minutes(day_of_year: u32) -> f64 {
    let b = (360.0 / 365.0 * (f64::from(day_of_year) - 81.0)).to_radians();
    9.87 * (2.0 * b).sin() - 7.53 * b.cos() - 1.5 * b.sin()
}

/// Returns `(sunrise, sunset)` at `place` on `local`'s date, in hours
/// of `local`'s clock. Either can fall below 0 or past 24 when the
/// place's solar noon sits far from 12:00 on its clock.
pub(crate) fn sun_times(place: Coordinates, local: &DateTime<Tz>) -> (f64, f64) {
    let (sunrise, sunset) = sun_window(place.latitude, local.ordinal());
    let shift = solar_noon(place.longitude, local) - 12.0;
    (sunrise + shift, sunset + shift)
}

/// Returns true when `local` falls inside the daytime window for the
/// given timezone. Falls back to a 6..18 window when no curated
/// coordinates exist for `tz`.
pub fn is_daytime_at(tz: Tz, local: &DateTime<Tz>) -> bool {
    match coordinates_for(tz) {
        Some(place) => is_daytime_at_coordinates(place, local),
        None => (6..18).contains(&local.hour()),
    }
}

/// Measures the time from solar noon around the 24-hour clock, so a
/// solar noon that wraps past midnight still holds midday inside the
/// window.
pub fn is_daytime_at_coordinates(place: Coordinates, local: &DateTime<Tz>) -> bool {
    let (sunrise, sunset) = sun_window(place.latitude, local.ordinal());
    let half_day = (sunset - sunrise) / 2.0;
    let hour = local.hour() as f64 + local.minute() as f64 / 60.0;
    let from_noon = (hour - solar_noon(place.longitude, local) + 12.0).rem_euclid(24.0) - 12.0;
    -half_day <= from_noon && from_noon < half_day
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
    use chrono::offset::Offset;

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
    fn every_coordinate_is_in_range() {
        assert!(
            all_timezones()
                .iter()
                .filter_map(|e| e.coordinates)
                .all(|c| (-90.0..=90.0).contains(&c.latitude)
                    && (-180.0..=180.0).contains(&c.longitude))
        );
    }

    /// Sun times match the almanac within minutes.
    ///
    /// Given: London, Madrid and Tokyo at midsummer 2026, and London in
    /// October and at midwinter, with their almanac sunrise and sunset
    /// on the local clock.
    /// When: the sun times are computed from each city's coordinates.
    /// Then: each falls within 6 minutes of the almanac.
    ///
    /// Why it matters: daylight saving moves the clock an hour from the
    /// sun, and Madrid sits far west of its zone's meridian. A window
    /// centred on 12:00 put London's sunrise at 03:47 against a real
    /// 04:43, a whole cell out of place in the day strip. In October
    /// solar noon also runs about 14 minutes early.
    #[test]
    fn sun_times_match_the_almanac_within_minutes() {
        // Given:
        let cases = [
            (Tz::Europe__London, 51.51, -0.13, (6, 21), (4, 43), (21, 21)),
            (
                Tz::Europe__London,
                51.51,
                -0.13,
                (10, 12),
                (7, 22),
                (18, 12),
            ),
            (Tz::Europe__London, 51.51, -0.13, (12, 21), (8, 4), (15, 54)),
            (Tz::Europe__Madrid, 40.42, -3.70, (6, 21), (6, 44), (21, 48)),
            (Tz::Asia__Tokyo, 35.69, 139.69, (6, 21), (4, 25), (19, 0)),
        ];
        for (tz, latitude, longitude, (month, day), rise, set) in cases {
            let date = tz.with_ymd_and_hms(2026, month, day, 12, 0, 0).unwrap();
            let place = Coordinates {
                latitude,
                longitude,
            };

            // When:
            let (sunrise, sunset) = sun_times(place, &date);

            // Then:
            let hours = |(h, m): (u32, u32)| f64::from(h) + f64::from(m) / 60.0;
            assert!(
                approx(sunrise, hours(rise), 0.1) && approx(sunset, hours(set), 0.1),
                "{tz} {month}-{day}: got rise {sunrise:.2}, set {sunset:.2}, almanac {rise:?} {set:?}"
            );
        }
    }

    /// A place whose solar noon wraps past midnight is still day at
    /// midday.
    ///
    /// Given: Tabwakea Village on Kiritimati, at 157°W but on UTC+14.
    /// When: its daylight is checked at local noon and local midnight.
    /// Then: noon is day and midnight is night.
    ///
    /// Why it matters: 12 + 157.49 / 15 + 14 puts solar noon at 36.5 on
    /// the local clock, so a window that does not wrap at 24 never
    /// contains the island's midday.
    #[test]
    fn a_solar_noon_past_midnight_still_makes_midday_day() {
        // Given:
        let place = Coordinates {
            latitude: 2.02,
            longitude: -157.49,
        };
        let tz = Tz::Pacific__Kiritimati;
        let at = |hour| tz.with_ymd_and_hms(2026, 6, 21, hour, 0, 0).unwrap();

        // When:
        let (noon, midnight) = (
            is_daytime_at_coordinates(place, &at(12)),
            is_daytime_at_coordinates(place, &at(0)),
        );

        // Then:
        assert!(noon, "local noon reads as night");
        assert!(!midnight, "local midnight reads as day");
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
            let Some(coordinates) = entry.coordinates else {
                continue;
            };
            for instant in [midyear, midwinter] {
                let local = instant.with_timezone(&entry.tz);
                assert_eq!(
                    is_daytime_at(entry.tz, &local),
                    is_daytime_at_coordinates(coordinates, &local),
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

    /// Spot-checks for well-known city coordinates. These are deliberately
    /// tight (±0.05°) so they catch any misalignment between catalogue
    /// entries and their coordinates, and the cases cover every sign of
    /// both axes. Casey comes from zone.tab rather than GeoNames.
    #[test]
    fn known_city_coordinates_within_tolerance() {
        use chrono_tz::Tz;
        let cases: &[(Tz, f64, f64, &str)] = &[
            (Tz::Europe__London, 51.51, -0.13, "London"),
            (Tz::Asia__Tokyo, 35.69, 139.69, "Tokyo"),
            (Tz::America__New_York, 40.71, -74.01, "New York"),
            (Tz::Australia__Sydney, -33.87, 151.21, "Sydney"),
            (Tz::Atlantic__Reykjavik, 64.13, -21.90, "Reykjavík"),
            (
                Tz::America__Argentina__Buenos_Aires,
                -34.60,
                -58.38,
                "Buenos Aires",
            ),
            (Tz::Asia__Singapore, 1.29, 103.85, "Singapore"),
            (Tz::Africa__Cairo, 30.04, 31.25, "Cairo"),
            (Tz::Pacific__Auckland, -36.85, 174.76, "Auckland"),
            (Tz::America__Los_Angeles, 34.05, -118.24, "Los Angeles"),
            (Tz::America__Resolute, 74.70, -94.83, "Resolute"),
            (Tz::Antarctica__Casey, -66.28, 110.52, "Casey"),
        ];
        for &(tz, latitude, longitude, name) in cases {
            let actual =
                coordinates_for(tz).unwrap_or_else(|| panic!("no coordinates for {name} ({tz:?})"));
            assert!(
                (actual.latitude - latitude).abs() < 0.05
                    && (actual.longitude - longitude).abs() < 0.05,
                "{name}: expected ~({latitude}, {longitude}), got {actual:?}"
            );
        }
    }

    #[test]
    fn every_data_row_parses_into_the_catalogue() {
        // The loader skips a malformed row silently (the panic lint
        // forbids anything louder there), so this count comparison is
        // where a skip becomes a failure. The fixed offsets are built in
        // code rather than read from the file, so they are not counted.
        assert_eq!(
            all_timezones()
                .iter()
                .filter(|e| !e.is_fixed_offset())
                .count(),
            data_rows(CITIES_TSV).count()
        );
    }

    /// The catalogue offers every whole-hour fixed offset.
    /// Given the offsets from UTC-12 to UTC+14,
    /// when the fixed-offset rows are read from the catalogue,
    /// then there is one row for each, and the zero offset is named UTC.
    /// Why it matters: an offset with no row cannot be found or saved.
    #[test]
    fn the_catalogue_offers_utc_and_a_row_for_each_fixed_offset() {
        // Given:
        let mut expected: Vec<String> = (-12..=14)
            .map(|hours: i32| match hours {
                0 => "UTC".to_string(),
                _ => format!("UTC{hours:+}"),
            })
            .collect();
        expected.sort();

        // When:
        let mut names: Vec<&str> = all_timezones()
            .iter()
            .filter(|e| e.is_fixed_offset())
            .map(|e| e.city)
            .collect();
        names.sort_unstable();

        // Then:
        assert_eq!(names, expected);
    }

    /// Each fixed-offset row keeps the offset its name claims.
    /// Given every fixed-offset row in the catalogue,
    /// when each one resolves its offset now,
    /// then the offset equals the hours in its name.
    /// Why it matters: the IANA names invert the sign, since `Etc/GMT+5`
    /// is UTC-5, so a row copied from the name puts the offset on the
    /// wrong side of UTC.
    #[test]
    fn a_fixed_offset_row_reports_the_offset_its_name_claims() {
        // Given:
        let now = chrono::Utc::now();
        for entry in all_timezones().iter().filter(|e| e.is_fixed_offset()) {
            let claimed: i32 = match entry.city.strip_prefix("UTC") {
                Some("") => 0,
                Some(rest) => rest.parse().expect("a signed hour count"),
                None => panic!("a fixed offset must be named UTC, got {}", entry.city),
            };

            // When:
            let actual = now
                .with_timezone(&entry.tz)
                .offset()
                .fix()
                .local_minus_utc();

            // Then:
            assert_eq!(
                actual,
                claimed * 3600,
                "{} resolves to {actual}s",
                entry.city
            );
        }
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

    /// Each zone the main city dump misses shows one real place.
    /// Given zones that `cities15000` holds no city for,
    /// when the catalogue rows for each zone are read,
    /// then each zone holds exactly the one place named here.
    /// Why it matters: a zone with no row cannot be found or saved, and a
    /// row picked by population alone names an Auckland suburb for the
    /// Chatham Islands.
    #[test]
    fn each_zone_the_main_dump_misses_shows_one_real_place() {
        // Given:
        let expected = [
            // GeoNames tags two larger Auckland suburbs with this zone, and
            // the first-order seat must win over them.
            ("Pacific/Chatham", "Waitangi"),
            // No seat here, so the largest settlement wins.
            ("Pacific/Kiritimati", "Tabwakea Village"),
            ("Pacific/Chuuk", "Weno"),
            ("Pacific/Easter", "Hanga Roa"),
            ("America/Iqaluit", "Iqaluit"),
            ("America/Indiana/Vevay", "Vevay"),
            // No GeoNames city at all, so the zone table places these.
            ("Antarctica/Casey", "Casey"),
            ("Australia/Lord_Howe", "Lord Howe"),
            ("Asia/Ust-Nera", "Ust-Nera"),
            ("Pacific/Kanton", "Kanton"),
            // An alias of Pacific/Pago_Pago, and still a place people search for.
            ("Pacific/Midway", "Midway"),
            // The zone name squashes this spelling, and the zone table has it.
            ("Antarctica/DumontDUrville", "Dumont-d'Urville"),
        ];

        for (zone, place) in expected {
            // When:
            let tz: Tz = zone.parse().expect("the test names a real zone");
            let places: Vec<&str> = all_timezones()
                .iter()
                .filter(|e| e.tz == tz)
                .map(|e| e.city)
                .collect();

            // Then:
            assert_eq!(places, [place], "{zone}");
        }
    }
}
