use solution::*;

/// A number that is not Copy or Clone.
#[derive(Debug, PartialEq, Default)]
struct Big(i128);

impl std::ops::Add for Big {
    type Output = Big;
    fn add(self, o: Big) -> Big {
        Big(self.0 + o.0)
    }
}

impl std::ops::Sub for Big {
    type Output = Big;
    fn sub(self, o: Big) -> Big {
        Big(self.0 - o.0)
    }
}

impl std::ops::Neg for Big {
    type Output = Big;
    fn neg(self) -> Big {
        Big(-self.0)
    }
}

impl std::ops::AddAssign for Big {
    fn add_assign(&mut self, o: Big) {
        self.0 += o.0;
    }
}

impl std::ops::Mul for Big {
    type Output = Big;
    fn mul(self, o: Big) -> Big {
        Big(self.0 * o.0)
    }
}

/// Works for any type whose dot product is with itself: needs `Dot`'s default type parameter.
fn len_sq<V: Dot + Copy>(v: V) -> V::Output {
    v.dot(v)
}

#[test]
fn add_assign() {
    let mut v = Vec2::new(1, 2);
    v += Vec2::new(10, 20);
    check!(r#"(1, 2) += (10, 20)"#, v, Vec2::new(11, 22));
}

#[test]
fn empty_sum_is_zero() {
    check!(r#"an empty iterator of Vec2<f64>"#, std::iter::empty::<Vec2<f64>>().sum::<Vec2<f64>>(), Vec2::new(0.0, 0.0));
}

#[test]
fn dot_floats() {
    check!(r#"(0.5, 2.0) . (4.0, -1.0)"#, Vec2::new(0.5, 2.0).dot(Vec2::new(4.0, -1.0)), 0.0);
}

#[test]
fn wrapping() {
    use std::num::Wrapping;
    check!(r#"Wrapping(250u8) + Wrapping(10u8) per part"#, Vec2::new(Wrapping(250u8), Wrapping(1u8)) + Vec2::new(Wrapping(10u8), Wrapping(1u8)), Vec2::new(Wrapping(4u8), Wrapping(2u8)));
}

#[test]
fn big_sum() {
    check!(r#"Big values summed"#, vec![Vec2::new(Big(1), Big(2)), Vec2::new(Big(3), Big(4))].into_iter().sum::<Vec2<Big>>(), Vec2::new(Big(4), Big(6)));
}

#[test]
fn big_dot() {
    check!(r#"Big (2, 3) . (4, 5)"#, Vec2::new(Big(2), Big(3)).dot(Vec2::new(Big(4), Big(5))), Big(23));
}

#[test]
fn scale_by_zero() {
    check!(r#"(7, -7) * 0"#, Vec2::new(7, -7) * std::hint::black_box(0), Vec2::new(0, 0));
}

#[test]
fn nested_vectors() {
    check!(r#"Vec2<Vec2<i32>> addition"#, Vec2::new(Vec2::new(1, 2), Vec2::new(3, 4)) + Vec2::new(Vec2::new(10, 20), Vec2::new(30, 40)), Vec2::new(Vec2::new(11, 22), Vec2::new(33, 44)));
}

#[test]
fn dot_negative() {
    check!(r#"(-2, 3) . (4, 5)"#, Vec2::new(-2, 3).dot(Vec2::new(4, 5)), 7);
}

#[test]
fn i64_extremes() {
    check!(r#"(i64::MAX - 1, 0) + (1, i64::MIN)"#, Vec2::new(i64::MAX - 1, 0) + Vec2::new(1, i64::MIN), Vec2::new(i64::MAX, i64::MIN));
}

#[test]
fn random_vs_tuples() {
    let mut rng = anneal_prelude::Rng::new(4502);
    for _ in 0..300 {
        let (ax, ay, bx, by, k) = (rng.int(-99, 99), rng.int(-99, 99), rng.int(-99, 99), rng.int(-99, 99), rng.int(-9, 9));
        let (a, b) = (Vec2::new(ax, ay), Vec2::new(bx, by));
        let mut c = a;
        c += b;
        check!(format!("a = ({ax}, {ay}), b = ({bx}, {by}), k = {k}"),
               (a + b, a - b, -a, a * k, c, vec![a, b, a].into_iter().sum::<Vec2<i64>>(), a.dot(b), len_sq(b)),
               (Vec2::new(ax + bx, ay + by), Vec2::new(ax - bx, ay - by), Vec2::new(-ax, -ay), Vec2::new(ax * k, ay * k),
                Vec2::new(ax + bx, ay + by), Vec2::new(2 * ax + bx, 2 * ay + by), ax * bx + ay * by, bx * bx + by * by));
    }
}
