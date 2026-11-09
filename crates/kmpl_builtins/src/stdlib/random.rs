//! `std:random`.

use std::collections::hash_map::RandomState;
use std::f64::consts::TAU;
use std::hash::{BuildHasher, Hasher};
use std::sync::Arc;

use kmpl_ast::Type;

use crate::{Builtin, Io, ParamType, RetType, Value, ok};

/// The generator of a running program: xoshiro256++. Not for secrets.
#[derive(Debug, Default)]
pub struct Random {
    /// Empty until the first number, or `seed`.
    state: Option<[u64; 4]>,
}

/// Turns one number into a full state, so that similar seeds give unrelated
/// sequences. This is splitmix64.
fn expand(seed: u64) -> [u64; 4] {
    let mut x = seed;
    [0; 4].map(|_| {
        x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = x;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    })
}

/// The standard library's hasher starts from a number given by the system.
fn seed_from_system() -> u64 {
    RandomState::new().build_hasher().finish()
}

impl Random {
    fn seed(&mut self, seed: u64) {
        self.state = Some(expand(seed));
    }

    fn next(&mut self) -> u64 {
        let s = self.state.get_or_insert_with(|| expand(seed_from_system()));
        let result = s[0].wrapping_add(s[3]).rotate_left(23).wrapping_add(s[0]);
        let t = s[1] << 17;
        s[2] ^= s[0];
        s[3] ^= s[1];
        s[1] ^= s[2];
        s[0] ^= s[3];
        s[2] ^= t;
        s[3] = s[3].rotate_left(45);
        result
    }

    /// From 0.0 up to, but not including, 1.0.
    fn real(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Below `range`, which is not zero. Every value is equally likely:
    /// numbers past the last whole multiple of `range` are thrown away.
    fn below(&mut self, range: u64) -> u64 {
        let leftover = (u64::MAX % range + 1) % range;
        let limit = u64::MAX - leftover;
        loop {
            let x = self.next();
            if x <= limit {
                return x % range;
            }
        }
    }

    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i as u64 + 1) as usize);
        }
    }
}

pub(super) static BUILTINS: &[Builtin] = &[
    Builtin {
        name: "_random_seed",
        generic: &[],
        params: &[ParamType::Exact(Type::Int)],
        ret: RetType::Nothing,
        func: |a, io| {
            io.random.seed(a[0].as_int() as u64);
            Ok(None)
        },
    },
    Builtin {
        name: "_random_real",
        generic: &[],
        params: &[],
        ret: RetType::Exact(Type::Real),
        func: |_, io| ok(Value::Real(io.random.real())),
    },
    Builtin {
        name: "_random_int",
        generic: &[],
        params: &[ParamType::Exact(Type::Int), ParamType::Exact(Type::Int)],
        ret: RetType::Exact(Type::Int),
        func: |a, io| {
            let (low, high) = (a[0].as_int(), a[1].as_int());
            if low >= high {
                return Err(format!("the range {low}..{high} of `random.int` is empty"));
            }
            let range = high.wrapping_sub(low) as u64;
            ok(Value::Int(low.wrapping_add(io.random.below(range) as i64)))
        },
    },
    Builtin {
        name: "_random_normal",
        generic: &[],
        params: &[ParamType::Exact(Type::Real), ParamType::Exact(Type::Real)],
        ret: RetType::Exact(Type::Real),
        func: |a, io| {
            let (mean, deviation) = (a[0].as_real(), a[1].as_real());
            if !mean.is_finite() || !deviation.is_finite() || deviation < 0.0 {
                return Err(format!(
                    "`random.normal` needs a finite mean and a deviation of at least 0, \
                     got {mean:?} and {deviation:?}"
                ));
            }
            // Box-Muller. `1 - real()` is never 0, so the logarithm is finite.
            let radius = (-2.0 * (1.0 - io.random.real()).ln()).sqrt();
            let angle = TAU * io.random.real();
            ok(Value::Real(mean + deviation * radius * angle.cos()))
        },
    },
    Builtin {
        name: "_random_shuffle",
        generic: &[],
        params: &[ParamType::ArrayOfElem],
        ret: RetType::ArrayOfElem,
        func: |a, io: &mut Io| {
            let mut array = std::mem::replace(&mut a[0], Value::Int(0));
            match &mut array {
                Value::Array(items) => io.random.shuffle(Arc::make_mut(items).as_mut_slice()),
                Value::Reals(items) => io.random.shuffle(Arc::make_mut(items).as_mut_slice()),
                Value::Ints(items) => io.random.shuffle(Arc::make_mut(items).as_mut_slice()),
                Value::Bytes(items) => io.random.shuffle(Arc::make_mut(items).as_mut_slice()),
                other => panic!("shuffling {other:?}"),
            }
            ok(array)
        },
    },
];

#[cfg(test)]
mod tests {
    use super::Random;

    #[test]
    fn matches_the_published_sequence() {
        let mut random = Random {
            state: Some([1, 2, 3, 4]),
        };
        let numbers = [0; 4].map(|_| random.next());
        assert_eq!(
            numbers,
            [41943041, 58720359, 3588806011781223, 3591011842654386]
        );
    }

    #[test]
    fn real_stays_below_one() {
        let mut random = Random::default();
        random.seed(1);
        assert!((0..10_000).all(|_| (0.0..1.0).contains(&random.real())));
    }

    #[test]
    fn below_reaches_every_value_and_nothing_else() {
        let mut random = Random::default();
        random.seed(7);
        let mut seen = [false; 5];
        for _ in 0..1000 {
            seen[random.below(5) as usize] = true;
        }
        assert_eq!(seen, [true; 5]);
    }

    #[test]
    fn a_shuffle_keeps_the_elements() {
        let mut random = Random::default();
        random.seed(3);
        let mut items: Vec<i32> = (0..100).collect();
        random.shuffle(&mut items);
        assert_ne!(items, (0..100).collect::<Vec<_>>());
        items.sort_unstable();
        assert_eq!(items, (0..100).collect::<Vec<_>>());
    }
}
