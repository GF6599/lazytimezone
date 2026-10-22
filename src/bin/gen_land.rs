//! Generates `data/land.tsv` from the Natural Earth land shapes.
//!
//! The input is `ne_50m_land.geojson`, which the `gen-land` recipe in
//! the justfile downloads. The output is the land the map view embeds at
//! compile time: one ring per line, as `longitude,latitude` points split
//! by spaces. A ring is the outline of land or of a hole in it, and the
//! map fills by the even-odd rule, so a hole needs no mark of its own.

use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        return Err("usage: gen_land <ne_50m_land.geojson> <output-tsv>".into());
    };
    let rings = rings(&std::fs::read_to_string(input)?)?;
    std::fs::write(&output, tsv(&rings))?;
    let points: usize = rings.iter().map(Vec::len).sum();
    println!("{} rings, {points} points to {output}", rings.len());
    Ok(())
}

/// One ring of (longitude, latitude) points.
type Ring = Vec<(f64, f64)>;

/// A coordinates value: GeoJSON nests arrays of numbers, and nothing else
/// appears under the key.
enum Value {
    Number(f64),
    Array(Vec<Value>),
}

/// Every ring under each `"coordinates"` key of `geojson`, in the order
/// of the file. Only the coordinates are read, so the file needs no
/// general JSON parser.
fn rings(geojson: &str) -> Result<Vec<Ring>, String> {
    const KEY: &str = "\"coordinates\"";
    let mut found = Vec::new();
    let mut rest = geojson;
    while let Some(at) = rest.find(KEY) {
        let value = rest[at + KEY.len()..]
            .trim_start()
            .strip_prefix(':')
            .ok_or("coordinates: the key has no value")?;
        let (value, tail) = parse(value)?;
        collect(&value, &mut found)?;
        rest = tail;
    }
    Ok(found)
}

/// Reads one value from the start of `text`, and gives the text after it.
fn parse(text: &str) -> Result<(Value, &str), String> {
    let text = text.trim_start();
    let Some(inner) = text.strip_prefix('[') else {
        let end = text
            .find(|c: char| !(c.is_ascii_digit() || matches!(c, '-' | '+' | '.' | 'e' | 'E')))
            .unwrap_or(text.len());
        let number = text[..end]
            .parse()
            .map_err(|_| format!("coordinates: no number at {:?}", start_of(text)))?;
        return Ok((Value::Number(number), &text[end..]));
    };
    let mut items = Vec::new();
    let mut rest = inner.trim_start();
    if let Some(after) = rest.strip_prefix(']') {
        return Ok((Value::Array(items), after));
    }
    loop {
        let (item, after) = parse(rest)?;
        items.push(item);
        let after = after.trim_start();
        if let Some(next) = after.strip_prefix(',') {
            rest = next;
        } else if let Some(end) = after.strip_prefix(']') {
            return Ok((Value::Array(items), end));
        } else {
            return Err(format!("coordinates: no , or ] at {:?}", start_of(after)));
        }
    }
}

/// The first characters of `text`, for an error message.
fn start_of(text: &str) -> String {
    text.chars().take(20).collect()
}

/// Adds every ring in `value` to `rings`: an array whose items are all
/// points is a ring, and any other array holds rings further down.
fn collect(value: &Value, rings: &mut Vec<Ring>) -> Result<(), String> {
    let Value::Array(items) = value else {
        return Err("coordinates: a number where an array belongs".into());
    };
    let point = |item: &Value| match item {
        Value::Array(numbers) => match numbers.as_slice() {
            [Value::Number(longitude), Value::Number(latitude), ..] => {
                Some((*longitude, *latitude))
            }
            _ => None,
        },
        Value::Number(_) => None,
    };
    let points: Option<Ring> = items.iter().map(point).collect();
    match points {
        Some(ring) if !ring.is_empty() => rings.push(ring),
        _ => {
            for item in items {
                collect(item, rings)?;
            }
        }
    }
    Ok(())
}

/// One ring per line, as `longitude,latitude` points to two decimals
/// split by spaces, without the point that repeats the first to close it.
fn tsv(rings: &[Ring]) -> String {
    let mut table = String::new();
    for ring in rings {
        let open = match (ring.first(), ring.last()) {
            (Some(first), Some(last)) if ring.len() > 1 && first == last => &ring[..ring.len() - 1],
            _ => &ring[..],
        };
        let points: Vec<String> = open
            .iter()
            .map(|(longitude, latitude)| format!("{longitude:.2},{latitude:.2}"))
            .collect();
        table.push_str(&points.join(" "));
        table.push('\n');
    }
    table
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// Every ring of a polygon and of a multipolygon is read once, holes
    /// included.
    ///
    /// Given: a polygon with an outline and a hole, and a multipolygon of
    /// two outlines.
    /// When: its rings are read.
    /// Then: there are four rings, in the order of the file, each with
    /// its points.
    ///
    /// Why it matters: Natural Earth stores islands as multipolygons and
    /// inland water as holes. A ring left out drops an island or fills a
    /// sea.
    #[test]
    fn every_ring_of_a_polygon_and_a_multipolygon_is_read() {
        // Given:
        let geojson = r#"{"type":"FeatureCollection","features":[
            {"type":"Feature","properties":{"scalerank":1},"geometry":{"type":"Polygon","coordinates":[
                [[0,0],[10,0],[10,10],[0,0]],
                [[2,2],[3,2],[3,3],[2,2]]]}},
            {"type":"Feature","properties":{},"geometry":{"type":"MultiPolygon","coordinates":[
                [[[20,0],[21,0],[21,1],[20,0]]],
                [[[-30.5,-1.25],[-29,-1],[-29,0],[-30.5,-1.25]]]]}}
        ]}"#;

        // When:
        let rings = rings(geojson).unwrap();

        // Then:
        assert_eq!(
            rings,
            vec![
                vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 0.0)],
                vec![(2.0, 2.0), (3.0, 2.0), (3.0, 3.0), (2.0, 2.0)],
                vec![(20.0, 0.0), (21.0, 0.0), (21.0, 1.0), (20.0, 0.0)],
                vec![(-30.5, -1.25), (-29.0, -1.0), (-29.0, 0.0), (-30.5, -1.25)],
            ]
        );
    }

    /// The table holds one ring per line, rounded to a hundredth of a
    /// degree, with no closing point.
    ///
    /// Given: a ring that closes on its first point, with long decimals.
    /// When: the table is written.
    /// Then: the line holds the points once each, to two decimals.
    ///
    /// Why it matters: the map fills a ring as closed, so the closing
    /// point only adds bytes to the binary, and a hundredth of a degree is
    /// far finer than a pixel of the map.
    #[test]
    fn the_table_holds_one_ring_per_line_rounded_with_no_closing_point() {
        // Given:
        let ring = vec![
            (0.123_456, -1.0),
            (10.0, 0.004_9),
            (5.555, 5.0),
            (0.123_456, -1.0),
        ];

        // When:
        let table = tsv(&[ring]);

        // Then:
        assert_eq!(table, "0.12,-1.00 10.00,0.00 5.55,5.00\n");
    }

    /// A coordinates array that is not numbers is an error that names it.
    ///
    /// Given: a polygon whose point holds a string.
    /// When: its rings are read.
    /// Then: the read fails, and the message names the coordinates.
    ///
    /// Why it matters: a changed file format must stop the generator,
    /// not write a map with land missing.
    #[test]
    fn a_coordinates_array_that_is_not_numbers_is_an_error() {
        // Given:
        let geojson = r#"{"geometry":{"type":"Polygon","coordinates":[[[0,"x"],[1,1]]]}}"#;

        // When:
        let error = rings(geojson).unwrap_err();

        // Then:
        assert!(error.contains("coordinates"), "{error}");
    }
}
