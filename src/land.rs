//! The land of the world map: the Natural Earth land shapes, which
//! `data/land.tsv` holds as one ring of `longitude,latitude` points per
//! line. The `gen-land` recipe in the justfile writes that file.

use std::sync::LazyLock;

const LAND_TSV: &str = include_str!("../data/land.tsv");

/// One ring of (longitude, latitude) points, closed back to its first.
type Ring = Vec<(f64, f64)>;

static RINGS: LazyLock<Vec<Ring>> = LazyLock::new(|| {
    LAND_TSV
        .lines()
        .map(|line| {
            line.split(' ')
                .filter_map(|point| {
                    let (longitude, latitude) = point.split_once(',')?;
                    Some((longitude.parse().ok()?, latitude.parse().ok()?))
                })
                .collect()
        })
        .collect()
});

/// The land rings, read from the embedded table once.
fn rings() -> &'static [Ring] {
    &RINGS
}

/// Whether each pixel of a `width` by `height` map is land, row by row.
/// `longitude_of` gives the longitude of a pixel column and must grow
/// from left to right, and `latitude_of` gives the latitude of a pixel
/// row.
///
/// Each row is filled by the even-odd rule: a pixel is land when the
/// ring edges that cross its row to its west number an odd count. So a
/// hole in a ring, such as the Caspian, stays open, and a row costs one
/// pass over the edges, which a frame can afford.
pub fn mask(
    width: usize,
    height: usize,
    longitude_of: impl Fn(usize) -> f64,
    latitude_of: impl Fn(usize) -> f64,
) -> Vec<bool> {
    let mut land = vec![false; width * height];
    let mut crossings = Vec::new();
    for y in 0..height {
        let latitude = latitude_of(y);
        crossings.clear();
        for ring in rings() {
            let edges = ring.iter().zip(ring.iter().cycle().skip(1));
            for (&(x1, y1), &(x2, y2)) in edges {
                if (y1 > latitude) != (y2 > latitude) {
                    crossings.push(x1 + (latitude - y1) / (y2 - y1) * (x2 - x1));
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        let mut west = 0;
        for x in 0..width {
            let longitude = longitude_of(x);
            while west < crossings.len() && crossings[west] < longitude {
                west += 1;
            }
            land[y * width + x] = west % 2 == 1;
        }
    }
    land
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// Every point of the embedded table reads as a place.
    ///
    /// Given: the land table the app embeds.
    /// When: its rings are read.
    /// Then: there is one ring per line, and each ring holds every point
    /// its line lists.
    ///
    /// Why it matters: a point that failed to read would cut a corner off
    /// a coast, and nothing on screen would say why.
    #[test]
    fn every_point_of_the_embedded_table_reads() {
        // Given:
        let lines: Vec<&str> = LAND_TSV.lines().collect();

        // When:
        let rings = rings();

        // Then:
        assert_eq!(rings.len(), lines.len());
        for (ring, line) in rings.iter().zip(&lines) {
            assert_eq!(ring.len(), line.split(' ').count(), "{line}");
        }
    }

    /// The mask marks land on the continents and leaves the seas open.
    ///
    /// Given: a mask one degree to a pixel over the whole world.
    /// When: places on land and in inland seas are looked up in it.
    /// Then: Chad and Brazil are land, and the Mediterranean, the Black
    /// Sea, the Caspian and Hudson Bay are not.
    ///
    /// Why it matters: a fill that leaked through a strait would paint a
    /// sea as land, and one that missed a ring would leave a hole in a
    /// continent.
    #[test]
    fn the_mask_marks_land_on_the_continents_and_leaves_the_seas_open() {
        // Given:
        let (width, height) = (360, 180);
        let longitude_of = |x: usize| -180.0 + x as f64 + 0.5;
        let latitude_of = |y: usize| 90.0 - y as f64 - 0.5;

        // When:
        let land = mask(width, height, longitude_of, latitude_of);

        // Then:
        let at = |longitude: f64, latitude: f64| {
            let x = (longitude + 180.0) as usize;
            let y = (90.0 - latitude) as usize;
            land[y * width + x]
        };
        for (name, longitude, latitude) in [("Chad", 19.0, 15.0), ("Brazil", -50.0, -10.0)] {
            assert!(at(longitude, latitude), "{name} is not land");
        }
        for (name, longitude, latitude) in [
            ("the Mediterranean", 18.0, 35.0),
            ("the Black Sea", 34.0, 43.5),
            ("the Caspian", 51.0, 42.0),
            ("Hudson Bay", -85.0, 60.0),
        ] {
            assert!(!at(longitude, latitude), "{name} is land");
        }
    }
}
