/// Mulberry32 — deterministic, fast, no extra crates.
pub(crate) fn rand_u32(state: &mut u32) -> u32 {
    let mut z = *state;
    z = z.wrapping_add(0x6d2b_79f5);
    let mut t = z;
    t = t ^ (t >> 15);
    t = t.wrapping_mul(z | 1);
    t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 0x3d));
    *state = z;
    t ^ (t >> 14)
}

// pub(crate) fn rand_f64(state: &mut u32) -> f64 {
//     rand_u32(state) as f64 / 0x1_0000_0000_u64 as f64
// }

// creates a random number between 0.0 and 1.0
pub(crate) fn rand_f32(state: &mut u32) -> f32 {
    rand_u32(state) as f32 / 0x1_0000_0000_u64 as f32
}
