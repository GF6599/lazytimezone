//! Generates `data/cities.tsv` from the GeoNames dumps.
//!
//! The input directory must hold `cities15000.txt`,
//! `admin1CodesASCII.txt` and `countryInfo.txt`. The `gen-cities`
//! recipe in the justfile downloads them and runs this binary.
//!
//! The output is the city catalogue that the app embeds at compile
//! time: one tab-separated row per city, sorted by population, so the
//! loader inherits a relevance order without storing scores.

use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use chrono_tz::Tz;

/// One output row. The field order matches the columns in the TSV.
#[derive(Debug)]
struct CityRow {
    name: String,
    /// ASCII transliteration, empty when it equals `name`.
    ascii: String,
    /// State, province, or prefecture name. Empty when GeoNames has
    /// no first-level division for the row.
    admin1: String,
    country: String,
    cc: String,
    latitude: f64,
    population: u64,
    tz: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(input_dir), Some(output)) = (args.next(), args.next()) else {
        return Err("usage: gen_cities <geonames-dir> <output-tsv>".into());
    };
    let dir = PathBuf::from(input_dir);
    let admin1 = parse_admin1(&std::fs::read_to_string(dir.join("admin1CodesASCII.txt"))?);
    let countries = parse_countries(&std::fs::read_to_string(dir.join("countryInfo.txt"))?);
    let cities_txt = std::fs::read_to_string(dir.join("cities15000.txt"))?;

    let mut rows = Vec::new();
    for (number, line) in cities_txt.lines().enumerate() {
        let row = parse_city_line(line, &admin1, &countries)
            .map_err(|e| format!("cities15000.txt line {}: {e}", number + 1))?;
        rows.push(row);
    }
    sort_rows(&mut rows);
    std::fs::write(&output, render_tsv(&rows))?;
    eprintln!("wrote {} cities to {output}", rows.len());
    Ok(())
}

/// `admin1CodesASCII.txt`: `US.MA<TAB>Massachusetts<TAB>...`. Returns
/// the `CC.CODE -> name` map.
fn parse_admin1(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let mut cols = line.split('\t');
            let key = cols.next()?;
            let name = cols.next()?;
            (!key.is_empty() && !name.is_empty())
                .then(|| (key.to_string(), name.to_string()))
        })
        .collect()
}

/// `countryInfo.txt`: `#`-prefixed comments, then one country per
/// line with the ISO code in column 0 and the name in column 4.
fn parse_countries(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            let cc = *cols.first()?;
            let name = *cols.get(4)?;
            (!cc.is_empty() && !name.is_empty()).then(|| (cc.to_string(), name.to_string()))
        })
        .collect()
}

/// Parses one `cities15000.txt` row. Columns used: 1 name, 2 ascii
/// name, 4 latitude, 8 country code, 10 admin1 code, 14 population,
/// 17 IANA timezone.
///
/// An unknown timezone or country is an error, not a fallback: both
/// mean the dump and this parser disagree, and a silent skip would
/// ship a catalogue with holes nobody notices.
fn parse_city_line(
    line: &str,
    admin1: &HashMap<String, String>,
    countries: &HashMap<String, String>,
) -> Result<CityRow, String> {
    let cols: Vec<&str> = line.trim_end_matches('\r').split('\t').collect();
    if cols.len() < 19 {
        return Err(format!("expected 19 columns, got {}", cols.len()));
    }
    let name = cols[1].trim();
    if name.is_empty() {
        return Err("empty city name".to_string());
    }
    let ascii = cols[2].trim();
    let latitude: f64 = cols[4]
        .parse()
        .map_err(|e| format!("bad latitude {:?}: {e}", cols[4]))?;
    if !(-90.0..=90.0).contains(&latitude) {
        return Err(format!("latitude {latitude} out of range"));
    }
    let cc = cols[8].trim();
    let country = countries
        .get(cc)
        .ok_or_else(|| format!("unknown country code {cc:?}"))?;
    let admin1_code = cols[10].trim();
    // "00" is the GeoNames placeholder for "no first-level division".
    let admin1_name = if admin1_code.is_empty() || admin1_code == "00" {
        ""
    } else {
        admin1
            .get(&format!("{cc}.{admin1_code}"))
            .map(String::as_str)
            .unwrap_or("")
    };
    let population: u64 = cols[14]
        .parse()
        .map_err(|e| format!("bad population {:?}: {e}", cols[14]))?;
    let tz = cols[17].trim();
    tz.parse::<Tz>()
        .map_err(|_| format!("unknown IANA timezone {tz:?}"))?;
    Ok(CityRow {
        name: name.to_string(),
        ascii: if ascii == name || ascii.is_empty() {
            String::new()
        } else {
            ascii.to_string()
        },
        admin1: admin1_name.to_string(),
        country: country.clone(),
        cc: cc.to_string(),
        latitude,
        population,
        tz: tz.to_string(),
    })
}

/// Population descending, then name, then zone, then country code, so
/// the output is deterministic and the loader reads a relevance order.
fn sort_rows(rows: &mut [CityRow]) {
    rows.sort_by(|a, b| {
        b.population
            .cmp(&a.population)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.tz.cmp(&b.tz))
            .then_with(|| a.cc.cmp(&b.cc))
            .then_with(|| a.admin1.cmp(&b.admin1))
    });
}

fn render_tsv(rows: &[CityRow]) -> String {
    let mut out = String::from(
        "# City catalogue derived from GeoNames (https://www.geonames.org/), CC BY 4.0.\n\
         # Regenerate with: just gen-cities\n\
         # name\tascii\tadmin1\tcountry\tcc\tlatitude\tpopulation\ttz\n",
    );
    for row in rows {
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{:.2}\t{}\t{}\n",
            row.name,
            row.ascii,
            row.admin1,
            row.country,
            row.cc,
            row.latitude,
            row.population,
            row.tz
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    // Tests panic on failure by design — see src/app.rs for the
    // rationale on why the panic lints are relaxed in test modules.
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    fn boston_line() -> String {
        let mut cols = vec![""; 19];
        cols[0] = "4930956";
        cols[1] = "Boston";
        cols[2] = "Boston";
        cols[4] = "42.35843";
        cols[5] = "-71.05977";
        cols[8] = "US";
        cols[10] = "MA";
        cols[14] = "667137";
        cols[17] = "America/New_York";
        cols.join("\t")
    }

    fn lookups() -> (HashMap<String, String>, HashMap<String, String>) {
        let admin1 = parse_admin1("US.MA\tMassachusetts\tMassachusetts\t6254926");
        let countries = parse_countries(
            "# comment line\nUS\tUSA\t840\tUS\tUnited States\tWashington\t9629091\t310232863\tNA",
        );
        (admin1, countries)
    }

    #[test]
    fn a_city_row_resolves_state_and_country_names() {
        let (admin1, countries) = lookups();

        let row = parse_city_line(&boston_line(), &admin1, &countries).unwrap();

        assert_eq!(row.name, "Boston");
        assert_eq!(row.admin1, "Massachusetts");
        assert_eq!(row.country, "United States");
        assert_eq!(row.cc, "US");
        assert_eq!(row.population, 667_137);
        assert_eq!(row.tz, "America/New_York");
    }

    #[test]
    fn the_ascii_column_is_empty_when_it_repeats_the_name() {
        let (admin1, countries) = lookups();

        let row = parse_city_line(&boston_line(), &admin1, &countries).unwrap();

        assert_eq!(row.ascii, "");
    }

    #[test]
    fn a_transliterated_name_is_kept() {
        let (admin1, countries) = lookups();
        let line = boston_line().replace("Boston\tBoston", "S\u{e3}o Paulo\tSao Paulo");

        let row = parse_city_line(&line, &admin1, &countries).unwrap();

        assert_eq!(row.name, "S\u{e3}o Paulo");
        assert_eq!(row.ascii, "Sao Paulo");
    }

    #[test]
    fn an_unknown_timezone_is_an_error_not_a_skip() {
        let (admin1, countries) = lookups();
        let line = boston_line().replace("America/New_York", "America/Nowhere");

        let err = parse_city_line(&line, &admin1, &countries).unwrap_err();

        assert!(err.contains("America/Nowhere"), "got: {err}");
    }

    #[test]
    fn an_unknown_country_code_is_an_error() {
        let (admin1, _) = lookups();

        let err = parse_city_line(&boston_line(), &admin1, &HashMap::new()).unwrap_err();

        assert!(err.contains("US"), "got: {err}");
    }

    #[test]
    fn a_missing_admin1_mapping_degrades_to_empty() {
        let (_, countries) = lookups();

        let row = parse_city_line(&boston_line(), &HashMap::new(), &countries).unwrap();

        assert_eq!(row.admin1, "");
    }

    #[test]
    fn rows_sort_by_population_descending() {
        let (admin1, countries) = lookups();
        let small = parse_city_line(&boston_line(), &admin1, &countries).unwrap();
        let big_line = boston_line()
            .replace("667137", "8000000")
            .replace("Boston\tBoston", "New York\tNew York");
        let big = parse_city_line(&big_line, &admin1, &countries).unwrap();
        let mut rows = vec![small, big];

        sort_rows(&mut rows);

        assert_eq!(rows[0].name, "New York");
    }

    #[test]
    fn the_tsv_has_commented_header_lines_and_one_row_per_city() {
        let (admin1, countries) = lookups();
        let rows = vec![parse_city_line(&boston_line(), &admin1, &countries).unwrap()];

        let tsv = render_tsv(&rows);

        let (headers, data): (Vec<&str>, Vec<&str>) =
            tsv.lines().partition(|l| l.starts_with('#'));
        assert_eq!(data.len(), 1);
        assert!(headers.iter().any(|h| h.contains("CC BY 4.0")));
        assert_eq!(
            data[0],
            "Boston\t\tMassachusetts\tUnited States\tUS\t42.36\t667137\tAmerica/New_York"
        );
    }
}
