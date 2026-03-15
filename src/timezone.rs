//! # Curated timezone catalogue.
//!
//! Provides a hand-picked list of ~50 world cities spanning every
//! inhabited UTC offset from −11 to +14. Entries are ordered by
//! offset in the source to make it easy to spot gaps, but the app
//! sorts them differently at runtime (favorites first, then A-Z).
//!
//! ## Why hardcoded data instead of a library?
//!
//! No Rust crate provides the combination of **display-friendly city
//! names**, **country names**, and **geographic regions** that a
//! world-clock UI needs. The ecosystem was surveyed (March 2026) and
//! every candidate falls short:
//!
//! | Crate | Provides | Missing |
//! |-------|----------|---------|
//! | [`chrono_tz`] | `Tz` enum, IANA identifiers | No city/country/region metadata |
//! | `iana-time-zone` | Detects the *local system's* timezone | Not a data catalogue |
//! | `icu_timezone` (ICU4X) | BCP-47 zone IDs, localised format names | No city/country/region; massive dep tree |
//!
//! The IANA database's `zone1970.tab` only provides 2-letter ISO
//! country codes and lat/long coordinates — no display names, no
//! regions. Even parsing it would give "Kolkata" not "Mumbai", and
//! "US" not "United States".
//!
//! ## Why a static list instead of all 500+ IANA zones?
//!
//! `chrono_tz` ships 500+ zones, most of which are aliases or
//! historical. A curated set keeps the UI scannable and avoids
//! confusing entries like `US/East-Indiana` or `Etc/GMT+5` (which
//! has an inverted sign). Cities were chosen for population size
//! and geographic spread, and display names are editorially chosen
//! (e.g. "Mumbai" over the IANA canonical "Kolkata").

use chrono_tz::Tz;

/// A single timezone entry displayed in the table.
///
/// All string fields are `&'static str` because the data is compiled
/// into the binary — no allocation or file I/O at startup.
pub struct TimezoneEntry {
    /// Display name shown in the City column (e.g. "Mumbai").
    pub city: &'static str,
    /// Country or territory (e.g. "India"). Empty for the UTC entry.
    pub country: &'static str,
    /// Geographic region used for search filtering (e.g. "Asia").
    pub region: &'static str,
    /// IANA timezone identifier from [`chrono_tz`], used for all
    /// time conversions. Note that the city display name may differ
    /// from the IANA name (e.g. "Mumbai" maps to `Asia/Kolkata`).
    pub tz: Tz,
}

/// Returns the full catalogue of curated timezone entries, ordered
/// by UTC offset from −11 (Pago Pago) to +14 (Kiritimati).
pub fn all_timezones() -> Vec<TimezoneEntry> {
    vec![
        // UTC-11
        TimezoneEntry {
            city: "Pago Pago",
            country: "American Samoa",
            region: "Pacific",
            tz: Tz::Pacific__Pago_Pago,
        },
        // UTC-10
        TimezoneEntry {
            city: "Honolulu",
            country: "USA",
            region: "North America",
            tz: Tz::Pacific__Honolulu,
        },
        // UTC-9
        TimezoneEntry {
            city: "Anchorage",
            country: "USA",
            region: "North America",
            tz: Tz::America__Anchorage,
        },
        // UTC-8
        TimezoneEntry {
            city: "Los Angeles",
            country: "USA",
            region: "North America",
            tz: Tz::America__Los_Angeles,
        },
        TimezoneEntry {
            city: "Vancouver",
            country: "Canada",
            region: "North America",
            tz: Tz::America__Vancouver,
        },
        // UTC-7
        TimezoneEntry {
            city: "Denver",
            country: "USA",
            region: "North America",
            tz: Tz::America__Denver,
        },
        TimezoneEntry {
            city: "Phoenix",
            country: "USA",
            region: "North America",
            tz: Tz::America__Phoenix,
        },
        // UTC-6
        TimezoneEntry {
            city: "Chicago",
            country: "USA",
            region: "North America",
            tz: Tz::America__Chicago,
        },
        TimezoneEntry {
            city: "Mexico City",
            country: "Mexico",
            region: "North America",
            tz: Tz::America__Mexico_City,
        },
        // UTC-5
        TimezoneEntry {
            city: "New York",
            country: "USA",
            region: "North America",
            tz: Tz::America__New_York,
        },
        TimezoneEntry {
            city: "Toronto",
            country: "Canada",
            region: "North America",
            tz: Tz::America__Toronto,
        },
        TimezoneEntry {
            city: "Bogota",
            country: "Colombia",
            region: "South America",
            tz: Tz::America__Bogota,
        },
        // UTC-4
        TimezoneEntry {
            city: "Santiago",
            country: "Chile",
            region: "South America",
            tz: Tz::America__Santiago,
        },
        TimezoneEntry {
            city: "Halifax",
            country: "Canada",
            region: "North America",
            tz: Tz::America__Halifax,
        },
        // UTC-3:30
        TimezoneEntry {
            city: "St. John's",
            country: "Canada",
            region: "North America",
            tz: Tz::America__St_Johns,
        },
        // UTC-3
        TimezoneEntry {
            city: "São Paulo",
            country: "Brazil",
            region: "South America",
            tz: Tz::America__Sao_Paulo,
        },
        TimezoneEntry {
            city: "Buenos Aires",
            country: "Argentina",
            region: "South America",
            tz: Tz::America__Argentina__Buenos_Aires,
        },
        // UTC-1
        TimezoneEntry {
            city: "Azores",
            country: "Portugal",
            region: "Atlantic",
            tz: Tz::Atlantic__Azores,
        },
        // UTC+0
        TimezoneEntry {
            city: "UTC",
            country: "",
            region: "",
            tz: Tz::UTC,
        },
        TimezoneEntry {
            city: "London",
            country: "UK",
            region: "Europe",
            tz: Tz::Europe__London,
        },
        TimezoneEntry {
            city: "Reykjavik",
            country: "Iceland",
            region: "Europe",
            tz: Tz::Atlantic__Reykjavik,
        },
        TimezoneEntry {
            city: "Accra",
            country: "Ghana",
            region: "Africa",
            tz: Tz::Africa__Accra,
        },
        // UTC+1
        TimezoneEntry {
            city: "Paris",
            country: "France",
            region: "Europe",
            tz: Tz::Europe__Paris,
        },
        TimezoneEntry {
            city: "Berlin",
            country: "Germany",
            region: "Europe",
            tz: Tz::Europe__Berlin,
        },
        TimezoneEntry {
            city: "Lagos",
            country: "Nigeria",
            region: "Africa",
            tz: Tz::Africa__Lagos,
        },
        // UTC+2
        TimezoneEntry {
            city: "Cairo",
            country: "Egypt",
            region: "Africa",
            tz: Tz::Africa__Cairo,
        },
        TimezoneEntry {
            city: "Athens",
            country: "Greece",
            region: "Europe",
            tz: Tz::Europe__Athens,
        },
        TimezoneEntry {
            city: "Johannesburg",
            country: "South Africa",
            region: "Africa",
            tz: Tz::Africa__Johannesburg,
        },
        // UTC+3
        TimezoneEntry {
            city: "Moscow",
            country: "Russia",
            region: "Europe",
            tz: Tz::Europe__Moscow,
        },
        TimezoneEntry {
            city: "Istanbul",
            country: "Turkey",
            region: "Europe",
            tz: Tz::Europe__Istanbul,
        },
        TimezoneEntry {
            city: "Nairobi",
            country: "Kenya",
            region: "Africa",
            tz: Tz::Africa__Nairobi,
        },
        // UTC+3:30
        TimezoneEntry {
            city: "Tehran",
            country: "Iran",
            region: "Asia",
            tz: Tz::Asia__Tehran,
        },
        // UTC+4
        TimezoneEntry {
            city: "Dubai",
            country: "UAE",
            region: "Asia",
            tz: Tz::Asia__Dubai,
        },
        // UTC+4:30
        TimezoneEntry {
            city: "Kabul",
            country: "Afghanistan",
            region: "Asia",
            tz: Tz::Asia__Kabul,
        },
        // UTC+5
        TimezoneEntry {
            city: "Karachi",
            country: "Pakistan",
            region: "Asia",
            tz: Tz::Asia__Karachi,
        },
        // UTC+5:30
        TimezoneEntry {
            city: "Mumbai",
            country: "India",
            region: "Asia",
            tz: Tz::Asia__Kolkata,
        },
        // UTC+5:45
        TimezoneEntry {
            city: "Kathmandu",
            country: "Nepal",
            region: "Asia",
            tz: Tz::Asia__Kathmandu,
        },
        // UTC+6
        TimezoneEntry {
            city: "Dhaka",
            country: "Bangladesh",
            region: "Asia",
            tz: Tz::Asia__Dhaka,
        },
        // UTC+7
        TimezoneEntry {
            city: "Bangkok",
            country: "Thailand",
            region: "Asia",
            tz: Tz::Asia__Bangkok,
        },
        TimezoneEntry {
            city: "Jakarta",
            country: "Indonesia",
            region: "Asia",
            tz: Tz::Asia__Jakarta,
        },
        // UTC+8
        TimezoneEntry {
            city: "Singapore",
            country: "Singapore",
            region: "Asia",
            tz: Tz::Asia__Singapore,
        },
        TimezoneEntry {
            city: "Shanghai",
            country: "China",
            region: "Asia",
            tz: Tz::Asia__Shanghai,
        },
        TimezoneEntry {
            city: "Hong Kong",
            country: "China",
            region: "Asia",
            tz: Tz::Asia__Hong_Kong,
        },
        TimezoneEntry {
            city: "Perth",
            country: "Australia",
            region: "Australia",
            tz: Tz::Australia__Perth,
        },
        // UTC+9
        TimezoneEntry {
            city: "Tokyo",
            country: "Japan",
            region: "Asia",
            tz: Tz::Asia__Tokyo,
        },
        TimezoneEntry {
            city: "Seoul",
            country: "South Korea",
            region: "Asia",
            tz: Tz::Asia__Seoul,
        },
        // UTC+9:30
        TimezoneEntry {
            city: "Adelaide",
            country: "Australia",
            region: "Australia",
            tz: Tz::Australia__Adelaide,
        },
        // UTC+10
        TimezoneEntry {
            city: "Sydney",
            country: "Australia",
            region: "Australia",
            tz: Tz::Australia__Sydney,
        },
        // UTC+11
        TimezoneEntry {
            city: "Noumea",
            country: "New Caledonia",
            region: "Pacific",
            tz: Tz::Pacific__Noumea,
        },
        // UTC+12
        TimezoneEntry {
            city: "Auckland",
            country: "New Zealand",
            region: "Pacific",
            tz: Tz::Pacific__Auckland,
        },
        // UTC+12:45
        TimezoneEntry {
            city: "Chatham Islands",
            country: "New Zealand",
            region: "Pacific",
            tz: Tz::Pacific__Chatham,
        },
        // UTC+13
        TimezoneEntry {
            city: "Apia",
            country: "Samoa",
            region: "Pacific",
            tz: Tz::Pacific__Apia,
        },
        // UTC+14
        TimezoneEntry {
            city: "Kiritimati",
            country: "Kiribati",
            region: "Pacific",
            tz: Tz::Pacific__Kiritimati,
        },
    ]
}
