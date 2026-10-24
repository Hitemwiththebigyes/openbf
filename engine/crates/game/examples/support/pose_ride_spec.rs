//! Positional POSE_RIDE input; independent of assets and rendering.
const USAGE: &str = "POSE_RIDE=<class>,<soldier>,<16 matrix words>[,thr,str,trn]";

#[derive(Debug)]
pub struct RideSpec<'a> {
    pub class: &'a str,
    pub body: &'a str,
    pub matrix: [f32; 16],
    pub input: [f32; 3],
}

pub fn parse(spec: &str) -> Result<RideSpec<'_>, String> {
    let words: Vec<_> = spec.split(',').map(str::trim).collect();
    if !(18..=21).contains(&words.len()) || words[0].is_empty() || words[1].is_empty() {
        return Err(USAGE.into());
    }
    let mut matrix = [0.0; 16];
    let mut input = [0.0; 3];
    for (i, word) in words[2..].iter().enumerate() {
        let value: f32 = word
            .parse()
            .map_err(|_| format!("{USAGE}: invalid number at word {}: {word:?}", i + 3))?;
        if !value.is_finite() {
            return Err(format!("{USAGE}: nonfinite number at word {}", i + 3));
        }
        if i < 16 {
            matrix[i] = value;
        } else {
            input[i - 16] = value;
        }
    }
    Ok(RideSpec {
        class: words[0],
        body: words[1],
        matrix,
        input,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const BASE: &str = "vehicle,soldier,1,0,0,0,0,1,0,0,0,0,1,0,10,20,30,1";

    #[test]
    fn short_specs_return_usage() {
        for spec in ["x", "x,y", ""] {
            assert!(parse(spec).unwrap_err().contains("POSE_RIDE="));
        }
    }

    #[test]
    fn matrix_and_optional_inputs_preserve_positions() {
        let ride = parse(BASE).unwrap();
        assert_eq!((ride.class, ride.body), ("vehicle", "soldier"));
        assert_eq!(&ride.matrix[12..], &[10.0, 20.0, 30.0, 1.0]);
        assert_eq!(ride.input, [0.0; 3]);
        for (suffix, expected) in [
            (",.5", [0.5, 0.0, 0.0]),
            (",.5,-.2", [0.5, -0.2, 0.0]),
            (",.5,-.2,1", [0.5, -0.2, 1.0]),
        ] {
            assert_eq!(parse(&format!("{BASE}{suffix}")).unwrap().input, expected);
        }
    }

    #[test]
    fn malformed_words_cannot_shift_inputs_into_matrix() {
        assert!(parse(&format!("{},.5,.2,1", BASE.replacen(",1,", ",bad,", 1))).is_err());
        for suffix in [",bad", ",0,bad", ",0,0,bad", ",NaN", ",inf"] {
            assert!(parse(&format!("{BASE}{suffix}")).is_err());
        }
        assert!(parse(&BASE.replacen(",1,", ",NaN,", 1)).is_err());
    }
}
