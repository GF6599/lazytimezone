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
                "Seattle",
                "Portland",
                "Las Vegas",
                "Sacramento",
                "Tijuana",
            ],
        },
        TimezoneEntry {
            city: "Vancouver",
            country: "Canada",
            region: "North America",
            tz: Tz::America__Vancouver,
            aliases: &["Victoria", "Whistler"],
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
        },
        TimezoneEntry {
            city: "Phoenix",
            country: "USA",
            region: "North America",
            tz: Tz::America__Phoenix,
            aliases: &["Tucson", "Scottsdale", "Mesa"],
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
        },
        TimezoneEntry {
            city: "Mexico City",
            country: "Mexico",
            region: "North America",
            tz: Tz::America__Mexico_City,
            aliases: &["Guadalajara", "Monterrey", "Puebla", "Toluca"],
        },
        TimezoneEntry {
            city: "Belmopan",
            country: "Belize",
            region: "North America",
            tz: Tz::America__Belize,
            aliases: &["Belize City"],
        },
        TimezoneEntry {
            city: "San Jose",
            country: "Costa Rica",
            region: "North America",
            tz: Tz::America__Costa_Rica,
            aliases: &["Limon", "Tamarindo"],
        },
        TimezoneEntry {
            city: "San Salvador",
            country: "El Salvador",
            region: "North America",
            tz: Tz::America__El_Salvador,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Guatemala City",
            country: "Guatemala",
            region: "North America",
            tz: Tz::America__Guatemala,
            aliases: &["Antigua"],
        },
        TimezoneEntry {
            city: "Tegucigalpa",
            country: "Honduras",
            region: "North America",
            tz: Tz::America__Tegucigalpa,
            aliases: &["San Pedro Sula"],
        },
        TimezoneEntry {
            city: "Managua",
            country: "Nicaragua",
            region: "North America",
            tz: Tz::America__Managua,
            aliases: &["Leon", "Granada"],
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
        },
        TimezoneEntry {
            city: "Toronto",
            country: "Canada",
            region: "North America",
            tz: Tz::America__Toronto,
            aliases: &["Montreal", "Ottawa", "Quebec City"],
        },
        TimezoneEntry {
            city: "Bogota",
            country: "Colombia",
            region: "South America",
            tz: Tz::America__Bogota,
            aliases: &["Medellin", "Cali", "Cartagena", "Barranquilla"],
        },
        TimezoneEntry {
            city: "Nassau",
            country: "Bahamas",
            region: "North America",
            tz: Tz::America__Nassau,
            aliases: &["Freeport"],
        },
        TimezoneEntry {
            city: "Havana",
            country: "Cuba",
            region: "North America",
            tz: Tz::America__Havana,
            aliases: &["Santiago de Cuba", "Varadero"],
        },
        TimezoneEntry {
            city: "Quito",
            country: "Ecuador",
            region: "South America",
            tz: Tz::America__Guayaquil,
            aliases: &["Guayaquil", "Cuenca"],
        },
        TimezoneEntry {
            city: "Port-au-Prince",
            country: "Haiti",
            region: "North America",
            tz: Tz::America__PortauPrince,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Kingston",
            country: "Jamaica",
            region: "North America",
            tz: Tz::America__Jamaica,
            aliases: &["Montego Bay", "Ocho Rios"],
        },
        TimezoneEntry {
            city: "Panama City",
            country: "Panama",
            region: "North America",
            tz: Tz::America__Panama,
            aliases: &["Colon", "Bocas del Toro"],
        },
        TimezoneEntry {
            city: "Lima",
            country: "Peru",
            region: "South America",
            tz: Tz::America__Lima,
            aliases: &["Cusco", "Arequipa", "Trujillo"],
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
        },
        TimezoneEntry {
            city: "Halifax",
            country: "Canada",
            region: "North America",
            tz: Tz::America__Halifax,
            aliases: &["Fredericton", "Charlottetown", "Moncton"],
        },
        TimezoneEntry {
            city: "St. John's",
            country: "Antigua and Barbuda",
            region: "North America",
            tz: Tz::America__Antigua,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Bridgetown",
            country: "Barbados",
            region: "North America",
            tz: Tz::America__Barbados,
            aliases: &[],
        },
        TimezoneEntry {
            city: "La Paz",
            country: "Bolivia",
            region: "South America",
            tz: Tz::America__La_Paz,
            aliases: &["Santa Cruz", "Sucre", "Cochabamba"],
        },
        TimezoneEntry {
            city: "Manaus",
            country: "Brazil",
            region: "South America",
            tz: Tz::America__Manaus,
            aliases: &["Boa Vista"],
        },
        TimezoneEntry {
            city: "Roseau",
            country: "Dominica",
            region: "North America",
            tz: Tz::America__Dominica,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Santo Domingo",
            country: "Dominican Republic",
            region: "North America",
            tz: Tz::America__Santo_Domingo,
            aliases: &["Santiago", "Punta Cana"],
        },
        TimezoneEntry {
            city: "St. George's",
            country: "Grenada",
            region: "North America",
            tz: Tz::America__Grenada,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Georgetown",
            country: "Guyana",
            region: "South America",
            tz: Tz::America__Guyana,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Asuncion",
            country: "Paraguay",
            region: "South America",
            tz: Tz::America__Asuncion,
            aliases: &["Ciudad del Este"],
        },
        TimezoneEntry {
            city: "Castries",
            country: "Saint Lucia",
            region: "North America",
            tz: Tz::America__St_Lucia,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Basseterre",
            country: "St. Kitts and Nevis",
            region: "North America",
            tz: Tz::America__St_Kitts,
            aliases: &["Charlestown"],
        },
        TimezoneEntry {
            city: "Kingstown",
            country: "St. Vincent and the Grenadines",
            region: "North America",
            tz: Tz::America__St_Vincent,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Port of Spain",
            country: "Trinidad and Tobago",
            region: "North America",
            tz: Tz::America__Port_of_Spain,
            aliases: &["Scarborough"],
        },
        TimezoneEntry {
            city: "Caracas",
            country: "Venezuela",
            region: "South America",
            tz: Tz::America__Caracas,
            aliases: &["Maracaibo", "Valencia", "Barquisimeto"],
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
                "Brasilia",
                "Belo Horizonte",
                "Salvador",
                "Fortaleza",
                "Recife",
                "Porto Alegre",
            ],
        },
        TimezoneEntry {
            city: "Buenos Aires",
            country: "Argentina",
            region: "South America",
            tz: Tz::America__Argentina__Buenos_Aires,
            aliases: &["Cordoba", "Rosario", "Mendoza", "Mar del Plata"],
        },
        TimezoneEntry {
            city: "Paramaribo",
            country: "Suriname",
            region: "South America",
            tz: Tz::America__Paramaribo,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Montevideo",
            country: "Uruguay",
            region: "South America",
            tz: Tz::America__Montevideo,
            aliases: &["Punta del Este"],
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
        },
        TimezoneEntry {
            city: "Praia",
            country: "Cape Verde",
            region: "Atlantic",
            tz: Tz::Atlantic__Cape_Verde,
            aliases: &["Mindelo"],
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
        },
        TimezoneEntry {
            city: "Reykjavik",
            country: "Iceland",
            region: "Europe",
            tz: Tz::Atlantic__Reykjavik,
            aliases: &["Akureyri"],
        },
        TimezoneEntry {
            city: "Accra",
            country: "Ghana",
            region: "Africa",
            tz: Tz::Africa__Accra,
            aliases: &["Kumasi", "Tamale"],
        },
        TimezoneEntry {
            city: "Ouagadougou",
            country: "Burkina Faso",
            region: "Africa",
            tz: Tz::Africa__Ouagadougou,
            aliases: &["Bobo-Dioulasso"],
        },
        TimezoneEntry {
            city: "Banjul",
            country: "Gambia",
            region: "Africa",
            tz: Tz::Africa__Banjul,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Conakry",
            country: "Guinea",
            region: "Africa",
            tz: Tz::Africa__Conakry,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Bissau",
            country: "Guinea-Bissau",
            region: "Africa",
            tz: Tz::Africa__Bissau,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Dublin",
            country: "Ireland",
            region: "Europe",
            tz: Tz::Europe__Dublin,
            aliases: &["Cork", "Galway", "Limerick"],
        },
        TimezoneEntry {
            city: "Abidjan",
            country: "Ivory Coast",
            region: "Africa",
            tz: Tz::Africa__Abidjan,
            aliases: &["Yamoussoukro", "Bouake"],
        },
        TimezoneEntry {
            city: "Monrovia",
            country: "Liberia",
            region: "Africa",
            tz: Tz::Africa__Monrovia,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Bamako",
            country: "Mali",
            region: "Africa",
            tz: Tz::Africa__Bamako,
            aliases: &["Timbuktu"],
        },
        TimezoneEntry {
            city: "Nouakchott",
            country: "Mauritania",
            region: "Africa",
            tz: Tz::Africa__Nouakchott,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Lisbon",
            country: "Portugal",
            region: "Europe",
            tz: Tz::Europe__Lisbon,
            aliases: &["Porto", "Faro", "Braga", "Coimbra"],
        },
        TimezoneEntry {
            city: "Sao Tome",
            country: "Sao Tome and Principe",
            region: "Africa",
            tz: Tz::Africa__Sao_Tome,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Dakar",
            country: "Senegal",
            region: "Africa",
            tz: Tz::Africa__Dakar,
            aliases: &["Saint-Louis"],
        },
        TimezoneEntry {
            city: "Freetown",
            country: "Sierra Leone",
            region: "Africa",
            tz: Tz::Africa__Freetown,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Lome",
            country: "Togo",
            region: "Africa",
            tz: Tz::Africa__Lome,
            aliases: &[],
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
        },
        TimezoneEntry {
            city: "Lagos",
            country: "Nigeria",
            region: "Africa",
            tz: Tz::Africa__Lagos,
            aliases: &["Abuja", "Kano", "Ibadan", "Port Harcourt"],
        },
        TimezoneEntry {
            city: "Tirana",
            country: "Albania",
            region: "Europe",
            tz: Tz::Europe__Tirane,
            aliases: &["Durres"],
        },
        TimezoneEntry {
            city: "Algiers",
            country: "Algeria",
            region: "Africa",
            tz: Tz::Africa__Algiers,
            aliases: &["Oran", "Constantine"],
        },
        TimezoneEntry {
            city: "Andorra la Vella",
            country: "Andorra",
            region: "Europe",
            tz: Tz::Europe__Andorra,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Luanda",
            country: "Angola",
            region: "Africa",
            tz: Tz::Africa__Luanda,
            aliases: &["Huambo", "Lobito"],
        },
        TimezoneEntry {
            city: "Vienna",
            country: "Austria",
            region: "Europe",
            tz: Tz::Europe__Vienna,
            aliases: &["Salzburg", "Innsbruck", "Graz", "Linz"],
        },
        TimezoneEntry {
            city: "Brussels",
            country: "Belgium",
            region: "Europe",
            tz: Tz::Europe__Brussels,
            aliases: &["Antwerp", "Ghent", "Bruges", "Liege"],
        },
        TimezoneEntry {
            city: "Porto-Novo",
            country: "Benin",
            region: "Africa",
            tz: Tz::Africa__PortoNovo,
            aliases: &["Cotonou"],
        },
        TimezoneEntry {
            city: "Sarajevo",
            country: "Bosnia and Herzegovina",
            region: "Europe",
            tz: Tz::Europe__Sarajevo,
            aliases: &["Banja Luka", "Mostar"],
        },
        TimezoneEntry {
            city: "Douala",
            country: "Cameroon",
            region: "Africa",
            tz: Tz::Africa__Douala,
            aliases: &["Yaounde"],
        },
        TimezoneEntry {
            city: "Bangui",
            country: "Central African Republic",
            region: "Africa",
            tz: Tz::Africa__Bangui,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Ndjamena",
            country: "Chad",
            region: "Africa",
            tz: Tz::Africa__Ndjamena,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Brazzaville",
            country: "Congo",
            region: "Africa",
            tz: Tz::Africa__Brazzaville,
            aliases: &["Pointe-Noire"],
        },
        TimezoneEntry {
            city: "Zagreb",
            country: "Croatia",
            region: "Europe",
            tz: Tz::Europe__Zagreb,
            aliases: &["Split", "Dubrovnik", "Rijeka"],
        },
        TimezoneEntry {
            city: "Prague",
            country: "Czechia",
            region: "Europe",
            tz: Tz::Europe__Prague,
            aliases: &["Brno", "Ostrava", "Pilsen"],
        },
        TimezoneEntry {
            city: "Copenhagen",
            country: "Denmark",
            region: "Europe",
            tz: Tz::Europe__Copenhagen,
            aliases: &["Aarhus", "Odense", "Aalborg"],
        },
        TimezoneEntry {
            city: "Kinshasa",
            country: "DR Congo",
            region: "Africa",
            tz: Tz::Africa__Kinshasa,
            aliases: &["Lubumbashi", "Mbuji-Mayi"],
        },
        TimezoneEntry {
            city: "Malabo",
            country: "Equatorial Guinea",
            region: "Africa",
            tz: Tz::Africa__Malabo,
            aliases: &["Bata"],
        },
        TimezoneEntry {
            city: "Libreville",
            country: "Gabon",
            region: "Africa",
            tz: Tz::Africa__Libreville,
            aliases: &["Port-Gentil"],
        },
        TimezoneEntry {
            city: "Budapest",
            country: "Hungary",
            region: "Europe",
            tz: Tz::Europe__Budapest,
            aliases: &["Debrecen", "Szeged"],
        },
        TimezoneEntry {
            city: "Rome",
            country: "Italy",
            region: "Europe",
            tz: Tz::Europe__Rome,
            aliases: &[
                "Milan", "Naples", "Turin", "Florence", "Venice", "Bologna", "Palermo", "Genoa",
            ],
        },
        TimezoneEntry {
            city: "Vaduz",
            country: "Liechtenstein",
            region: "Europe",
            tz: Tz::Europe__Vaduz,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Luxembourg City",
            country: "Luxembourg",
            region: "Europe",
            tz: Tz::Europe__Luxembourg,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Valletta",
            country: "Malta",
            region: "Europe",
            tz: Tz::Europe__Malta,
            aliases: &["Sliema", "St. Julian's"],
        },
        TimezoneEntry {
            city: "Monaco",
            country: "Monaco",
            region: "Europe",
            tz: Tz::Europe__Monaco,
            aliases: &["Monte Carlo"],
        },
        TimezoneEntry {
            city: "Podgorica",
            country: "Montenegro",
            region: "Europe",
            tz: Tz::Europe__Podgorica,
            aliases: &["Budva", "Kotor"],
        },
        TimezoneEntry {
            city: "Casablanca",
            country: "Morocco",
            region: "Africa",
            tz: Tz::Africa__Casablanca,
            aliases: &["Rabat", "Marrakesh", "Fez", "Tangier"],
        },
        TimezoneEntry {
            city: "Windhoek",
            country: "Namibia",
            region: "Africa",
            tz: Tz::Africa__Windhoek,
            aliases: &["Walvis Bay", "Swakopmund"],
        },
        TimezoneEntry {
            city: "Amsterdam",
            country: "Netherlands",
            region: "Europe",
            tz: Tz::Europe__Amsterdam,
            aliases: &["Rotterdam", "The Hague", "Utrecht", "Eindhoven"],
        },
        TimezoneEntry {
            city: "Niamey",
            country: "Niger",
            region: "Africa",
            tz: Tz::Africa__Niamey,
            aliases: &["Zinder"],
        },
        TimezoneEntry {
            city: "Skopje",
            country: "North Macedonia",
            region: "Europe",
            tz: Tz::Europe__Skopje,
            aliases: &["Ohrid", "Bitola"],
        },
        TimezoneEntry {
            city: "Oslo",
            country: "Norway",
            region: "Europe",
            tz: Tz::Europe__Oslo,
            aliases: &["Bergen", "Trondheim", "Stavanger", "Tromso"],
        },
        TimezoneEntry {
            city: "Warsaw",
            country: "Poland",
            region: "Europe",
            tz: Tz::Europe__Warsaw,
            aliases: &["Krakow", "Gdansk", "Wroclaw", "Poznan", "Lodz"],
        },
        TimezoneEntry {
            city: "San Marino",
            country: "San Marino",
            region: "Europe",
            tz: Tz::Europe__San_Marino,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Belgrade",
            country: "Serbia",
            region: "Europe",
            tz: Tz::Europe__Belgrade,
            aliases: &["Novi Sad", "Nis"],
        },
        TimezoneEntry {
            city: "Bratislava",
            country: "Slovakia",
            region: "Europe",
            tz: Tz::Europe__Bratislava,
            aliases: &["Kosice"],
        },
        TimezoneEntry {
            city: "Ljubljana",
            country: "Slovenia",
            region: "Europe",
            tz: Tz::Europe__Ljubljana,
            aliases: &["Maribor"],
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
        },
        TimezoneEntry {
            city: "Stockholm",
            country: "Sweden",
            region: "Europe",
            tz: Tz::Europe__Stockholm,
            aliases: &["Gothenburg", "Malmo", "Uppsala"],
        },
        TimezoneEntry {
            city: "Zurich",
            country: "Switzerland",
            region: "Europe",
            tz: Tz::Europe__Zurich,
            aliases: &["Geneva", "Basel", "Bern", "Lausanne"],
        },
        TimezoneEntry {
            city: "Tunis",
            country: "Tunisia",
            region: "Africa",
            tz: Tz::Africa__Tunis,
            aliases: &["Sfax", "Sousse"],
        },
        TimezoneEntry {
            city: "Vatican City",
            country: "Vatican City",
            region: "Europe",
            tz: Tz::Europe__Vatican,
            aliases: &[],
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
        },
        TimezoneEntry {
            city: "Johannesburg",
            country: "South Africa",
            region: "Africa",
            tz: Tz::Africa__Johannesburg,
            aliases: &["Cape Town", "Durban", "Pretoria", "Port Elizabeth"],
        },
        TimezoneEntry {
            city: "Gaborone",
            country: "Botswana",
            region: "Africa",
            tz: Tz::Africa__Gaborone,
            aliases: &["Francistown", "Maun"],
        },
        TimezoneEntry {
            city: "Sofia",
            country: "Bulgaria",
            region: "Europe",
            tz: Tz::Europe__Sofia,
            aliases: &["Plovdiv", "Varna", "Burgas"],
        },
        TimezoneEntry {
            city: "Bujumbura",
            country: "Burundi",
            region: "Africa",
            tz: Tz::Africa__Bujumbura,
            aliases: &["Gitega"],
        },
        TimezoneEntry {
            city: "Nicosia",
            country: "Cyprus",
            region: "Europe",
            tz: Tz::Asia__Nicosia,
            aliases: &["Limassol", "Larnaca", "Paphos"],
        },
        TimezoneEntry {
            city: "Tallinn",
            country: "Estonia",
            region: "Europe",
            tz: Tz::Europe__Tallinn,
            aliases: &["Tartu"],
        },
        TimezoneEntry {
            city: "Mbabane",
            country: "Eswatini",
            region: "Africa",
            tz: Tz::Africa__Mbabane,
            aliases: &["Manzini"],
        },
        TimezoneEntry {
            city: "Helsinki",
            country: "Finland",
            region: "Europe",
            tz: Tz::Europe__Helsinki,
            aliases: &["Tampere", "Turku", "Espoo", "Oulu"],
        },
        TimezoneEntry {
            city: "Jerusalem",
            country: "Israel",
            region: "Asia",
            tz: Tz::Asia__Jerusalem,
            aliases: &["Tel Aviv", "Haifa", "Eilat"],
        },
        TimezoneEntry {
            city: "Amman",
            country: "Jordan",
            region: "Asia",
            tz: Tz::Asia__Amman,
            aliases: &["Aqaba", "Irbid", "Petra"],
        },
        TimezoneEntry {
            city: "Riga",
            country: "Latvia",
            region: "Europe",
            tz: Tz::Europe__Riga,
            aliases: &["Daugavpils", "Jurmala"],
        },
        TimezoneEntry {
            city: "Beirut",
            country: "Lebanon",
            region: "Asia",
            tz: Tz::Asia__Beirut,
            aliases: &["Tripoli", "Byblos"],
        },
        TimezoneEntry {
            city: "Maseru",
            country: "Lesotho",
            region: "Africa",
            tz: Tz::Africa__Maseru,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Tripoli",
            country: "Libya",
            region: "Africa",
            tz: Tz::Africa__Tripoli,
            aliases: &["Benghazi", "Misrata"],
        },
        TimezoneEntry {
            city: "Vilnius",
            country: "Lithuania",
            region: "Europe",
            tz: Tz::Europe__Vilnius,
            aliases: &["Kaunas", "Klaipeda"],
        },
        TimezoneEntry {
            city: "Lilongwe",
            country: "Malawi",
            region: "Africa",
            tz: Tz::Africa__Blantyre,
            aliases: &["Blantyre", "Mzuzu"],
        },
        TimezoneEntry {
            city: "Chisinau",
            country: "Moldova",
            region: "Europe",
            tz: Tz::Europe__Chisinau,
            aliases: &["Tiraspol", "Balti"],
        },
        TimezoneEntry {
            city: "Maputo",
            country: "Mozambique",
            region: "Africa",
            tz: Tz::Africa__Maputo,
            aliases: &["Beira", "Nampula"],
        },
        TimezoneEntry {
            city: "Ramallah",
            country: "Palestine",
            region: "Asia",
            tz: Tz::Asia__Hebron,
            aliases: &["Gaza", "Hebron", "Bethlehem", "Nablus"],
        },
        TimezoneEntry {
            city: "Bucharest",
            country: "Romania",
            region: "Europe",
            tz: Tz::Europe__Bucharest,
            aliases: &["Cluj-Napoca", "Timisoara", "Iasi", "Constanta", "Brasov"],
        },
        TimezoneEntry {
            city: "Kigali",
            country: "Rwanda",
            region: "Africa",
            tz: Tz::Africa__Kigali,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Juba",
            country: "South Sudan",
            region: "Africa",
            tz: Tz::Africa__Juba,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Khartoum",
            country: "Sudan",
            region: "Africa",
            tz: Tz::Africa__Khartoum,
            aliases: &["Omdurman", "Port Sudan"],
        },
        TimezoneEntry {
            city: "Damascus",
            country: "Syria",
            region: "Asia",
            tz: Tz::Asia__Damascus,
            aliases: &["Aleppo", "Homs", "Latakia"],
        },
        TimezoneEntry {
            city: "Kyiv",
            country: "Ukraine",
            region: "Europe",
            tz: Tz::Europe__Kyiv,
            aliases: &["Kharkiv", "Odesa", "Dnipro", "Lviv"],
        },
        TimezoneEntry {
            city: "Lusaka",
            country: "Zambia",
            region: "Africa",
            tz: Tz::Africa__Lusaka,
            aliases: &["Livingstone", "Ndola", "Kitwe"],
        },
        TimezoneEntry {
            city: "Harare",
            country: "Zimbabwe",
            region: "Africa",
            tz: Tz::Africa__Harare,
            aliases: &["Bulawayo", "Victoria Falls"],
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
        },
        TimezoneEntry {
            city: "Nairobi",
            country: "Kenya",
            region: "Africa",
            tz: Tz::Africa__Nairobi,
            aliases: &["Mombasa", "Kisumu", "Nakuru"],
        },
        TimezoneEntry {
            city: "Manama",
            country: "Bahrain",
            region: "Asia",
            tz: Tz::Asia__Bahrain,
            aliases: &["Muharraq"],
        },
        TimezoneEntry {
            city: "Minsk",
            country: "Belarus",
            region: "Europe",
            tz: Tz::Europe__Minsk,
            aliases: &["Gomel", "Brest", "Grodno"],
        },
        TimezoneEntry {
            city: "Moroni",
            country: "Comoros",
            region: "Africa",
            tz: Tz::Indian__Comoro,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Djibouti",
            country: "Djibouti",
            region: "Africa",
            tz: Tz::Africa__Djibouti,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Asmara",
            country: "Eritrea",
            region: "Africa",
            tz: Tz::Africa__Asmara,
            aliases: &["Massawa"],
        },
        TimezoneEntry {
            city: "Addis Ababa",
            country: "Ethiopia",
            region: "Africa",
            tz: Tz::Africa__Addis_Ababa,
            aliases: &["Dire Dawa", "Gondar", "Lalibela"],
        },
        TimezoneEntry {
            city: "Baghdad",
            country: "Iraq",
            region: "Asia",
            tz: Tz::Asia__Baghdad,
            aliases: &["Basra", "Erbil", "Mosul", "Sulaymaniyah"],
        },
        TimezoneEntry {
            city: "Kuwait City",
            country: "Kuwait",
            region: "Asia",
            tz: Tz::Asia__Kuwait,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Antananarivo",
            country: "Madagascar",
            region: "Africa",
            tz: Tz::Indian__Antananarivo,
            aliases: &["Toamasina", "Nosy Be"],
        },
        TimezoneEntry {
            city: "Doha",
            country: "Qatar",
            region: "Asia",
            tz: Tz::Asia__Qatar,
            aliases: &["Al Wakrah"],
        },
        TimezoneEntry {
            city: "Riyadh",
            country: "Saudi Arabia",
            region: "Asia",
            tz: Tz::Asia__Riyadh,
            aliases: &["Jeddah", "Mecca", "Medina", "Dammam"],
        },
        TimezoneEntry {
            city: "Mogadishu",
            country: "Somalia",
            region: "Africa",
            tz: Tz::Africa__Mogadishu,
            aliases: &["Hargeisa"],
        },
        TimezoneEntry {
            city: "Dar es Salaam",
            country: "Tanzania",
            region: "Africa",
            tz: Tz::Africa__Dar_es_Salaam,
            aliases: &["Dodoma", "Zanzibar", "Arusha", "Kilimanjaro"],
        },
        TimezoneEntry {
            city: "Kampala",
            country: "Uganda",
            region: "Africa",
            tz: Tz::Africa__Kampala,
            aliases: &["Entebbe", "Jinja"],
        },
        TimezoneEntry {
            city: "Aden",
            country: "Yemen",
            region: "Asia",
            tz: Tz::Asia__Aden,
            aliases: &["Sanaa", "Taiz"],
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
        },
        TimezoneEntry {
            city: "Yerevan",
            country: "Armenia",
            region: "Asia",
            tz: Tz::Asia__Yerevan,
            aliases: &["Gyumri"],
        },
        TimezoneEntry {
            city: "Baku",
            country: "Azerbaijan",
            region: "Asia",
            tz: Tz::Asia__Baku,
            aliases: &["Ganja", "Sumqayit"],
        },
        TimezoneEntry {
            city: "Tbilisi",
            country: "Georgia",
            region: "Asia",
            tz: Tz::Asia__Tbilisi,
            aliases: &["Batumi", "Kutaisi"],
        },
        TimezoneEntry {
            city: "Port Louis",
            country: "Mauritius",
            region: "Africa",
            tz: Tz::Indian__Mauritius,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Muscat",
            country: "Oman",
            region: "Asia",
            tz: Tz::Asia__Muscat,
            aliases: &["Salalah", "Nizwa"],
        },
        TimezoneEntry {
            city: "Victoria",
            country: "Seychelles",
            region: "Africa",
            tz: Tz::Indian__Mahe,
            aliases: &["Mahe"],
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
        },
        TimezoneEntry {
            city: "Male",
            country: "Maldives",
            region: "Asia",
            tz: Tz::Indian__Maldives,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Dushanbe",
            country: "Tajikistan",
            region: "Asia",
            tz: Tz::Asia__Dushanbe,
            aliases: &["Khujand"],
        },
        TimezoneEntry {
            city: "Ashgabat",
            country: "Turkmenistan",
            region: "Asia",
            tz: Tz::Asia__Ashgabat,
            aliases: &["Turkmenabat", "Mary"],
        },
        TimezoneEntry {
            city: "Tashkent",
            country: "Uzbekistan",
            region: "Asia",
            tz: Tz::Asia__Tashkent,
            aliases: &["Samarkand", "Bukhara", "Khiva", "Namangan"],
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
        },
        TimezoneEntry {
            city: "Colombo",
            country: "Sri Lanka",
            region: "Asia",
            tz: Tz::Asia__Colombo,
            aliases: &["Kandy", "Galle", "Jaffna"],
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
        },
        TimezoneEntry {
            city: "Thimphu",
            country: "Bhutan",
            region: "Asia",
            tz: Tz::Asia__Thimphu,
            aliases: &["Paro"],
        },
        TimezoneEntry {
            city: "Almaty",
            country: "Kazakhstan",
            region: "Asia",
            tz: Tz::Asia__Almaty,
            aliases: &["Astana", "Shymkent", "Nur-Sultan"],
        },
        TimezoneEntry {
            city: "Bishkek",
            country: "Kyrgyzstan",
            region: "Asia",
            tz: Tz::Asia__Bishkek,
            aliases: &["Osh"],
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
        },
        TimezoneEntry {
            city: "Phnom Penh",
            country: "Cambodia",
            region: "Asia",
            tz: Tz::Asia__Phnom_Penh,
            aliases: &["Siem Reap", "Angkor Wat", "Battambang", "Sihanoukville"],
        },
        TimezoneEntry {
            city: "Vientiane",
            country: "Laos",
            region: "Asia",
            tz: Tz::Asia__Vientiane,
            aliases: &["Luang Prabang"],
        },
        TimezoneEntry {
            city: "Novosibirsk",
            country: "Russia",
            region: "Asia",
            tz: Tz::Asia__Novosibirsk,
            aliases: &["Krasnoyarsk", "Tomsk", "Barnaul", "Omsk"],
        },
        TimezoneEntry {
            city: "Ho Chi Minh City",
            country: "Vietnam",
            region: "Asia",
            tz: Tz::Asia__Ho_Chi_Minh,
            aliases: &["Hanoi", "Da Nang", "Saigon", "Hoi An", "Nha Trang"],
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
        },
        TimezoneEntry {
            city: "Hong Kong",
            country: "China",
            region: "Asia",
            tz: Tz::Asia__Hong_Kong,
            aliases: &["Macau", "Kowloon"],
        },
        TimezoneEntry {
            city: "Perth",
            country: "Australia",
            region: "Australia",
            tz: Tz::Australia__Perth,
            aliases: &["Fremantle"],
        },
        TimezoneEntry {
            city: "Bandar Seri Begawan",
            country: "Brunei",
            region: "Asia",
            tz: Tz::Asia__Brunei,
            aliases: &[],
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
        },
        TimezoneEntry {
            city: "Ulaanbaatar",
            country: "Mongolia",
            region: "Asia",
            tz: Tz::Asia__Ulaanbaatar,
            aliases: &["Erdenet", "Darkhan"],
        },
        TimezoneEntry {
            city: "Manila",
            country: "Philippines",
            region: "Asia",
            tz: Tz::Asia__Manila,
            aliases: &["Cebu", "Davao", "Quezon City", "Boracay"],
        },
        TimezoneEntry {
            city: "Taipei",
            country: "Taiwan",
            region: "Asia",
            tz: Tz::Asia__Taipei,
            aliases: &["Kaohsiung", "Taichung", "Tainan"],
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
        },
        TimezoneEntry {
            city: "Seoul",
            country: "South Korea",
            region: "Asia",
            tz: Tz::Asia__Seoul,
            aliases: &["Busan", "Incheon", "Daegu", "Jeju"],
        },
        TimezoneEntry {
            city: "Pyongyang",
            country: "North Korea",
            region: "Asia",
            tz: Tz::Asia__Pyongyang,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Dili",
            country: "Timor-Leste",
            region: "Asia",
            tz: Tz::Asia__Dili,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Palau",
            country: "Palau",
            region: "Pacific",
            tz: Tz::Pacific__Palau,
            aliases: &["Koror"],
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
        },
        TimezoneEntry {
            city: "Chuuk",
            country: "Micronesia",
            region: "Pacific",
            tz: Tz::Pacific__Chuuk,
            aliases: &["Pohnpei"],
        },
        TimezoneEntry {
            city: "Port Moresby",
            country: "Papua New Guinea",
            region: "Pacific",
            tz: Tz::Pacific__Port_Moresby,
            aliases: &["Lae", "Mount Hagen"],
        },
        TimezoneEntry {
            city: "Vladivostok",
            country: "Russia",
            region: "Asia",
            tz: Tz::Asia__Vladivostok,
            aliases: &["Khabarovsk"],
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
        },
        TimezoneEntry {
            city: "Honiara",
            country: "Solomon Islands",
            region: "Pacific",
            tz: Tz::Pacific__Guadalcanal,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Port Vila",
            country: "Vanuatu",
            region: "Pacific",
            tz: Tz::Pacific__Efate,
            aliases: &[],
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
        },
        TimezoneEntry {
            city: "Suva",
            country: "Fiji",
            region: "Pacific",
            tz: Tz::Pacific__Fiji,
            aliases: &["Nadi", "Lautoka"],
        },
        TimezoneEntry {
            city: "Tarawa",
            country: "Kiribati",
            region: "Pacific",
            tz: Tz::Pacific__Tarawa,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Majuro",
            country: "Marshall Islands",
            region: "Pacific",
            tz: Tz::Pacific__Majuro,
            aliases: &["Kwajalein"],
        },
        TimezoneEntry {
            city: "Nauru",
            country: "Nauru",
            region: "Pacific",
            tz: Tz::Pacific__Nauru,
            aliases: &[],
        },
        TimezoneEntry {
            city: "Funafuti",
            country: "Tuvalu",
            region: "Pacific",
            tz: Tz::Pacific__Funafuti,
            aliases: &[],
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
        },
        TimezoneEntry {
            city: "Nukualofa",
            country: "Tonga",
            region: "Pacific",
            tz: Tz::Pacific__Tongatapu,
            aliases: &[],
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
        ],
        Tz::Europe__Paris | Tz::Europe__Berlin => &[
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
        ],
        Tz::Europe__Athens | Tz::Africa__Cairo => &[
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
        Tz::Asia__Kolkata => &[
            SupplementalSearchTerm {
                raw: "India Standard Time",
                display_in_results: false,
            },
            SupplementalSearchTerm {
                raw: "IST",
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
        ],
        _ => &[],
    }
}
