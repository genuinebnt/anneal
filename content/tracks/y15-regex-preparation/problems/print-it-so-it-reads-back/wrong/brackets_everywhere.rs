#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Formula {
    Var(char),
    Not(Box<Formula>),
    And(Vec<Formula>),
    Or(Vec<Formula>),
}

fn strength(formula: &Formula) -> u8 {
    match formula {
        Formula::Or(_) => 0,
        Formula::And(_) => 1,
        Formula::Not(_) => 2,
        Formula::Var(_) => 3,
    }
}

/// Writes `formula` into a place that takes only forms of at least this strength without brackets.
fn write(formula: &Formula, at_least: u8, out: &mut String) {
    if strength(formula) < at_least || (at_least > 0 && strength(formula) < 2) {
        out.push('(');
        write(formula, 0, out);
        out.push(')');
        return;
    }
    match formula {
        Formula::Var(c) => out.push(*c),
        Formula::Not(inner) => {
            out.push('!');
            write(inner, 2, out);
        }
        Formula::And(parts) | Formula::Or(parts) => {
            let (separator, own) = if matches!(formula, Formula::And(_)) {
                ('&', 1)
            } else {
                ('|', 0)
            };
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    out.push(separator);
                }
                write(part, own + 1, out);
            }
        }
    }
}

/// Text that reads back as exactly this tree, with no unnecessary brackets.
pub fn to_text(formula: &Formula) -> String {
    let mut out = String::new();
    write(formula, 0, &mut out);
    out
}

/// splitmix64: any seed, zero included, gives a well-mixed sequence.
struct Random(u64);

impl Random {
    fn below(&mut self, n: u64) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z ^ (z >> 31)) % n
    }
}

fn grow(random: &mut Random, height: usize) -> Formula {
    let form = if height <= 1 { 0 } else { random.below(4) };
    match form {
        0 => Formula::Var(['a', 'b', 'c'][random.below(3) as usize]),
        1 => Formula::Not(Box::new(grow(random, height - 1))),
        _ => {
            let count = 2 + random.below(3);
            let parts = (0..count).map(|_| grow(random, height - 1)).collect();
            if form == 2 {
                Formula::And(parts)
            } else {
                Formula::Or(parts)
            }
        }
    }
}

/// A made-up formula of height at most `max_height`, the same for the same arguments.
pub fn sample(seed: u64, max_height: usize) -> Formula {
    grow(&mut Random(seed), max_height)
}
