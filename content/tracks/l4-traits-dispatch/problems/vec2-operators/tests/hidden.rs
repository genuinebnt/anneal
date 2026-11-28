use solution::*;

#[test]
fn total_empty() {
    check!(r#"total(&[])"#, total(&[]), Vec2 { x: 0, y: 0 });
}

#[test]
fn add_assign() {
    let mut p = Vec2 { x: 1, y: 1 };
    p += Vec2 { x: 5, y: -5 };
    p += Vec2 { x: 5, y: -5 };
    check!(r#"p += (5, -5) twice"#, p, Vec2 { x: 11, y: -9 });
}

#[test]
fn perpendicular_dot() {
    check!(r#"(3, 4) * (-4, 3)"#, Vec2 { x: 3, y: 4 } * Vec2 { x: -4, y: 3 }, 0);
}

#[test]
fn scale_by_zero() {
    check!(r#"0 * (7, 8)"#, std::hint::black_box(0) * Vec2 { x: 7, y: 8 }, Vec2 { x: 0, y: 0 });
}

#[test]
fn scale_by_negative() {
    check!(r#"(7, -8) * -2"#, Vec2 { x: 7, y: -8 } * -2, Vec2 { x: -14, y: 16 });
}

#[test]
fn double_negation() {
    check!(r#"-(-(5, 6))"#, -(-Vec2 { x: 5, y: 6 }), Vec2 { x: 5, y: 6 });
}

#[test]
fn walk_backwards() {
    check!(r#"walk [((2, 1), -3)]"#, walk(&[(Vec2 { x: 2, y: 1 }, -3)]), Vec2 { x: -6, y: -3 });
}

#[test]
fn sub_is_not_commutative() {
    check!(r#"(0, 0) - (1, 2)"#, Vec2 { x: 0, y: 0 } - Vec2 { x: 1, y: 2 }, Vec2 { x: -1, y: -2 });
}

#[test]
fn sum_of_mapped() {
    check!(r#"(1..=4) mapped to (i, i*i), summed"#, (1..=4).map(|i| Vec2 { x: i, y: i * i }).sum::<Vec2>(), Vec2 { x: 10, y: 30 });
}

#[test]
fn random_vs_brute_force() {
    let mut rng = anneal_prelude::Rng::new(4414);
    for _ in 0..300 {
        let c: Vec<i64> = rng.vec(4, -50, 50);
        let k = rng.int(-9, 9);
        let (a, b) = (Vec2 { x: c[0], y: c[1] }, Vec2 { x: c[2], y: c[3] });
        let d = format!("a = {a:?}, b = {b:?}, k = {k}");
        check!(d.clone(), a + b * k, Vec2 { x: c[0] + c[2] * k, y: c[1] + c[3] * k });
        check!(d.clone(), k * (a - b), Vec2 { x: k * (c[0] - c[2]), y: k * (c[1] - c[3]) });
        check!(d.clone(), a * b, c[0] * c[2] + c[1] * c[3]);
        check!(d.clone(), -a + b, Vec2 { x: c[2] - c[0], y: c[3] - c[1] });
        let n = rng.below(5);
        let pts: Vec<Vec2> = (0..n).map(|_| Vec2 { x: rng.int(-9, 9), y: rng.int(-9, 9) }).collect();
        let want = pts.iter().fold((0, 0), |(x, y), p| (x + p.x, y + p.y));
        check!(format!("total({pts:?})"), total(&pts), Vec2 { x: want.0, y: want.1 });
    }
}
