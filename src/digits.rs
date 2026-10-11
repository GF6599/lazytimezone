//! Clock digits drawn from strokes into octant cells.

/// The width of `text` in cells when drawn `rows` cells tall.
pub fn width(_text: &str, _rows: u16) -> u16 {
    0
}

/// Draws `text` as one string per cell row, `rows` rows tall.
pub fn render(_text: &str, _rows: u16) -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

    use super::*;

    /// A zero is a closed ring that is empty in the middle.
    ///
    /// Given: the digit zero at the largest hero size.
    /// When: it is drawn.
    /// Then: the middle row has ink at both sides and none at the centre.
    ///
    /// Why it matters: a slash or ink in the centre of each zero makes
    /// a clock full of zeros noisy, and a blocked counter makes 0, 6, 8
    /// and 9 hard to tell apart at a glance.
    #[test]
    fn the_zero_is_a_ring_with_no_slash() {
        // Given:
        let rows = 21;

        // When:
        let art = render("0", rows);

        // Then:
        let middle: Vec<char> = art[usize::from(rows / 2)].chars().collect();
        let centre = middle.len() / 2;
        assert_eq!(
            middle[centre],
            ' ',
            "ink at the centre:\n{}",
            art.join("\n")
        );
        assert_ne!(middle[0], ' ', "no left side:\n{}", art.join("\n"));
        assert_ne!(
            middle[middle.len() - 1],
            ' ',
            "no right side:\n{}",
            art.join("\n")
        );
    }

    /// Every time of day draws at the same width.
    ///
    /// Given: times whose digits differ in shape, from all ones to all
    /// eights.
    /// When: each is drawn at the same size.
    /// Then: every row of every drawing has the width `width` reports.
    ///
    /// Why it matters: the hero centres the art by its width, so a
    /// digit narrower than the others would shift the clock sideways
    /// every second.
    #[test]
    fn every_time_of_day_draws_at_the_same_width() {
        // Given:
        let times = ["11:11:11", "00:00:00", "23:59:47", "88:88:88"];
        let rows = 7;

        // When:
        let drawings: Vec<Vec<String>> = times.iter().map(|t| render(t, rows)).collect();

        // Then:
        let expected = usize::from(width("00:00:00", rows));
        assert!(expected > 0, "width reports no cells");
        for (time, art) in times.iter().zip(&drawings) {
            assert_eq!(art.len(), usize::from(rows), "{time} rows");
            for line in art {
                assert_eq!(
                    line.chars().count(),
                    expected,
                    "{time}:\n{}",
                    art.join("\n")
                );
            }
        }
    }
}
