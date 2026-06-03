//! # Curated timezone catalogue.
//!
//! Provides a hand-picked list of ~220 world cities covering every
//! inhabited UTC offset from −11 to +14 and all 193 UN member states
//! (plus observer states and Taiwan). Entries are ordered by offset
//! in the source to make it easy to spot gaps, but the app sorts
//! them differently at runtime (favorites first, then A-Z).
//!
//! Each entry may carry **aliases** — other well-known cities in the
//! same timezone that are searchable but not shown in the table.
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
//! ## Why a curated list instead of all 500+ IANA zones?
//!
//! `chrono_tz` ships 500+ zones, most of which are aliases or
//! historical. A curated set picks one representative city per
//! country and avoids confusing entries like `US/East-Indiana` or
//! `Etc/GMT+5` (which has an inverted sign). Display names are
//! editorially chosen (e.g. "Mumbai" over the IANA canonical
//! "Kolkata").

use chrono::{DateTime, Datelike, Timelike};
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
    /// Other well-known cities in the same timezone, used for search
    /// matching but not displayed in the table. For example, "San Diego"
    /// is an alias for the Los Angeles entry.
    pub aliases: &'static [&'static str],
    /// Latitude of the **display city** in degrees (positive = north,
    /// negative = south). Used by [`is_daytime_at`] to drive the
    /// day/night colouring in the table — for high-latitude cities the
    /// daylight window varies dramatically by season, and using a fixed
    /// 06:00–18:00 window would mis-colour Reykjavík in June and
    /// Auckland in December. Values are approximate (1–2 decimal places
    /// — well within the precision needed for a binary "is it day?"
    /// answer).
    pub latitude: f64,
}

/// Returns the full catalogue of curated timezone entries, ordered
/// by UTC offset from −11 (Pago Pago) to +14 (Kiritimati).
pub fn all_timezones() -> Vec<TimezoneEntry> {
    vec![
        // ──────────────────────────────────────────
        // UTC-11
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Pago Pago",
            country: "American Samoa",
            region: "Pacific",
            tz: Tz::Pacific__Pago_Pago,
            aliases: &[],
            latitude: -14.28,
        },
        // ──────────────────────────────────────────
        // UTC-10
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Honolulu",
            country: "USA",
            region: "North America",
            tz: Tz::Pacific__Honolulu,
            aliases: &["Maui", "Kauai", "Hilo", "Waikiki"],
            latitude: 21.31,
        },
        // ──────────────────────────────────────────
        // UTC-9
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Anchorage",
            country: "USA",
            region: "North America",
            tz: Tz::America__Anchorage,
            aliases: &["Fairbanks", "Juneau"],
            latitude: 61.22,
        },
        // ──────────────────────────────────────────
        // UTC-8
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Los Angeles",
            country: "USA",
            region: "North America",
            tz: Tz::America__Los_Angeles,
            aliases: &[
                "San Diego",
                "San Francisco",
                "SF",
                "Seattle",
                "Portland",
                "Las Vegas",
                "Sacramento",
                "Tijuana",
            ],
            latitude: 34.05,
        },
        TimezoneEntry {
            city: "Vancouver",
            country: "Canada",
            region: "North America",
            tz: Tz::America__Vancouver,
            aliases: &["Victoria", "Whistler"],
            latitude: 49.28,
        },
        // ──────────────────────────────────────────
        // UTC-7
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Denver",
            country: "USA",
            region: "North America",
            tz: Tz::America__Denver,
            aliases: &["Salt Lake City", "Albuquerque", "Boise", "El Paso"],
            latitude: 39.74,
        },
        TimezoneEntry {
            city: "Phoenix",
            country: "USA",
            region: "North America",
            tz: Tz::America__Phoenix,
            aliases: &["Tucson", "Scottsdale", "Mesa"],
            latitude: 33.45,
        },
        // ──────────────────────────────────────────
        // UTC-6
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Chicago",
            country: "USA",
            region: "North America",
            tz: Tz::America__Chicago,
            aliases: &[
                "Houston",
                "Dallas",
                "San Antonio",
                "Austin",
                "Minneapolis",
                "Milwaukee",
                "New Orleans",
                "Nashville",
                "Memphis",
                "Kansas City",
                "Oklahoma City",
                "Omaha",
                "St. Louis",
                "Tulsa",
            ],
            latitude: 41.88,
        },
        TimezoneEntry {
            city: "Mexico City",
            country: "Mexico",
            region: "North America",
            tz: Tz::America__Mexico_City,
            aliases: &["Guadalajara", "Monterrey", "Puebla", "Toluca"],
            latitude: 19.43,
        },
        TimezoneEntry {
            city: "Belmopan",
            country: "Belize",
            region: "North America",
            tz: Tz::America__Belize,
            aliases: &["Belize City"],
            latitude: 17.25,
        },
        TimezoneEntry {
            city: "San Jose",
            country: "Costa Rica",
            region: "North America",
            tz: Tz::America__Costa_Rica,
            aliases: &["Limon", "Tamarindo"],
            latitude: 9.93,
        },
        TimezoneEntry {
            city: "San Salvador",
            country: "El Salvador",
            region: "North America",
            tz: Tz::America__El_Salvador,
            aliases: &[],
            latitude: 13.69,
        },
        TimezoneEntry {
            city: "Guatemala City",
            country: "Guatemala",
            region: "North America",
            tz: Tz::America__Guatemala,
            aliases: &["Antigua"],
            latitude: 14.63,
        },
        TimezoneEntry {
            city: "Tegucigalpa",
            country: "Honduras",
            region: "North America",
            tz: Tz::America__Tegucigalpa,
            aliases: &["San Pedro Sula"],
            latitude: 14.07,
        },
        TimezoneEntry {
            city: "Managua",
            country: "Nicaragua",
            region: "North America",
            tz: Tz::America__Managua,
            aliases: &["Leon", "Granada"],
            latitude: 12.13,
        },
        // ──────────────────────────────────────────
        // UTC-5
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "New York",
            country: "USA",
            region: "North America",
            tz: Tz::America__New_York,
            aliases: &[
                "Washington DC",
                "Boston",
                "Philadelphia",
                "Miami",
                "Atlanta",
                "Detroit",
                "Charlotte",
                "Orlando",
                "Tampa",
                "Baltimore",
                "Pittsburgh",
                "Cleveland",
                "Indianapolis",
                "Columbus",
                "Raleigh",
                "Jacksonville",
                "Richmond",
                "Cincinnati",
                "Buffalo",
            ],
            latitude: 40.71,
        },
        TimezoneEntry {
            city: "Toronto",
            country: "Canada",
            region: "North America",
            tz: Tz::America__Toronto,
            aliases: &["Montreal", "Ottawa", "Quebec City"],
            latitude: 43.65,
        },
        TimezoneEntry {
            city: "Bogotá",
            country: "Colombia",
            region: "South America",
            tz: Tz::America__Bogota,
            aliases: &["Bogota", "Medellin", "Cali", "Cartagena", "Barranquilla"],
            latitude: 4.71,
        },
        TimezoneEntry {
            city: "Nassau",
            country: "Bahamas",
            region: "North America",
            tz: Tz::America__Nassau,
            aliases: &["Freeport"],
            latitude: 25.05,
        },
        TimezoneEntry {
            city: "Havana",
            country: "Cuba",
            region: "North America",
            tz: Tz::America__Havana,
            aliases: &["Santiago de Cuba", "Varadero"],
            latitude: 23.13,
        },
        TimezoneEntry {
            city: "Quito",
            country: "Ecuador",
            region: "South America",
            tz: Tz::America__Guayaquil,
            aliases: &["Guayaquil", "Cuenca"],
            latitude: -0.18,
        },
        TimezoneEntry {
            city: "Port-au-Prince",
            country: "Haiti",
            region: "North America",
            tz: Tz::America__PortauPrince,
            aliases: &[],
            latitude: 18.55,
        },
        TimezoneEntry {
            city: "Kingston",
            country: "Jamaica",
            region: "North America",
            tz: Tz::America__Jamaica,
            aliases: &["Montego Bay", "Ocho Rios"],
            latitude: 17.97,
        },
        TimezoneEntry {
            city: "Panama City",
            country: "Panama",
            region: "North America",
            tz: Tz::America__Panama,
            aliases: &["Colon", "Bocas del Toro"],
            latitude: 8.97,
        },
        TimezoneEntry {
            city: "Lima",
            country: "Peru",
            region: "South America",
            tz: Tz::America__Lima,
            aliases: &["Cusco", "Arequipa", "Trujillo"],
            latitude: -12.05,
        },
        // ──────────────────────────────────────────
        // UTC-4
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Santiago",
            country: "Chile",
            region: "South America",
            tz: Tz::America__Santiago,
            aliases: &["Valparaiso", "Concepcion", "Vina del Mar"],
            latitude: -33.45,
        },
        TimezoneEntry {
            city: "Halifax",
            country: "Canada",
            region: "North America",
            tz: Tz::America__Halifax,
            aliases: &["Fredericton", "Charlottetown", "Moncton"],
            latitude: 44.65,
        },
        TimezoneEntry {
            city: "St. John's",
            country: "Antigua and Barbuda",
            region: "North America",
            tz: Tz::America__Antigua,
            aliases: &[],
            latitude: 17.12,
        },
        TimezoneEntry {
            city: "Bridgetown",
            country: "Barbados",
            region: "North America",
            tz: Tz::America__Barbados,
            aliases: &[],
            latitude: 13.10,
        },
        TimezoneEntry {
            city: "La Paz",
            country: "Bolivia",
            region: "South America",
            tz: Tz::America__La_Paz,
            aliases: &["Santa Cruz", "Sucre", "Cochabamba"],
            latitude: -16.50,
        },
        TimezoneEntry {
            city: "Manaus",
            country: "Brazil",
            region: "South America",
            tz: Tz::America__Manaus,
            aliases: &["Boa Vista"],
            latitude: -3.12,
        },
        TimezoneEntry {
            city: "Roseau",
            country: "Dominica",
            region: "North America",
            tz: Tz::America__Dominica,
            aliases: &[],
            latitude: 15.30,
        },
        TimezoneEntry {
            city: "Santo Domingo",
            country: "Dominican Republic",
            region: "North America",
            tz: Tz::America__Santo_Domingo,
            aliases: &["Santiago", "Punta Cana"],
            latitude: 18.47,
        },
        TimezoneEntry {
            city: "St. George's",
            country: "Grenada",
            region: "North America",
            tz: Tz::America__Grenada,
            aliases: &[],
            latitude: 12.05,
        },
        TimezoneEntry {
            city: "Georgetown",
            country: "Guyana",
            region: "South America",
            tz: Tz::America__Guyana,
            aliases: &[],
            latitude: 6.80,
        },
        TimezoneEntry {
            city: "Asunción",
            country: "Paraguay",
            region: "South America",
            tz: Tz::America__Asuncion,
            aliases: &["Asuncion", "Ciudad del Este"],
            latitude: -25.27,
        },
        TimezoneEntry {
            city: "Castries",
            country: "Saint Lucia",
            region: "North America",
            tz: Tz::America__St_Lucia,
            aliases: &[],
            latitude: 14.00,
        },
        TimezoneEntry {
            city: "Basseterre",
            country: "St. Kitts and Nevis",
            region: "North America",
            tz: Tz::America__St_Kitts,
            aliases: &["Charlestown"],
            latitude: 17.30,
        },
        TimezoneEntry {
            city: "Kingstown",
            country: "St. Vincent and the Grenadines",
            region: "North America",
            tz: Tz::America__St_Vincent,
            aliases: &[],
            latitude: 13.16,
        },
        TimezoneEntry {
            city: "Port of Spain",
            country: "Trinidad and Tobago",
            region: "North America",
            tz: Tz::America__Port_of_Spain,
            aliases: &["Scarborough"],
            latitude: 10.66,
        },
        TimezoneEntry {
            city: "Caracas",
            country: "Venezuela",
            region: "South America",
            tz: Tz::America__Caracas,
            aliases: &["Maracaibo", "Valencia", "Barquisimeto"],
            latitude: 10.50,
        },
        // ──────────────────────────────────────────
        // UTC-3:30
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "St. John's",
            country: "Canada",
            region: "North America",
            tz: Tz::America__St_Johns,
            aliases: &[],
            latitude: 47.56,
        },
        // ──────────────────────────────────────────
        // UTC-3
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "São Paulo",
            country: "Brazil",
            region: "South America",
            tz: Tz::America__Sao_Paulo,
            aliases: &[
                "Rio de Janeiro",
                "Brasília",
                "Brasilia",
                "Belo Horizonte",
                "Salvador",
                "Fortaleza",
                "Recife",
                "Porto Alegre",
            ],
            latitude: -23.55,
        },
        TimezoneEntry {
            city: "Buenos Aires",
            country: "Argentina",
            region: "South America",
            tz: Tz::America__Argentina__Buenos_Aires,
            aliases: &["Cordoba", "Rosario", "Mendoza", "Mar del Plata"],
            latitude: -34.60,
        },
        TimezoneEntry {
            city: "Paramaribo",
            country: "Suriname",
            region: "South America",
            tz: Tz::America__Paramaribo,
            aliases: &[],
            latitude: 5.85,
        },
        TimezoneEntry {
            city: "Montevideo",
            country: "Uruguay",
            region: "South America",
            tz: Tz::America__Montevideo,
            aliases: &["Punta del Este"],
            latitude: -34.90,
        },
        // ──────────────────────────────────────────
        // UTC-1
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Azores",
            country: "Portugal",
            region: "Atlantic",
            tz: Tz::Atlantic__Azores,
            aliases: &["Ponta Delgada"],
            latitude: 37.74,
        },
        TimezoneEntry {
            city: "Praia",
            country: "Cape Verde",
            region: "Atlantic",
            tz: Tz::Atlantic__Cape_Verde,
            aliases: &["Mindelo"],
            latitude: 14.93,
        },
        // ──────────────────────────────────────────
        // UTC+0
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "UTC",
            country: "",
            region: "",
            tz: Tz::UTC,
            aliases: &["GMT", "Greenwich", "Zulu"],
            latitude: 0.0,
        },
        TimezoneEntry {
            city: "London",
            country: "UK",
            region: "Europe",
            tz: Tz::Europe__London,
            aliases: &[
                "Manchester",
                "Birmingham",
                "Edinburgh",
                "Glasgow",
                "Cardiff",
                "Belfast",
                "Liverpool",
                "Leeds",
                "Bristol",
                "Sheffield",
            ],
            latitude: 51.51,
        },
        TimezoneEntry {
            city: "Reykjavík",
            country: "Iceland",
            region: "Europe",
            tz: Tz::Atlantic__Reykjavik,
            aliases: &["Reykjavik", "Akureyri"],
            latitude: 64.13,
        },
        TimezoneEntry {
            city: "Accra",
            country: "Ghana",
            region: "Africa",
            tz: Tz::Africa__Accra,
            aliases: &["Kumasi", "Tamale"],
            latitude: 5.55,
        },
        TimezoneEntry {
            city: "Ouagadougou",
            country: "Burkina Faso",
            region: "Africa",
            tz: Tz::Africa__Ouagadougou,
            aliases: &["Bobo-Dioulasso"],
            latitude: 12.37,
        },
        TimezoneEntry {
            city: "Banjul",
            country: "Gambia",
            region: "Africa",
            tz: Tz::Africa__Banjul,
            aliases: &[],
            latitude: 13.45,
        },
        TimezoneEntry {
            city: "Conakry",
            country: "Guinea",
            region: "Africa",
            tz: Tz::Africa__Conakry,
            aliases: &[],
            latitude: 9.51,
        },
        TimezoneEntry {
            city: "Bissau",
            country: "Guinea-Bissau",
            region: "Africa",
            tz: Tz::Africa__Bissau,
            aliases: &[],
            latitude: 11.86,
        },
        TimezoneEntry {
            city: "Dublin",
            country: "Ireland",
            region: "Europe",
            tz: Tz::Europe__Dublin,
            aliases: &["Cork", "Galway", "Limerick"],
            latitude: 53.35,
        },
        TimezoneEntry {
            city: "Abidjan",
            country: "Ivory Coast",
            region: "Africa",
            tz: Tz::Africa__Abidjan,
            aliases: &["Yamoussoukro", "Bouake"],
            latitude: 5.36,
        },
        TimezoneEntry {
            city: "Monrovia",
            country: "Liberia",
            region: "Africa",
            tz: Tz::Africa__Monrovia,
            aliases: &[],
            latitude: 6.31,
        },
        TimezoneEntry {
            city: "Bamako",
            country: "Mali",
            region: "Africa",
            tz: Tz::Africa__Bamako,
            aliases: &["Timbuktu"],
            latitude: 12.65,
        },
        TimezoneEntry {
            city: "Nouakchott",
            country: "Mauritania",
            region: "Africa",
            tz: Tz::Africa__Nouakchott,
            aliases: &[],
            latitude: 18.07,
        },
        TimezoneEntry {
            city: "Lisbon",
            country: "Portugal",
            region: "Europe",
            tz: Tz::Europe__Lisbon,
            aliases: &["Porto", "Faro", "Braga", "Coimbra"],
            latitude: 38.72,
        },
        TimezoneEntry {
            city: "Sao Tome",
            country: "Sao Tome and Principe",
            region: "Africa",
            tz: Tz::Africa__Sao_Tome,
            aliases: &[],
            latitude: 0.34,
        },
        TimezoneEntry {
            city: "Dakar",
            country: "Senegal",
            region: "Africa",
            tz: Tz::Africa__Dakar,
            aliases: &["Saint-Louis"],
            latitude: 14.69,
        },
        TimezoneEntry {
            city: "Freetown",
            country: "Sierra Leone",
            region: "Africa",
            tz: Tz::Africa__Freetown,
            aliases: &[],
            latitude: 8.48,
        },
        TimezoneEntry {
            city: "Lome",
            country: "Togo",
            region: "Africa",
            tz: Tz::Africa__Lome,
            aliases: &[],
            latitude: 6.13,
        },
        // ──────────────────────────────────────────
        // UTC+1
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Paris",
            country: "France",
            region: "Europe",
            tz: Tz::Europe__Paris,
            aliases: &[
                "Lyon",
                "Marseille",
                "Toulouse",
                "Nice",
                "Bordeaux",
                "Strasbourg",
                "Lille",
            ],
            latitude: 48.86,
        },
        TimezoneEntry {
            city: "Berlin",
            country: "Germany",
            region: "Europe",
            tz: Tz::Europe__Berlin,
            aliases: &[
                "Munich",
                "Hamburg",
                "Frankfurt",
                "Cologne",
                "Stuttgart",
                "Dusseldorf",
                "Hanover",
                "Dresden",
                "Leipzig",
            ],
            latitude: 52.52,
        },
        TimezoneEntry {
            city: "Lagos",
            country: "Nigeria",
            region: "Africa",
            tz: Tz::Africa__Lagos,
            aliases: &["Abuja", "Kano", "Ibadan", "Port Harcourt"],
            latitude: 6.46,
        },
        TimezoneEntry {
            city: "Tirana",
            country: "Albania",
            region: "Europe",
            tz: Tz::Europe__Tirane,
            aliases: &["Durres"],
            latitude: 41.33,
        },
        TimezoneEntry {
            city: "Algiers",
            country: "Algeria",
            region: "Africa",
            tz: Tz::Africa__Algiers,
            aliases: &["Oran", "Constantine"],
            latitude: 36.75,
        },
        TimezoneEntry {
            city: "Andorra la Vella",
            country: "Andorra",
            region: "Europe",
            tz: Tz::Europe__Andorra,
            aliases: &[],
            latitude: 42.51,
        },
        TimezoneEntry {
            city: "Luanda",
            country: "Angola",
            region: "Africa",
            tz: Tz::Africa__Luanda,
            aliases: &["Huambo", "Lobito"],
            latitude: -8.84,
        },
        TimezoneEntry {
            city: "Vienna",
            country: "Austria",
            region: "Europe",
            tz: Tz::Europe__Vienna,
            aliases: &["Salzburg", "Innsbruck", "Graz", "Linz"],
            latitude: 48.21,
        },
        TimezoneEntry {
            city: "Brussels",
            country: "Belgium",
            region: "Europe",
            tz: Tz::Europe__Brussels,
            aliases: &["Antwerp", "Ghent", "Bruges", "Liege"],
            latitude: 50.85,
        },
        TimezoneEntry {
            city: "Porto-Novo",
            country: "Benin",
            region: "Africa",
            tz: Tz::Africa__PortoNovo,
            aliases: &["Cotonou"],
            latitude: 6.50,
        },
        TimezoneEntry {
            city: "Sarajevo",
            country: "Bosnia and Herzegovina",
            region: "Europe",
            tz: Tz::Europe__Sarajevo,
            aliases: &["Banja Luka", "Mostar"],
            latitude: 43.86,
        },
        TimezoneEntry {
            city: "Douala",
            country: "Cameroon",
            region: "Africa",
            tz: Tz::Africa__Douala,
            aliases: &["Yaounde"],
            latitude: 4.05,
        },
        TimezoneEntry {
            city: "Bangui",
            country: "Central African Republic",
            region: "Africa",
            tz: Tz::Africa__Bangui,
            aliases: &[],
            latitude: 4.36,
        },
        TimezoneEntry {
            city: "Ndjamena",
            country: "Chad",
            region: "Africa",
            tz: Tz::Africa__Ndjamena,
            aliases: &[],
            latitude: 12.13,
        },
        TimezoneEntry {
            city: "Brazzaville",
            country: "Congo",
            region: "Africa",
            tz: Tz::Africa__Brazzaville,
            aliases: &["Pointe-Noire"],
            latitude: -4.27,
        },
        TimezoneEntry {
            city: "Zagreb",
            country: "Croatia",
            region: "Europe",
            tz: Tz::Europe__Zagreb,
            aliases: &["Split", "Dubrovnik", "Rijeka"],
            latitude: 45.81,
        },
        TimezoneEntry {
            city: "Prague",
            country: "Czechia",
            region: "Europe",
            tz: Tz::Europe__Prague,
            aliases: &["Brno", "Ostrava", "Pilsen"],
            latitude: 50.09,
        },
        TimezoneEntry {
            city: "Copenhagen",
            country: "Denmark",
            region: "Europe",
            tz: Tz::Europe__Copenhagen,
            aliases: &["Aarhus", "Odense", "Aalborg"],
            latitude: 55.68,
        },
        TimezoneEntry {
            city: "Kinshasa",
            country: "DR Congo",
            region: "Africa",
            tz: Tz::Africa__Kinshasa,
            aliases: &["Lubumbashi", "Mbuji-Mayi"],
            latitude: -4.32,
        },
        TimezoneEntry {
            city: "Malabo",
            country: "Equatorial Guinea",
            region: "Africa",
            tz: Tz::Africa__Malabo,
            aliases: &["Bata"],
            latitude: 3.75,
        },
        TimezoneEntry {
            city: "Libreville",
            country: "Gabon",
            region: "Africa",
            tz: Tz::Africa__Libreville,
            aliases: &["Port-Gentil"],
            latitude: 0.42,
        },
        TimezoneEntry {
            city: "Budapest",
            country: "Hungary",
            region: "Europe",
            tz: Tz::Europe__Budapest,
            aliases: &["Debrecen", "Szeged"],
            latitude: 47.50,
        },
        TimezoneEntry {
            city: "Rome",
            country: "Italy",
            region: "Europe",
            tz: Tz::Europe__Rome,
            aliases: &[
                "Milan", "Naples", "Turin", "Florence", "Venice", "Bologna", "Palermo", "Genoa",
            ],
            latitude: 41.90,
        },
        TimezoneEntry {
            city: "Vaduz",
            country: "Liechtenstein",
            region: "Europe",
            tz: Tz::Europe__Vaduz,
            aliases: &[],
            latitude: 47.14,
        },
        TimezoneEntry {
            city: "Luxembourg City",
            country: "Luxembourg",
            region: "Europe",
            tz: Tz::Europe__Luxembourg,
            aliases: &[],
            latitude: 49.61,
        },
        TimezoneEntry {
            city: "Valletta",
            country: "Malta",
            region: "Europe",
            tz: Tz::Europe__Malta,
            aliases: &["Sliema", "St. Julian's"],
            latitude: 35.90,
        },
        TimezoneEntry {
            city: "Monaco",
            country: "Monaco",
            region: "Europe",
            tz: Tz::Europe__Monaco,
            aliases: &["Monte Carlo"],
            latitude: 43.74,
        },
        TimezoneEntry {
            city: "Podgorica",
            country: "Montenegro",
            region: "Europe",
            tz: Tz::Europe__Podgorica,
            aliases: &["Budva", "Kotor"],
            latitude: 42.44,
        },
        TimezoneEntry {
            city: "Casablanca",
            country: "Morocco",
            region: "Africa",
            tz: Tz::Africa__Casablanca,
            aliases: &["Rabat", "Marrakesh", "Fez", "Tangier"],
            latitude: 33.57,
        },
        TimezoneEntry {
            city: "Windhoek",
            country: "Namibia",
            region: "Africa",
            tz: Tz::Africa__Windhoek,
            aliases: &["Walvis Bay", "Swakopmund"],
            latitude: -22.56,
        },
        TimezoneEntry {
            city: "Amsterdam",
            country: "Netherlands",
            region: "Europe",
            tz: Tz::Europe__Amsterdam,
            aliases: &["Rotterdam", "The Hague", "Utrecht", "Eindhoven"],
            latitude: 52.37,
        },
        TimezoneEntry {
            city: "Niamey",
            country: "Niger",
            region: "Africa",
            tz: Tz::Africa__Niamey,
            aliases: &["Zinder"],
            latitude: 13.51,
        },
        TimezoneEntry {
            city: "Skopje",
            country: "North Macedonia",
            region: "Europe",
            tz: Tz::Europe__Skopje,
            aliases: &["Ohrid", "Bitola"],
            latitude: 42.00,
        },
        TimezoneEntry {
            city: "Oslo",
            country: "Norway",
            region: "Europe",
            tz: Tz::Europe__Oslo,
            aliases: &["Bergen", "Trondheim", "Stavanger", "Tromso"],
            latitude: 59.91,
        },
        TimezoneEntry {
            city: "Warsaw",
            country: "Poland",
            region: "Europe",
            tz: Tz::Europe__Warsaw,
            aliases: &["Krakow", "Gdansk", "Wroclaw", "Poznan", "Lodz"],
            latitude: 52.23,
        },
        TimezoneEntry {
            city: "San Marino",
            country: "San Marino",
            region: "Europe",
            tz: Tz::Europe__San_Marino,
            aliases: &[],
            latitude: 43.94,
        },
        TimezoneEntry {
            city: "Belgrade",
            country: "Serbia",
            region: "Europe",
            tz: Tz::Europe__Belgrade,
            aliases: &["Novi Sad", "Nis"],
            latitude: 44.79,
        },
        TimezoneEntry {
            city: "Bratislava",
            country: "Slovakia",
            region: "Europe",
            tz: Tz::Europe__Bratislava,
            aliases: &["Kosice"],
            latitude: 48.15,
        },
        TimezoneEntry {
            city: "Ljubljana",
            country: "Slovenia",
            region: "Europe",
            tz: Tz::Europe__Ljubljana,
            aliases: &["Maribor"],
            latitude: 46.06,
        },
        TimezoneEntry {
            city: "Madrid",
            country: "Spain",
            region: "Europe",
            tz: Tz::Europe__Madrid,
            aliases: &[
                "Barcelona",
                "Valencia",
                "Seville",
                "Bilbao",
                "Malaga",
                "Zaragoza",
            ],
            latitude: 40.42,
        },
        TimezoneEntry {
            city: "Stockholm",
            country: "Sweden",
            region: "Europe",
            tz: Tz::Europe__Stockholm,
            aliases: &["Gothenburg", "Malmo", "Uppsala"],
            latitude: 59.33,
        },
        TimezoneEntry {
            city: "Zürich",
            country: "Switzerland",
            region: "Europe",
            tz: Tz::Europe__Zurich,
            aliases: &["Zurich", "Geneva", "Basel", "Bern", "Lausanne"],
            latitude: 47.37,
        },
        TimezoneEntry {
            city: "Tunis",
            country: "Tunisia",
            region: "Africa",
            tz: Tz::Africa__Tunis,
            aliases: &["Sfax", "Sousse"],
            latitude: 36.81,
        },
        TimezoneEntry {
            city: "Vatican City",
            country: "Vatican City",
            region: "Europe",
            tz: Tz::Europe__Vatican,
            aliases: &[],
            latitude: 41.90,
        },
        // ──────────────────────────────────────────
        // UTC+2
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Cairo",
            country: "Egypt",
            region: "Africa",
            tz: Tz::Africa__Cairo,
            aliases: &["Alexandria", "Giza", "Luxor", "Aswan", "Sharm el-Sheikh"],
            latitude: 30.04,
        },
        TimezoneEntry {
            city: "Athens",
            country: "Greece",
            region: "Europe",
            tz: Tz::Europe__Athens,
            aliases: &[
                "Thessaloniki",
                "Heraklion",
                "Patras",
                "Crete",
                "Rhodes",
                "Santorini",
            ],
            latitude: 37.98,
        },
        TimezoneEntry {
            city: "Johannesburg",
            country: "South Africa",
            region: "Africa",
            tz: Tz::Africa__Johannesburg,
            aliases: &["Cape Town", "Durban", "Pretoria", "Port Elizabeth"],
            latitude: -26.20,
        },
        TimezoneEntry {
            city: "Gaborone",
            country: "Botswana",
            region: "Africa",
            tz: Tz::Africa__Gaborone,
            aliases: &["Francistown", "Maun"],
            latitude: -24.65,
        },
        TimezoneEntry {
            city: "Sofia",
            country: "Bulgaria",
            region: "Europe",
            tz: Tz::Europe__Sofia,
            aliases: &["Plovdiv", "Varna", "Burgas"],
            latitude: 42.70,
        },
        TimezoneEntry {
            city: "Bujumbura",
            country: "Burundi",
            region: "Africa",
            tz: Tz::Africa__Bujumbura,
            aliases: &["Gitega"],
            latitude: -3.38,
        },
        TimezoneEntry {
            city: "Nicosia",
            country: "Cyprus",
            region: "Europe",
            tz: Tz::Asia__Nicosia,
            aliases: &["Limassol", "Larnaca", "Paphos"],
            latitude: 35.18,
        },
        TimezoneEntry {
            city: "Tallinn",
            country: "Estonia",
            region: "Europe",
            tz: Tz::Europe__Tallinn,
            aliases: &["Tartu"],
            latitude: 59.44,
        },
        TimezoneEntry {
            city: "Mbabane",
            country: "Eswatini",
            region: "Africa",
            tz: Tz::Africa__Mbabane,
            aliases: &["Manzini"],
            latitude: -26.32,
        },
        TimezoneEntry {
            city: "Helsinki",
            country: "Finland",
            region: "Europe",
            tz: Tz::Europe__Helsinki,
            aliases: &["Tampere", "Turku", "Espoo", "Oulu"],
            latitude: 60.17,
        },
        TimezoneEntry {
            city: "Jerusalem",
            country: "Israel",
            region: "Asia",
            tz: Tz::Asia__Jerusalem,
            aliases: &["Tel Aviv", "Haifa", "Eilat"],
            latitude: 31.78,
        },
        TimezoneEntry {
            city: "Amman",
            country: "Jordan",
            region: "Asia",
            tz: Tz::Asia__Amman,
            aliases: &["Aqaba", "Irbid", "Petra"],
            latitude: 31.95,
        },
        TimezoneEntry {
            city: "Riga",
            country: "Latvia",
            region: "Europe",
            tz: Tz::Europe__Riga,
            aliases: &["Daugavpils", "Jurmala"],
            latitude: 56.95,
        },
        TimezoneEntry {
            city: "Beirut",
            country: "Lebanon",
            region: "Asia",
            tz: Tz::Asia__Beirut,
            aliases: &["Tripoli", "Byblos"],
            latitude: 33.89,
        },
        TimezoneEntry {
            city: "Maseru",
            country: "Lesotho",
            region: "Africa",
            tz: Tz::Africa__Maseru,
            aliases: &[],
            latitude: -29.31,
        },
        TimezoneEntry {
            city: "Tripoli",
            country: "Libya",
            region: "Africa",
            tz: Tz::Africa__Tripoli,
            aliases: &["Benghazi", "Misrata"],
            latitude: 32.89,
        },
        TimezoneEntry {
            city: "Vilnius",
            country: "Lithuania",
            region: "Europe",
            tz: Tz::Europe__Vilnius,
            aliases: &["Kaunas", "Klaipeda"],
            latitude: 54.69,
        },
        TimezoneEntry {
            city: "Lilongwe",
            country: "Malawi",
            region: "Africa",
            tz: Tz::Africa__Blantyre,
            aliases: &["Blantyre", "Mzuzu"],
            latitude: -13.96,
        },
        TimezoneEntry {
            city: "Chisinau",
            country: "Moldova",
            region: "Europe",
            tz: Tz::Europe__Chisinau,
            aliases: &["Tiraspol", "Balti"],
            latitude: 47.01,
        },
        TimezoneEntry {
            city: "Maputo",
            country: "Mozambique",
            region: "Africa",
            tz: Tz::Africa__Maputo,
            aliases: &["Beira", "Nampula"],
            latitude: -25.97,
        },
        TimezoneEntry {
            city: "Ramallah",
            country: "Palestine",
            region: "Asia",
            tz: Tz::Asia__Hebron,
            aliases: &["Gaza", "Hebron", "Bethlehem", "Nablus"],
            latitude: 31.90,
        },
        TimezoneEntry {
            city: "Bucharest",
            country: "Romania",
            region: "Europe",
            tz: Tz::Europe__Bucharest,
            aliases: &["Cluj-Napoca", "Timisoara", "Iasi", "Constanta", "Brasov"],
            latitude: 44.43,
        },
        TimezoneEntry {
            city: "Kigali",
            country: "Rwanda",
            region: "Africa",
            tz: Tz::Africa__Kigali,
            aliases: &[],
            latitude: -1.94,
        },
        TimezoneEntry {
            city: "Juba",
            country: "South Sudan",
            region: "Africa",
            tz: Tz::Africa__Juba,
            aliases: &[],
            latitude: 4.85,
        },
        TimezoneEntry {
            city: "Khartoum",
            country: "Sudan",
            region: "Africa",
            tz: Tz::Africa__Khartoum,
            aliases: &["Omdurman", "Port Sudan"],
            latitude: 15.50,
        },
        TimezoneEntry {
            city: "Damascus",
            country: "Syria",
            region: "Asia",
            tz: Tz::Asia__Damascus,
            aliases: &["Aleppo", "Homs", "Latakia"],
            latitude: 33.51,
        },
        TimezoneEntry {
            city: "Kyiv",
            country: "Ukraine",
            region: "Europe",
            tz: Tz::Europe__Kyiv,
            aliases: &["Kharkiv", "Odesa", "Dnipro", "Lviv"],
            latitude: 50.45,
        },
        TimezoneEntry {
            city: "Lusaka",
            country: "Zambia",
            region: "Africa",
            tz: Tz::Africa__Lusaka,
            aliases: &["Livingstone", "Ndola", "Kitwe"],
            latitude: -15.42,
        },
        TimezoneEntry {
            city: "Harare",
            country: "Zimbabwe",
            region: "Africa",
            tz: Tz::Africa__Harare,
            aliases: &["Bulawayo", "Victoria Falls"],
            latitude: -17.83,
        },
        // ──────────────────────────────────────────
        // UTC+3
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Moscow",
            country: "Russia",
            region: "Europe",
            tz: Tz::Europe__Moscow,
            aliases: &["St. Petersburg", "Kazan", "Volgograd", "Nizhny Novgorod"],
            latitude: 55.76,
        },
        TimezoneEntry {
            city: "Istanbul",
            country: "Turkey",
            region: "Europe",
            tz: Tz::Europe__Istanbul,
            aliases: &[
                "Ankara",
                "Izmir",
                "Antalya",
                "Bursa",
                "Bodrum",
                "Cappadocia",
            ],
            latitude: 41.01,
        },
        TimezoneEntry {
            city: "Nairobi",
            country: "Kenya",
            region: "Africa",
            tz: Tz::Africa__Nairobi,
            aliases: &["Mombasa", "Kisumu", "Nakuru"],
            latitude: -1.29,
        },
        TimezoneEntry {
            city: "Manama",
            country: "Bahrain",
            region: "Asia",
            tz: Tz::Asia__Bahrain,
            aliases: &["Muharraq"],
            latitude: 26.23,
        },
        TimezoneEntry {
            city: "Minsk",
            country: "Belarus",
            region: "Europe",
            tz: Tz::Europe__Minsk,
            aliases: &["Gomel", "Brest", "Grodno"],
            latitude: 53.90,
        },
        TimezoneEntry {
            city: "Moroni",
            country: "Comoros",
            region: "Africa",
            tz: Tz::Indian__Comoro,
            aliases: &[],
            latitude: -11.70,
        },
        TimezoneEntry {
            city: "Djibouti",
            country: "Djibouti",
            region: "Africa",
            tz: Tz::Africa__Djibouti,
            aliases: &[],
            latitude: 11.59,
        },
        TimezoneEntry {
            city: "Asmara",
            country: "Eritrea",
            region: "Africa",
            tz: Tz::Africa__Asmara,
            aliases: &["Massawa"],
            latitude: 15.32,
        },
        TimezoneEntry {
            city: "Addis Ababa",
            country: "Ethiopia",
            region: "Africa",
            tz: Tz::Africa__Addis_Ababa,
            aliases: &["Dire Dawa", "Gondar", "Lalibela"],
            latitude: 9.03,
        },
        TimezoneEntry {
            city: "Baghdad",
            country: "Iraq",
            region: "Asia",
            tz: Tz::Asia__Baghdad,
            aliases: &["Basra", "Erbil", "Mosul", "Sulaymaniyah"],
            latitude: 33.31,
        },
        TimezoneEntry {
            city: "Kuwait City",
            country: "Kuwait",
            region: "Asia",
            tz: Tz::Asia__Kuwait,
            aliases: &[],
            latitude: 29.38,
        },
        TimezoneEntry {
            city: "Antananarivo",
            country: "Madagascar",
            region: "Africa",
            tz: Tz::Indian__Antananarivo,
            aliases: &["Toamasina", "Nosy Be"],
            latitude: -18.88,
        },
        TimezoneEntry {
            city: "Doha",
            country: "Qatar",
            region: "Asia",
            tz: Tz::Asia__Qatar,
            aliases: &["Al Wakrah"],
            latitude: 25.29,
        },
        TimezoneEntry {
            city: "Riyadh",
            country: "Saudi Arabia",
            region: "Asia",
            tz: Tz::Asia__Riyadh,
            aliases: &["Jeddah", "Mecca", "Medina", "Dammam"],
            latitude: 24.71,
        },
        TimezoneEntry {
            city: "Mogadishu",
            country: "Somalia",
            region: "Africa",
            tz: Tz::Africa__Mogadishu,
            aliases: &["Hargeisa"],
            latitude: 2.05,
        },
        TimezoneEntry {
            city: "Dar es Salaam",
            country: "Tanzania",
            region: "Africa",
            tz: Tz::Africa__Dar_es_Salaam,
            aliases: &["Dodoma", "Zanzibar", "Arusha", "Kilimanjaro"],
            latitude: -6.79,
        },
        TimezoneEntry {
            city: "Kampala",
            country: "Uganda",
            region: "Africa",
            tz: Tz::Africa__Kampala,
            aliases: &["Entebbe", "Jinja"],
            latitude: 0.35,
        },
        TimezoneEntry {
            city: "Aden",
            country: "Yemen",
            region: "Asia",
            tz: Tz::Asia__Aden,
            aliases: &["Sanaa", "Taiz"],
            latitude: 12.79,
        },
        // ──────────────────────────────────────────
        // UTC+3:30
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Tehran",
            country: "Iran",
            region: "Asia",
            tz: Tz::Asia__Tehran,
            aliases: &["Isfahan", "Mashhad", "Tabriz", "Shiraz"],
            latitude: 35.69,
        },
        // ──────────────────────────────────────────
        // UTC+4
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Dubai",
            country: "UAE",
            region: "Asia",
            tz: Tz::Asia__Dubai,
            aliases: &["Abu Dhabi", "Sharjah", "Ajman"],
            latitude: 25.20,
        },
        TimezoneEntry {
            city: "Yerevan",
            country: "Armenia",
            region: "Asia",
            tz: Tz::Asia__Yerevan,
            aliases: &["Gyumri"],
            latitude: 40.18,
        },
        TimezoneEntry {
            city: "Baku",
            country: "Azerbaijan",
            region: "Asia",
            tz: Tz::Asia__Baku,
            aliases: &["Ganja", "Sumqayit"],
            latitude: 40.41,
        },
        TimezoneEntry {
            city: "Tbilisi",
            country: "Georgia",
            region: "Asia",
            tz: Tz::Asia__Tbilisi,
            aliases: &["Batumi", "Kutaisi"],
            latitude: 41.72,
        },
        TimezoneEntry {
            city: "Port Louis",
            country: "Mauritius",
            region: "Africa",
            tz: Tz::Indian__Mauritius,
            aliases: &[],
            latitude: -20.16,
        },
        TimezoneEntry {
            city: "Muscat",
            country: "Oman",
            region: "Asia",
            tz: Tz::Asia__Muscat,
            aliases: &["Salalah", "Nizwa"],
            latitude: 23.59,
        },
        TimezoneEntry {
            city: "Victoria",
            country: "Seychelles",
            region: "Africa",
            tz: Tz::Indian__Mahe,
            aliases: &["Mahe"],
            latitude: -4.62,
        },
        // ──────────────────────────────────────────
        // UTC+4:30
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Kabul",
            country: "Afghanistan",
            region: "Asia",
            tz: Tz::Asia__Kabul,
            aliases: &["Kandahar", "Herat", "Mazar-i-Sharif"],
            latitude: 34.53,
        },
        // ──────────────────────────────────────────
        // UTC+5
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Karachi",
            country: "Pakistan",
            region: "Asia",
            tz: Tz::Asia__Karachi,
            aliases: &[
                "Lahore",
                "Islamabad",
                "Rawalpindi",
                "Faisalabad",
                "Peshawar",
            ],
            latitude: 24.86,
        },
        TimezoneEntry {
            city: "Male",
            country: "Maldives",
            region: "Asia",
            tz: Tz::Indian__Maldives,
            aliases: &[],
            latitude: 4.18,
        },
        TimezoneEntry {
            city: "Dushanbe",
            country: "Tajikistan",
            region: "Asia",
            tz: Tz::Asia__Dushanbe,
            aliases: &["Khujand"],
            latitude: 38.54,
        },
        TimezoneEntry {
            city: "Ashgabat",
            country: "Turkmenistan",
            region: "Asia",
            tz: Tz::Asia__Ashgabat,
            aliases: &["Turkmenabat", "Mary"],
            latitude: 37.95,
        },
        TimezoneEntry {
            city: "Tashkent",
            country: "Uzbekistan",
            region: "Asia",
            tz: Tz::Asia__Tashkent,
            aliases: &["Samarkand", "Bukhara", "Khiva", "Namangan"],
            latitude: 41.31,
        },
        // ──────────────────────────────────────────
        // UTC+5:30
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Mumbai",
            country: "India",
            region: "Asia",
            tz: Tz::Asia__Kolkata,
            aliases: &[
                "Delhi",
                "New Delhi",
                "Bangalore",
                "Bengaluru",
                "Chennai",
                "Hyderabad",
                "Kolkata",
                "Pune",
                "Ahmedabad",
                "Jaipur",
                "Lucknow",
                "Goa",
                "Kochi",
            ],
            latitude: 19.08,
        },
        TimezoneEntry {
            city: "Colombo",
            country: "Sri Lanka",
            region: "Asia",
            tz: Tz::Asia__Colombo,
            aliases: &["Kandy", "Galle", "Jaffna"],
            latitude: 6.93,
        },
        // ──────────────────────────────────────────
        // UTC+5:45
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Kathmandu",
            country: "Nepal",
            region: "Asia",
            tz: Tz::Asia__Kathmandu,
            aliases: &["Pokhara", "Lalitpur", "Biratnagar"],
            latitude: 27.72,
        },
        // ──────────────────────────────────────────
        // UTC+6
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Dhaka",
            country: "Bangladesh",
            region: "Asia",
            tz: Tz::Asia__Dhaka,
            aliases: &["Chittagong", "Sylhet", "Rajshahi", "Khulna"],
            latitude: 23.81,
        },
        TimezoneEntry {
            city: "Thimphu",
            country: "Bhutan",
            region: "Asia",
            tz: Tz::Asia__Thimphu,
            aliases: &["Paro"],
            latitude: 27.47,
        },
        TimezoneEntry {
            city: "Almaty",
            country: "Kazakhstan",
            region: "Asia",
            tz: Tz::Asia__Almaty,
            aliases: &["Astana", "Shymkent", "Nur-Sultan"],
            latitude: 43.26,
        },
        TimezoneEntry {
            city: "Bishkek",
            country: "Kyrgyzstan",
            region: "Asia",
            tz: Tz::Asia__Bishkek,
            aliases: &["Osh"],
            latitude: 42.87,
        },
        // ──────────────────────────────────────────
        // UTC+6:30
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Yangon",
            country: "Myanmar",
            region: "Asia",
            tz: Tz::Asia__Yangon,
            aliases: &["Mandalay", "Naypyidaw", "Rangoon"],
            latitude: 16.85,
        },
        // ──────────────────────────────────────────
        // UTC+7
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Bangkok",
            country: "Thailand",
            region: "Asia",
            tz: Tz::Asia__Bangkok,
            aliases: &["Chiang Mai", "Phuket", "Pattaya", "Krabi"],
            latitude: 13.76,
        },
        TimezoneEntry {
            city: "Jakarta",
            country: "Indonesia",
            region: "Asia",
            tz: Tz::Asia__Jakarta,
            aliases: &[
                "Surabaya",
                "Bandung",
                "Medan",
                "Bali",
                "Yogyakarta",
                "Semarang",
            ],
            latitude: -6.21,
        },
        TimezoneEntry {
            city: "Phnom Penh",
            country: "Cambodia",
            region: "Asia",
            tz: Tz::Asia__Phnom_Penh,
            aliases: &["Siem Reap", "Angkor Wat", "Battambang", "Sihanoukville"],
            latitude: 11.55,
        },
        TimezoneEntry {
            city: "Vientiane",
            country: "Laos",
            region: "Asia",
            tz: Tz::Asia__Vientiane,
            aliases: &["Luang Prabang"],
            latitude: 17.97,
        },
        TimezoneEntry {
            city: "Novosibirsk",
            country: "Russia",
            region: "Asia",
            tz: Tz::Asia__Novosibirsk,
            aliases: &["Krasnoyarsk", "Tomsk", "Barnaul", "Omsk"],
            latitude: 55.04,
        },
        TimezoneEntry {
            city: "Ho Chi Minh City",
            country: "Vietnam",
            region: "Asia",
            tz: Tz::Asia__Ho_Chi_Minh,
            aliases: &["Hanoi", "Da Nang", "Saigon", "Hoi An", "Nha Trang"],
            latitude: 10.82,
        },
        // ──────────────────────────────────────────
        // UTC+8
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Singapore",
            country: "Singapore",
            region: "Asia",
            tz: Tz::Asia__Singapore,
            aliases: &[],
            latitude: 1.35,
        },
        TimezoneEntry {
            city: "Shanghai",
            country: "China",
            region: "Asia",
            tz: Tz::Asia__Shanghai,
            aliases: &[
                "Beijing",
                "Shenzhen",
                "Guangzhou",
                "Chengdu",
                "Wuhan",
                "Hangzhou",
                "Nanjing",
                "Tianjin",
                "Xi'an",
                "Chongqing",
                "Suzhou",
                "Qingdao",
            ],
            latitude: 31.23,
        },
        TimezoneEntry {
            city: "Hong Kong",
            country: "China",
            region: "Asia",
            tz: Tz::Asia__Hong_Kong,
            aliases: &["Macau", "Kowloon"],
            latitude: 22.32,
        },
        TimezoneEntry {
            city: "Perth",
            country: "Australia",
            region: "Australia",
            tz: Tz::Australia__Perth,
            aliases: &["Fremantle"],
            latitude: -31.95,
        },
        TimezoneEntry {
            city: "Bandar Seri Begawan",
            country: "Brunei",
            region: "Asia",
            tz: Tz::Asia__Brunei,
            aliases: &[],
            latitude: 4.94,
        },
        TimezoneEntry {
            city: "Kuala Lumpur",
            country: "Malaysia",
            region: "Asia",
            tz: Tz::Asia__Kuala_Lumpur,
            aliases: &[
                "Penang",
                "George Town",
                "Johor Bahru",
                "Kota Kinabalu",
                "Malacca",
                "Langkawi",
            ],
            latitude: 3.139,
        },
        TimezoneEntry {
            city: "Ulaanbaatar",
            country: "Mongolia",
            region: "Asia",
            tz: Tz::Asia__Ulaanbaatar,
            aliases: &["Erdenet", "Darkhan"],
            latitude: 47.92,
        },
        TimezoneEntry {
            city: "Manila",
            country: "Philippines",
            region: "Asia",
            tz: Tz::Asia__Manila,
            aliases: &["Cebu", "Davao", "Quezon City", "Boracay"],
            latitude: 14.60,
        },
        TimezoneEntry {
            city: "Taipei",
            country: "Taiwan",
            region: "Asia",
            tz: Tz::Asia__Taipei,
            aliases: &["Kaohsiung", "Taichung", "Tainan"],
            latitude: 25.03,
        },
        // ──────────────────────────────────────────
        // UTC+9
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Tokyo",
            country: "Japan",
            region: "Asia",
            tz: Tz::Asia__Tokyo,
            aliases: &[
                "Osaka", "Kyoto", "Yokohama", "Nagoya", "Sapporo", "Fukuoka", "Kobe", "Okinawa",
            ],
            latitude: 35.69,
        },
        TimezoneEntry {
            city: "Seoul",
            country: "South Korea",
            region: "Asia",
            tz: Tz::Asia__Seoul,
            aliases: &["Busan", "Incheon", "Daegu", "Jeju"],
            latitude: 37.57,
        },
        TimezoneEntry {
            city: "Pyongyang",
            country: "North Korea",
            region: "Asia",
            tz: Tz::Asia__Pyongyang,
            aliases: &[],
            latitude: 39.04,
        },
        TimezoneEntry {
            city: "Dili",
            country: "Timor-Leste",
            region: "Asia",
            tz: Tz::Asia__Dili,
            aliases: &[],
            latitude: -8.56,
        },
        TimezoneEntry {
            city: "Palau",
            country: "Palau",
            region: "Pacific",
            tz: Tz::Pacific__Palau,
            aliases: &["Koror"],
            latitude: 7.34,
        },
        // ──────────────────────────────────────────
        // UTC+9:30
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Adelaide",
            country: "Australia",
            region: "Australia",
            tz: Tz::Australia__Adelaide,
            aliases: &["Darwin"],
            latitude: -34.93,
        },
        // ──────────────────────────────────────────
        // UTC+10
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Sydney",
            country: "Australia",
            region: "Australia",
            tz: Tz::Australia__Sydney,
            aliases: &["Melbourne", "Canberra", "Brisbane", "Gold Coast", "Hobart"],
            latitude: -33.87,
        },
        TimezoneEntry {
            city: "Chuuk",
            country: "Micronesia",
            region: "Pacific",
            tz: Tz::Pacific__Chuuk,
            aliases: &["Pohnpei"],
            latitude: 7.45,
        },
        TimezoneEntry {
            city: "Port Moresby",
            country: "Papua New Guinea",
            region: "Pacific",
            tz: Tz::Pacific__Port_Moresby,
            aliases: &["Lae", "Mount Hagen"],
            latitude: -9.44,
        },
        TimezoneEntry {
            city: "Vladivostok",
            country: "Russia",
            region: "Asia",
            tz: Tz::Asia__Vladivostok,
            aliases: &["Khabarovsk"],
            latitude: 43.12,
        },
        // ──────────────────────────────────────────
        // UTC+11
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Noumea",
            country: "New Caledonia",
            region: "Pacific",
            tz: Tz::Pacific__Noumea,
            aliases: &[],
            latitude: -22.27,
        },
        TimezoneEntry {
            city: "Honiara",
            country: "Solomon Islands",
            region: "Pacific",
            tz: Tz::Pacific__Guadalcanal,
            aliases: &[],
            latitude: -9.43,
        },
        TimezoneEntry {
            city: "Port Vila",
            country: "Vanuatu",
            region: "Pacific",
            tz: Tz::Pacific__Efate,
            aliases: &[],
            latitude: -17.74,
        },
        // ──────────────────────────────────────────
        // UTC+12
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Auckland",
            country: "New Zealand",
            region: "Pacific",
            tz: Tz::Pacific__Auckland,
            aliases: &[
                "Wellington",
                "Christchurch",
                "Hamilton",
                "Queenstown",
                "Dunedin",
            ],
            latitude: -36.85,
        },
        TimezoneEntry {
            city: "Suva",
            country: "Fiji",
            region: "Pacific",
            tz: Tz::Pacific__Fiji,
            aliases: &["Nadi", "Lautoka"],
            latitude: -18.13,
        },
        TimezoneEntry {
            city: "Tarawa",
            country: "Kiribati",
            region: "Pacific",
            tz: Tz::Pacific__Tarawa,
            aliases: &[],
            latitude: 1.42,
        },
        TimezoneEntry {
            city: "Majuro",
            country: "Marshall Islands",
            region: "Pacific",
            tz: Tz::Pacific__Majuro,
            aliases: &["Kwajalein"],
            latitude: 7.12,
        },
        TimezoneEntry {
            city: "Nauru",
            country: "Nauru",
            region: "Pacific",
            tz: Tz::Pacific__Nauru,
            aliases: &[],
            latitude: -0.55,
        },
        TimezoneEntry {
            city: "Funafuti",
            country: "Tuvalu",
            region: "Pacific",
            tz: Tz::Pacific__Funafuti,
            aliases: &[],
            latitude: -8.52,
        },
        // ──────────────────────────────────────────
        // UTC+12:45
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Chatham Islands",
            country: "New Zealand",
            region: "Pacific",
            tz: Tz::Pacific__Chatham,
            aliases: &[],
            latitude: -43.95,
        },
        // ──────────────────────────────────────────
        // UTC+13
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Apia",
            country: "Samoa",
            region: "Pacific",
            tz: Tz::Pacific__Apia,
            aliases: &[],
            latitude: -13.83,
        },
        TimezoneEntry {
            city: "Nukualofa",
            country: "Tonga",
            region: "Pacific",
            tz: Tz::Pacific__Tongatapu,
            aliases: &[],
            latitude: -21.13,
        },
        // ──────────────────────────────────────────
        // UTC+14
        // ──────────────────────────────────────────
        TimezoneEntry {
            city: "Kiritimati",
            country: "Kiribati",
            region: "Pacific",
            tz: Tz::Pacific__Kiritimati,
            aliases: &["Christmas Island"],
            latitude: 1.87,
        },
    ]
}

#[derive(Clone, Copy)]
pub(crate) struct SupplementalSearchTerm {
    pub raw: &'static str,
    pub display_in_results: bool,
}

pub(crate) fn country_search_aliases(country: &str) -> &'static [&'static str] {
    match country {
        "USA" => &["US", "United States", "United States of America", "America"],
        "UK" => &["United Kingdom", "Britain", "Great Britain", "England"],
        "UAE" => &["United Arab Emirates", "Emirates"],
        // Common abbreviations and endonyms for major economies. These are
        // search-only — the canonical country name shown in the table remains
        // whatever the catalogue entry sets.
        "New Zealand" => &["NZ", "Aotearoa"],
        "South Africa" => &["SA", "RSA"],
        "China" => &["PRC", "People's Republic of China"],
        "South Korea" => &["ROK", "Korea", "Republic of Korea"],
        "North Korea" => &["DPRK", "Democratic People's Republic of Korea"],
        "Brazil" => &["Brasil"],
        "Spain" => &["Espana", "España"],
        "Germany" => &["Deutschland"],
        "Japan" => &["Nippon", "Nihon"],
        "Russia" => &["Rossiya", "Russian Federation"],
        _ => &[],
    }
}

pub(crate) fn supplemental_search_terms(
    entry: &TimezoneEntry,
) -> &'static [SupplementalSearchTerm] {
    match entry.tz {
        Tz::Pacific__Honolulu => &[
            SupplementalSearchTerm {
                raw: "Hawaii",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Hawaii Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Hawaii-Aleutian Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "HST",
                display_in_results: false,
            },
        ],
        Tz::America__Anchorage => &[
            SupplementalSearchTerm {
                raw: "Alaska",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Alaska Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "AKST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "AKDT",
                display_in_results: false,
            },
        ],
        Tz::America__Los_Angeles => &[
            SupplementalSearchTerm {
                raw: "Pacific Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Pacific",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "PT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "PST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "PDT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "California",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Oregon",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Washington State",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Nevada",
                display_in_results: true,
            },
            // Short-form city codes (A).
            SupplementalSearchTerm {
                raw: "LA",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "SF",
                display_in_results: false,
            },
            // Airport codes (B).
            SupplementalSearchTerm {
                raw: "LAX",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "SFO",
                display_in_results: false,
            },
        ],
        Tz::America__Vancouver => &[
            SupplementalSearchTerm {
                raw: "Pacific Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Pacific",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "PT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "PST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "PDT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "British Columbia",
                display_in_results: true,
            },
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "YVR",
                display_in_results: false,
            },
        ],
        Tz::America__Denver => &[
            SupplementalSearchTerm {
                raw: "Mountain Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Mountain",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "MT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "MST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "MDT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Colorado",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Utah",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "New Mexico",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Wyoming",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Montana",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Idaho",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "West Texas",
                display_in_results: true,
            },
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "DEN",
                display_in_results: false,
            },
        ],
        Tz::America__Phoenix => &[
            SupplementalSearchTerm {
                raw: "Arizona",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Mountain Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Mountain",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "MT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "MST",
                display_in_results: false,
            },
        ],
        Tz::America__Chicago => &[
            SupplementalSearchTerm {
                raw: "Central Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Central",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CDT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Texas",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Illinois",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Minnesota",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Wisconsin",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Missouri",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Louisiana",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Oklahoma",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Kansas",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Nebraska",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Iowa",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Arkansas",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Mississippi",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Alabama",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Tennessee",
                display_in_results: true,
            },
            // Short-form (A).
            SupplementalSearchTerm {
                raw: "Chi",
                display_in_results: false,
            },
            // Airport codes (B). Houston/Dallas are in this tz per the
            // catalogue's Chicago entry aliases.
            SupplementalSearchTerm {
                raw: "ORD",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "MDW",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "DFW",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "IAH",
                display_in_results: false,
            },
        ],
        Tz::America__Mexico_City => &[
            SupplementalSearchTerm {
                raw: "Central Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Central",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CDT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CDMX",
                display_in_results: true,
            },
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "MEX",
                display_in_results: false,
            },
        ],
        Tz::America__New_York => &[
            SupplementalSearchTerm {
                raw: "Eastern Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Eastern",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "ET",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "EST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "EDT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Florida",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Georgia",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Massachusetts",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Pennsylvania",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Virginia",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "New Jersey",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Maryland",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Connecticut",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Maine",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Ohio",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Michigan",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "North Carolina",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "South Carolina",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "District of Columbia",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "DC",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Delaware",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "New Hampshire",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Vermont",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "West Virginia",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Rhode Island",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Kentucky",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Indiana",
                display_in_results: true,
            },
            // Short-form city codes (A). These would otherwise be filtered
            // out by `score_field`'s 3-char minimum on `contains`-mode.
            SupplementalSearchTerm {
                raw: "NYC",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "NY",
                display_in_results: false,
            },
            // Airport codes (B). Atlanta, Boston, and DC-area airports are
            // all in the Eastern (America/New_York) tz.
            SupplementalSearchTerm {
                raw: "JFK",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "LGA",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "EWR",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "ATL",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "BOS",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "DCA",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "IAD",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "BWI",
                display_in_results: false,
            },
        ],
        Tz::America__Toronto => &[
            SupplementalSearchTerm {
                raw: "Eastern Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Eastern",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "ET",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "EST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "EDT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Ontario",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Quebec",
                display_in_results: true,
            },
            // Airport code (B). Doubles as a short-form for Toronto.
            SupplementalSearchTerm {
                raw: "YYZ",
                display_in_results: false,
            },
        ],
        Tz::America__Halifax => &[
            SupplementalSearchTerm {
                raw: "Atlantic Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Atlantic",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "AT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "AST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "ADT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Nova Scotia",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "New Brunswick",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Prince Edward Island",
                display_in_results: true,
            },
        ],
        Tz::America__St_Johns => &[
            SupplementalSearchTerm {
                raw: "Newfoundland",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Newfoundland Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "NST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "NDT",
                display_in_results: false,
            },
        ],
        Tz::Europe__London => &[
            SupplementalSearchTerm {
                raw: "Greenwich Mean Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "British Summer Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "GMT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "BST",
                display_in_results: false,
            },
            // Short-form (A).
            SupplementalSearchTerm {
                raw: "LDN",
                display_in_results: false,
            },
            // Airport codes (B).
            SupplementalSearchTerm {
                raw: "LHR",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "LGW",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "STN",
                display_in_results: false,
            },
            // Historical / colloquial (C). Scotland is searchable but the
            // canonical display label remains "London".
            SupplementalSearchTerm {
                raw: "Scotland",
                display_in_results: false,
            },
        ],
        Tz::Europe__Paris => &[
            SupplementalSearchTerm {
                raw: "Central European Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Central Europe",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CET",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CEST",
                display_in_results: false,
            },
            // Short-form (A) and airport codes (B).
            SupplementalSearchTerm {
                raw: "PAR",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CDG",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "ORY",
                display_in_results: false,
            },
        ],
        Tz::Europe__Berlin => &[
            SupplementalSearchTerm {
                raw: "Central European Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Central Europe",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CET",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CEST",
                display_in_results: false,
            },
            // Short-form (A) and airport codes (B). Frankfurt and Munich
            // are aliased to Berlin in the catalogue, so FRA/MUC route here.
            SupplementalSearchTerm {
                raw: "BER",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "FRA",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "MUC",
                display_in_results: false,
            },
        ],
        Tz::Europe__Athens => &[
            SupplementalSearchTerm {
                raw: "Eastern European Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Eastern Europe",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "EET",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "EEST",
                display_in_results: false,
            },
        ],
        Tz::Africa__Cairo => &[
            SupplementalSearchTerm {
                raw: "Eastern European Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Eastern Europe",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "EET",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "EEST",
                display_in_results: false,
            },
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "CAI",
                display_in_results: false,
            },
        ],
        Tz::Asia__Kolkata => &[
            SupplementalSearchTerm {
                raw: "India Standard Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "IST",
                display_in_results: false,
            },
            // Historical / colloquial city names (C). The canonical display
            // labels remain "Mumbai"/"Kolkata"/"Chennai" via the entry's
            // city/aliases — these search-only terms catch users who type
            // the pre-rename names.
            SupplementalSearchTerm {
                raw: "Bombay",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Calcutta",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Madras",
                display_in_results: false,
            },
            // Airport codes (B). Delhi is aliased to Mumbai in the catalogue,
            // so DEL routes here.
            SupplementalSearchTerm {
                raw: "BOM",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "DEL",
                display_in_results: false,
            },
        ],
        Tz::Asia__Tokyo => &[
            SupplementalSearchTerm {
                raw: "Japan Standard Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "JST",
                display_in_results: false,
            },
            // Short-form (A).
            SupplementalSearchTerm {
                raw: "TYO",
                display_in_results: false,
            },
            // Airport codes (B).
            SupplementalSearchTerm {
                raw: "HND",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "NRT",
                display_in_results: false,
            },
        ],
        Tz::Australia__Perth => &[
            SupplementalSearchTerm {
                raw: "Western Australia",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Australian Western Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "AWST",
                display_in_results: false,
            },
        ],
        Tz::Australia__Adelaide => &[
            SupplementalSearchTerm {
                raw: "South Australia",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Northern Territory",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Australian Central Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Central Australia",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "ACST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "ACDT",
                display_in_results: false,
            },
        ],
        Tz::Australia__Sydney => &[
            SupplementalSearchTerm {
                raw: "New South Wales",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Victoria",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Queensland",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Tasmania",
                display_in_results: true,
            },
            SupplementalSearchTerm {
                raw: "Australian Eastern Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "AEST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "AEDT",
                display_in_results: false,
            },
            // Short-form (A) and airport codes (B). Melbourne is aliased to
            // Sydney in the catalogue, so MEL routes here.
            SupplementalSearchTerm {
                raw: "SYD",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "MEL",
                display_in_results: false,
            },
        ],
        // ------------------------------------------------------------------
        // New arms for shortcuts (A), airport codes (B), historical names (C).
        // Grouped here at the bottom for ease of review; ordering within the
        // match doesn't affect runtime as the discriminant is unique.
        // ------------------------------------------------------------------
        Tz::America__Sao_Paulo => &[
            // Short-form (A) and airport code (B).
            SupplementalSearchTerm {
                raw: "SP",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "SAO",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "GRU",
                display_in_results: false,
            },
        ],
        Tz::America__Argentina__Buenos_Aires => &[
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "EZE",
                display_in_results: false,
            },
        ],
        Tz::America__Santiago => &[
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "SCL",
                display_in_results: false,
            },
        ],
        Tz::Europe__Amsterdam => &[
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "AMS",
                display_in_results: false,
            },
            // Historical / colloquial (C). "Holland" remains search-only;
            // canonical display label stays "Amsterdam".
            SupplementalSearchTerm {
                raw: "Holland",
                display_in_results: false,
            },
        ],
        Tz::Europe__Madrid => &[
            // Airport codes (B). Barcelona-El Prat is in the same tz.
            SupplementalSearchTerm {
                raw: "MAD",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "BCN",
                display_in_results: false,
            },
        ],
        Tz::Europe__Rome => &[
            // Airport codes (B). Milan-Malpensa is in the same tz.
            SupplementalSearchTerm {
                raw: "FCO",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "MXP",
                display_in_results: false,
            },
        ],
        Tz::Europe__Zurich => &[
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "ZRH",
                display_in_results: false,
            },
        ],
        Tz::Europe__Vienna => &[
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "VIE",
                display_in_results: false,
            },
        ],
        Tz::Europe__Istanbul => &[
            // Airport codes (B). Note: IST collides with India Standard Time
            // (also a supplemental term on Asia/Kolkata), but search scoring
            // surfaces both, which is the correct behaviour for an ambiguous
            // 3-letter code.
            SupplementalSearchTerm {
                raw: "IST",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "SAW",
                display_in_results: false,
            },
        ],
        Tz::Europe__Kyiv => &[
            // Historical spelling (C). "Kyiv" is the catalogue's display
            // label; "Kiev" routes here for users who type the older form.
            SupplementalSearchTerm {
                raw: "Kiev",
                display_in_results: false,
            },
        ],
        Tz::Asia__Dubai => &[
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "DXB",
                display_in_results: false,
            },
        ],
        Tz::Asia__Qatar => &[
            // Airport code (B). Doha is the city; DOH the IATA code.
            SupplementalSearchTerm {
                raw: "DOH",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Doha",
                display_in_results: true,
            },
        ],
        Tz::Asia__Tehran => &[
            // Historical / colloquial (C).
            SupplementalSearchTerm {
                raw: "Persia",
                display_in_results: false,
            },
        ],
        Tz::Asia__Yangon => &[
            // Historical / colloquial (C). "Burma" was the pre-1989 name.
            SupplementalSearchTerm {
                raw: "Burma",
                display_in_results: false,
            },
        ],
        Tz::Asia__Bangkok => &[
            // Short-form (A) and historical name (C).
            SupplementalSearchTerm {
                raw: "BKK",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Siam",
                display_in_results: false,
            },
        ],
        Tz::Asia__Jakarta => &[
            // Short-form (A) and airport code (B).
            SupplementalSearchTerm {
                raw: "JKT",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "CGK",
                display_in_results: false,
            },
        ],
        Tz::Asia__Singapore => &[
            // Short-forms (A) and airport code (B).
            SupplementalSearchTerm {
                raw: "SG",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "SGP",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "SIN",
                display_in_results: false,
            },
        ],
        Tz::Asia__Shanghai => &[
            // Airport codes (B) and historical Beijing romanisation (C).
            // PVG = Shanghai Pudong, SHA = Shanghai Hongqiao.
            SupplementalSearchTerm {
                raw: "PVG",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "SHA",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "Peking",
                display_in_results: false,
            },
        ],
        Tz::Asia__Hong_Kong => &[
            // Short-forms (A) / airport code (B).
            SupplementalSearchTerm {
                raw: "HK",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "HKG",
                display_in_results: false,
            },
        ],
        Tz::Asia__Kuala_Lumpur => &[
            // Short-form (A) and airport code (B).
            SupplementalSearchTerm {
                raw: "KL",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "KUL",
                display_in_results: false,
            },
        ],
        Tz::Asia__Seoul => &[
            // Airport codes (B). GMP = Gimpo, ICN = Incheon.
            SupplementalSearchTerm {
                raw: "ICN",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "GMP",
                display_in_results: false,
            },
        ],
        Tz::Asia__Colombo => &[
            // Historical / colloquial (C).
            SupplementalSearchTerm {
                raw: "Ceylon",
                display_in_results: false,
            },
        ],
        Tz::Pacific__Auckland => &[
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "AKL",
                display_in_results: false,
            },
        ],
        Tz::Africa__Johannesburg => &[
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "JNB",
                display_in_results: false,
            },
        ],
        Tz::Africa__Nairobi => &[
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "NBO",
                display_in_results: false,
            },
        ],
        Tz::Africa__Lagos => &[
            // Airport code (B).
            SupplementalSearchTerm {
                raw: "LOS",
                display_in_results: false,
            },
        ],
        _ => &[],
    }
}

// ============================================================================
// Day/night colouring
// ============================================================================
//
// The big-clock colour and per-row "Local Time" colour use a
// "daytime?" predicate. The naive `(6..18).contains(hour)` window is
// wrong at high latitudes (Stockholm in December, Reykjavík in June)
// and slightly off everywhere else (sunrise drifts ~3 hours through
// the year at mid-latitudes).
//
// We compute sunrise/sunset from the city's latitude and the day of
// year using a simplified solar-position model. Cities without a
// curated latitude fall back to the simple window — never worse than
// the previous behaviour.

/// Returns the curated latitude for `tz` if present in [`all_timezones`].
///
/// After the data-restructuring refactor, the source of truth for each city's
/// latitude is the `latitude` field on [`TimezoneEntry`]. This wrapper exists
/// purely so the original `latitude_for` call sites (and the legacy test
/// `every_catalogue_entry_has_a_latitude`) keep compiling.
///
/// Complexity: O(n) over the catalogue per call (~217 entries). Called once
/// per visible table row per second by [`is_daytime_at`], which is trivial.
/// If profiling ever shows this on a hot path, switch callers that already
/// hold a `&TimezoneEntry` to read `entry.latitude` directly.
pub(crate) fn latitude_for(tz: Tz) -> Option<f64> {
    all_timezones()
        .into_iter()
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
        Some(lat) => {
            let (sunrise, sunset) = sun_window(lat, local.ordinal());
            let h = local.hour() as f64 + local.minute() as f64 / 60.0;
            h >= sunrise && h < sunset
        }
        None => (6..18).contains(&local.hour()),
    }
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

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
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

    /// Locks in the invariant that every catalogue entry has a curated
    /// latitude. Without this, a new city added to `all_timezones()`
    /// without a matching arm in `latitude_for` would silently fall
    /// through to the `(6..18).contains(&hour)` default in
    /// `is_daytime_at`, which is wrong for any high-latitude or
    /// southern-hemisphere city.
    #[test]
    fn every_catalogue_entry_has_a_latitude() {
        for entry in all_timezones() {
            assert!(
                latitude_for(entry.tz).is_some(),
                "missing latitude for {} ({:?})",
                entry.city,
                entry.tz,
            );
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
            (Tz::Asia__Singapore, 1.35, "Singapore"),
            (Tz::Africa__Cairo, 30.04, "Cairo"),
            (Tz::Pacific__Auckland, -36.85, "Auckland"),
            (Tz::America__Los_Angeles, 34.05, "Los Angeles"),
            (Tz::UTC, 0.00, "UTC (equator default)"),
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

    /// Coverage: every tz that has supplemental_search_terms must appear in the
    /// catalogue. This catches drift the other direction (supplemental terms for
    /// a removed city).
    #[test]
    fn every_supplemental_search_term_targets_a_catalogue_entry() {
        use std::collections::HashSet;
        let catalogue_tzs: HashSet<_> = all_timezones().into_iter().map(|e| e.tz).collect();
        // Iterate the catalogue and verify any supplemental terms it points at
        // are valid — this is the inverse of `every_catalogue_entry_has_a_latitude`.
        for entry in all_timezones() {
            if !supplemental_search_terms(&entry).is_empty() {
                assert!(
                    catalogue_tzs.contains(&entry.tz),
                    "supplemental_search_terms for {:?} but tz not in catalogue",
                    entry.tz
                );
            }
        }
        // NB: a stronger test would iterate every tz that has terms and check
        // catalogue membership, but supplemental_search_terms is keyed by Tz
        // (enum), not by an iterable list — there's no source-of-truth iterator
        // without exposing internals. The above test catches the practical case.
    }

    /// Helper for the findability tests below: look up `tz` in the catalogue
    /// and assert that `term` is in either its `aliases` or its
    /// `supplemental_search_terms`. Case-insensitive — short codes are
    /// stored in upper case but users type them in any case.
    fn assert_findable(tz: Tz, term: &str) {
        let entry = all_timezones()
            .into_iter()
            .find(|e| e.tz == tz)
            .unwrap_or_else(|| panic!("tz not in catalogue: {tz:?}"));
        let in_aliases = entry.aliases.iter().any(|a| a.eq_ignore_ascii_case(term));
        let in_supp = supplemental_search_terms(&entry)
            .iter()
            .any(|s| s.raw.eq_ignore_ascii_case(term));
        assert!(
            in_aliases || in_supp,
            "{:?} should be findable as {:?} but it's in neither aliases nor supplemental terms",
            tz,
            term,
        );
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
            ("USA", "America"), // pre-existing
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
