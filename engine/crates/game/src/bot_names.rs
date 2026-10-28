//! The AI soldiers' names as BF2's PS2 makes them (issue #50; statsref
//! README "Bot names"), on every map.
//!
//! - **The lists** (B2 `FUN_0036d880`, decomp line 366898 [V]): a team's
//!   first and last names are the text keys `common.sides.<side>.firstnames.NN`
//!   / `.lastnames.NN` from 01 until one is missing (64 at most), `<side>` by
//!   the team's side: 1 `all`, 2 `imp`, 3 `rep`, 4 `cis`, else `neu`.
//!   English: the Rebels have 48 + 48, the Republic the 24 Greek letters and
//!   no last names, the CIS "Unit" alone, the Empire none at all. A BF1 side
//!   takes the same lists by its side.
//! - **The generator** (B2 `FUN_0036f5f8`, decomp line 368030 [V]; the
//!   division is `divu`, ELF 0x36f7a4 / 0x36f840): one LCG per team,
//!   `x = x * 0x19660d + 0x3c6ef35f`, seeded with the side's number when
//!   the lists are filled. A draw takes two steps and is
//!   `(x1 >> 16 | x2 & 0xffff0000) % n`. A name is a first name (or, with
//!   none, two letters `'A' + draw(26)`), a space, and a last name (or, with
//!   none, three digits `'0' + draw(10)`): "Ben Pulastra", "Gamma 935",
//!   "Unit 538", "KL 987".
//! - **Who gets which** `[D]`: each name is the team's next draw, in the
//!   order its AI soldiers are made. The PS2's lists match: the first 20
//!   draws of each side hold every name seen on the captures (Rebels from
//!   Thrak Farelle, Rayce Farelle, Lan Olesa ...; CIS Unit 538, 182, 790 ...;
//!   Republic Delta 855, Theta 269, Chi 987 ...; Empire DX 178, KL 987 ...),
//!   so a round's names are the same every time, as on the PS2: the side's
//!   number seeds them, not the round.

/// The side's number the PS2's team record carries (`FUN_0036d880`'s
/// switch [V]): it picks the key prefix and seeds the generator.
pub fn side_number(faction: &str) -> u32 {
    match faction {
        "all" => 1,
        "imp" => 2,
        "rep" => 3,
        "cis" => 4,
        _ => 0,
    }
}

/// One team's generator.
#[derive(Debug, Clone)]
pub struct BotNames {
    x: u32,
}

impl BotNames {
    pub fn new(faction: &str) -> Self {
        Self {
            x: side_number(faction),
        }
    }

    /// One draw below `n` (two steps of the LCG).
    fn draw(&mut self, n: u32) -> u32 {
        let a = self.x.wrapping_mul(0x0019_660d).wrapping_add(0x3c6e_f35f);
        let b = a.wrapping_mul(0x0019_660d).wrapping_add(0x3c6e_f35f);
        self.x = b;
        ((a >> 16) | (b & 0xffff_0000)) % n.max(1)
    }

    /// The team's next name.
    pub fn next(&mut self, first: &[String], last: &[String]) -> String {
        let mut name = if first.is_empty() {
            (0..2)
                .map(|_| char::from(b'A' + self.draw(26) as u8))
                .collect()
        } else {
            first[self.draw(first.len() as u32) as usize].clone()
        };
        name.push(' ');
        if last.is_empty() {
            for _ in 0..3 {
                name.push(char::from(b'0' + self.draw(10) as u8));
            }
        } else {
            name.push_str(&last[self.draw(last.len() as u32) as usize]);
        }
        name
    }

    /// The team's first `n` names.
    pub fn list(faction: &str, first: &[String], last: &[String], n: usize) -> Vec<String> {
        let mut g = Self::new(faction);
        (0..n).map(|_| g.next(first, last)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    const GREEK: [&str; 24] = [
        "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta", "Iota", "Kappa",
        "Lamda", "Mu", "Nu", "Xi", "Omikron", "Pi", "Rho", "Sigma", "Tau", "Upsilon", "Phi", "Chi",
        "Psi", "Omega",
    ];

    /// Each side's names come out as the PS2 showed them (statsref README:
    /// every Empire, CIS and Republic name it lists is among the first 20).
    #[test]
    fn each_side_draws_the_ps2s_names() {
        let imp = BotNames::list("imp", &[], &[], 20);
        assert_eq!(&imp[..4], ["DX 178", "KL 987", "TF 997", "JM 221"]);
        for seen in [
            "PG 134", "SU 697", "CX 512", "JM 221", "SF 021", "VH 961", "KG 077", "ZP 778",
            "JE 929", "KL 987", "RP 493",
        ] {
            assert!(imp.iter().any(|n| n == seen), "{seen}");
        }
        let cis = BotNames::list("cis", &strings(&["Unit"]), &[], 20);
        assert_eq!(&cis[..3], ["Unit 538", "Unit 182", "Unit 790"]);
        for seen in [
            "538", "843", "464", "416", "167", "790", "754", "729", "273", "032", "397", "466",
            "253", "182", "892",
        ] {
            assert!(cis.contains(&format!("Unit {seen}")), "{seen}");
        }
        let rep = BotNames::list("rep", &strings(&GREEK), &[], 20);
        assert_eq!(&rep[..3], ["Delta 855", "Theta 269", "Chi 987"]);
        for seen in [
            "Gamma 935",
            "Upsilon 941",
            "Rho 773",
            "Alpha 332",
            "Theta 269",
            "Nu 783",
            "Omega 370",
            "Rho 571",
            "Epsilon 497",
            "Psi 487",
            "Rho 557",
            "Chi 178",
            "Chi 987",
        ] {
            assert!(rep.iter().any(|n| n == seen), "{seen}");
        }
    }

    /// The same side gives the same names every time (the side seeds it);
    /// another side another sequence; a side with first and last names
    /// takes one of each.
    #[test]
    fn names_are_deterministic_per_side() {
        let (f, l) = (strings(&["Ben", "Lot"]), strings(&["Krotan", "Pulastra"]));
        assert_eq!(
            BotNames::list("all", &f, &l, 8),
            BotNames::list("all", &f, &l, 8)
        );
        assert_ne!(
            BotNames::list("all", &[], &[], 8),
            BotNames::list("imp", &[], &[], 8)
        );
        for n in BotNames::list("all", &f, &l, 8) {
            let (a, b) = n.split_once(' ').unwrap();
            assert!(f.iter().any(|x| x == a) && l.iter().any(|x| x == b), "{n}");
        }
        assert_eq!(side_number("team"), 0);
    }
}
