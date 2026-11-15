//! Protocol-47 arithmetic kept separate from the modern simulation.
use crate::sprint::Keys;
use horizon_core::geometry::Vec3;

/// 1.8.9 trims each motion component independently before jumping/travel.
pub fn trim_velocity(v: Vec3) -> Vec3 {
    let trim = |x: f64| if x.abs() < 0.005 { 0.0 } else { x };
    Vec3::new(trim(v.x), trim(v.y), trim(v.z))
}

/// Legacy jump sets motionY; it does not retain a larger modern jump velocity.
pub fn jump_velocity(boost: Option<i32>) -> f64 {
    0.41999998688697815 + boost.map_or(0.0, |n| f64::from((n as f32 + 1.0) * 0.1))
}

/// MovementInputFromOptions applies sneak/use scaling before EntityLivingBase's
/// 0.98 damping. Diagonal normalization belongs to moveFlying, not the keyboard.
pub fn movement_input(keys: Keys, using_item: bool) -> [f32; 2] {
    let mut v = [
        (i32::from(keys.left) - i32::from(keys.right)) as f32,
        (i32::from(keys.forward) - i32::from(keys.backward)) as f32,
    ];
    for axis in &mut v {
        if keys.shift { *axis *= 0.3; }
        if using_item { *axis *= 0.2; }
        *axis *= 0.98;
    }
    v
}
