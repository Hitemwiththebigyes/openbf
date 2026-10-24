//! Bound the diagnostic's square grid before converting floats or iterating.
// One million XZ probes per map is already expensive (each checks eight levels).
// This is a diagnostic work limit, not a gameplay or census acceptance value.
const MAX_PROBES: usize = 1_000_000;

pub fn parse_cell(value: Option<&str>) -> Result<f32, String> {
    let cell: f32 = match value {
        None => 2.0,
        Some(value) => value
            .parse()
            .map_err(|_| "ORDN_CELL must be a finite positive number".to_owned())?,
    };
    if !cell.is_finite() || cell <= 0.0 {
        return Err("ORDN_CELL must be a finite positive number".into());
    }
    Ok(cell)
}

pub fn bounds(x0: f32, z0: f32, x1: f32, z1: f32, cell: f32) -> Result<(f32, f32, usize), String> {
    if !cell.is_finite() || cell <= 0.0 {
        return Err("ORDN_CELL must be a finite positive number".into());
    }
    if ![x0, z0, x1, z1].iter().all(|v| v.is_finite()) || x0 > x1 || z0 > z1 {
        return Err("ORDN_CELL grid requires finite, ordered hub bounds".into());
    }
    // Keep the original f32 arithmetic and inclusive loop extent for valid grids.
    let (x0, z0) = ((x0 / cell).floor() * cell, (z0 / cell).floor() * cell);
    let steps = ((x1 - x0).max(z1 - z0) / cell).trunc();
    // Previously n = steps as usize + 1 and loops used 0..=n.
    // Compare before the cast/additions so saturation and overflow cannot occur.
    if !steps.is_finite() || !(0.0..=998.0).contains(&steps) {
        return Err(format!(
            "ORDN_CELL grid exceeds {MAX_PROBES} XZ probes per map; increase ORDN_CELL"
        ));
    }
    let n = steps as usize + 1;
    debug_assert!((n + 1) * (n + 1) <= MAX_PROBES);
    Ok((x0, z0, n))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_cells_fail_promptly() {
        for value in ["0", "-1", "NaN", "inf", "-inf", "bad", ""] {
            assert!(parse_cell(Some(value)).is_err(), "{value}");
        }
        assert_eq!(parse_cell(None).unwrap(), 2.0);
    }
    #[test]
    fn tiny_and_overflowing_grids_are_rejected_before_casting() {
        assert!(bounds(0.0, 0.0, 100.0, 100.0, 1e-30).is_err());
        assert!(bounds(-f32::MAX, 0.0, f32::MAX, 1.0, 2.0).is_err());
        assert!(bounds(f32::NAN, 0.0, 10.0, 10.0, 2.0).is_err());
        assert!(bounds(0.0, 0.0, 999.0, 999.0, 1.0).is_err());
        assert_eq!(bounds(0.0, 0.0, 998.0, 998.0, 1.0).unwrap().2, 999);
    }
    #[test]
    fn ordinary_grid_keeps_alignment_and_inclusive_extent() {
        assert_eq!(bounds(-3.0, 1.0, 5.0, 7.0, 2.0).unwrap(), (-4.0, 0.0, 5));
        let (x0, z0, n) = bounds(-3.0, 1.0, 5.0, 7.0, 2.0).unwrap();
        let points: Vec<_> = (0..=n)
            .flat_map(|z| (0..=n).map(move |x| (x0 + x as f32 * 2.0, z0 + z as f32 * 2.0)))
            .collect();
        assert_eq!(points.len(), 36);
        assert_eq!(points[0], (-4.0, 0.0));
        assert_eq!(points[35], (6.0, 10.0));
    }
}
