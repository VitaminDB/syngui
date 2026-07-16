use crate::core::Size;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Constraints {
    pub min_width: f32,
    pub max_width: f32,
    pub min_height: f32,
    pub max_height: f32,
    pub containing_block: Size,
}

impl Constraints {
    pub fn new(min_w: f32, max_w: f32, min_h: f32, max_h: f32) -> Self {
        let cb_w = if max_w.is_finite() { max_w } else { 0.0 };
        let cb_h = if max_h.is_finite() { max_h } else { 0.0 };
        Self {
            min_width: min_w,
            max_width: max_w,
            min_height: min_h,
            max_height: max_h,
            containing_block: Size::new(cb_w, cb_h),
        }
    }

    pub fn tight(size: Size) -> Self {
        Self {
            min_width: size.width,
            max_width: size.width,
            min_height: size.height,
            max_height: size.height,
            containing_block: size,
        }
    }

    pub fn loose(size: Size) -> Self {
        Self {
            min_width: 0.0,
            max_width: size.width,
            min_height: 0.0,
            max_height: size.height,
            containing_block: size,
        }
    }

    pub fn expand() -> Self {
        Self {
            min_width: f32::INFINITY,
            max_width: f32::INFINITY,
            min_height: f32::INFINITY,
            max_height: f32::INFINITY,
            containing_block: Size::zero(),
        }
    }

    pub fn with_containing_block(mut self, cb: Size) -> Self {
        self.containing_block = cb;
        self
    }

    pub fn constrain(&self, size: Size) -> Size {
        Size::new(
            size.width.clamp(self.min_width.min(self.max_width), self.max_width),
            size.height.clamp(self.min_height.min(self.max_height), self.max_height),
        )
    }

    pub fn constrain_width(&self, width: f32) -> f32 {
        width.clamp(self.min_width.min(self.max_width), self.max_width)
    }

    pub fn constrain_height(&self, height: f32) -> f32 {
        height.clamp(self.min_height.min(self.max_height), self.max_height)
    }

    pub fn normalize(&self) -> Self {
        Self {
            min_width: self.min_width.min(self.max_width),
            max_width: self.max_width,
            min_height: self.min_height.min(self.max_height),
            max_height: self.max_height,
            containing_block: self.containing_block,
        }
    }

    pub fn is_tight(&self) -> bool {
        self.min_width == self.max_width && self.min_height == self.max_height
    }

    pub fn has_bounded_width(&self) -> bool {
        self.max_width < f32::INFINITY
    }

    pub fn has_bounded_height(&self) -> bool {
        self.max_height < f32::INFINITY
    }

    pub fn loosen(&self) -> Self {
        Self {
            min_width: 0.0,
            max_width: self.max_width,
            min_height: 0.0,
            max_height: self.max_height,
            containing_block: self.containing_block,
        }
    }
}

impl Constraints {
    pub fn hash_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.min_width.to_bits().hash(&mut hasher);
        self.max_width.to_bits().hash(&mut hasher);
        self.min_height.to_bits().hash(&mut hasher);
        self.max_height.to_bits().hash(&mut hasher);
        hasher.finish()
    }
}

impl Default for Constraints {
    fn default() -> Self {
        Self {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
            containing_block: Size::zero(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stores_values() {
        let c = Constraints::new(10.0, 100.0, 20.0, 200.0);
        assert_eq!(c.min_width, 10.0);
        assert_eq!(c.max_width, 100.0);
        assert_eq!(c.min_height, 20.0);
        assert_eq!(c.max_height, 200.0);
    }

    #[test]
    fn tight_min_equals_max() {
        let c = Constraints::tight(Size::new(50.0, 30.0));
        assert_eq!(c.min_width, 50.0);
        assert_eq!(c.max_width, 50.0);
        assert_eq!(c.min_height, 30.0);
        assert_eq!(c.max_height, 30.0);
    }

    #[test]
    fn loose_min_is_zero() {
        let c = Constraints::loose(Size::new(200.0, 100.0));
        assert_eq!(c.min_width, 0.0);
        assert_eq!(c.max_width, 200.0);
        assert_eq!(c.min_height, 0.0);
        assert_eq!(c.max_height, 100.0);
    }

    #[test]
    fn expand_all_infinity() {
        let c = Constraints::expand();
        assert_eq!(c.min_width, f32::INFINITY);
        assert_eq!(c.max_width, f32::INFINITY);
        assert_eq!(c.min_height, f32::INFINITY);
        assert_eq!(c.max_height, f32::INFINITY);
    }

    #[test]
    fn default_unbounded() {
        let c = Constraints::default();
        assert_eq!(c.min_width, 0.0);
        assert_eq!(c.max_width, f32::INFINITY);
        assert_eq!(c.min_height, 0.0);
        assert_eq!(c.max_height, f32::INFINITY);
    }

    #[test]
    fn constrain_within_range() {
        let c = Constraints::new(10.0, 100.0, 20.0, 200.0);
        let s = c.constrain(Size::new(50.0, 80.0));
        assert_eq!(s.width, 50.0);
        assert_eq!(s.height, 80.0);
    }

    #[test]
    fn constrain_clamps_below_min() {
        let c = Constraints::new(10.0, 100.0, 20.0, 200.0);
        let s = c.constrain(Size::new(5.0, 10.0));
        assert_eq!(s.width, 10.0);
        assert_eq!(s.height, 20.0);
    }

    #[test]
    fn constrain_clamps_above_max() {
        let c = Constraints::new(10.0, 100.0, 20.0, 200.0);
        let s = c.constrain(Size::new(500.0, 999.0));
        assert_eq!(s.width, 100.0);
        assert_eq!(s.height, 200.0);
    }

    #[test]
    fn constrain_width_clamps() {
        let c = Constraints::new(10.0, 100.0, 0.0, 100.0);
        assert_eq!(c.constrain_width(5.0), 10.0);
        assert_eq!(c.constrain_width(50.0), 50.0);
        assert_eq!(c.constrain_width(200.0), 100.0);
    }

    #[test]
    fn constrain_height_clamps() {
        let c = Constraints::new(0.0, 100.0, 20.0, 200.0);
        assert_eq!(c.constrain_height(10.0), 20.0);
        assert_eq!(c.constrain_height(100.0), 100.0);
        assert_eq!(c.constrain_height(300.0), 200.0);
    }

    #[test]
    fn is_tight_true() {
        let c = Constraints::tight(Size::new(50.0, 50.0));
        assert!(c.is_tight());
    }

    #[test]
    fn is_tight_false() {
        let c = Constraints::loose(Size::new(50.0, 50.0));
        assert!(!c.is_tight());
    }

    #[test]
    fn is_tight_partial() {
        let c = Constraints::new(50.0, 50.0, 0.0, 100.0);
        assert!(!c.is_tight());
    }

    #[test]
    fn has_bounded_width() {
        assert!(Constraints::new(0.0, 100.0, 0.0, 100.0).has_bounded_width());
        assert!(!Constraints::default().has_bounded_width());
        assert!(!Constraints::expand().has_bounded_width());
    }

    #[test]
    fn has_bounded_height() {
        assert!(Constraints::new(0.0, 100.0, 0.0, 200.0).has_bounded_height());
        assert!(!Constraints::default().has_bounded_height());
    }

    #[test]
    fn loosen_resets_min_keeps_max() {
        let c = Constraints::new(50.0, 100.0, 30.0, 200.0).loosen();
        assert_eq!(c.min_width, 0.0);
        assert_eq!(c.max_width, 100.0);
        assert_eq!(c.min_height, 0.0);
        assert_eq!(c.max_height, 200.0);
    }

    #[test]
    fn loosen_tight_becomes_loose() {
        let c = Constraints::tight(Size::new(50.0, 50.0)).loosen();
        assert!(!c.is_tight());
        assert_eq!(c.min_width, 0.0);
        assert_eq!(c.max_width, 50.0);
    }

    #[test]
    fn hash_key_deterministic() {
        let c = Constraints::new(10.0, 100.0, 20.0, 200.0);
        assert_eq!(c.hash_key(), c.hash_key());
    }

    #[test]
    fn hash_key_same_values_same_hash() {
        let a = Constraints::new(10.0, 100.0, 20.0, 200.0);
        let b = Constraints::new(10.0, 100.0, 20.0, 200.0);
        assert_eq!(a.hash_key(), b.hash_key());
    }

    #[test]
    fn hash_key_different_values_different_hash() {
        let a = Constraints::new(10.0, 100.0, 20.0, 200.0);
        let b = Constraints::new(10.0, 100.0, 20.0, 201.0);
        assert_ne!(a.hash_key(), b.hash_key());
    }

    #[test]
    fn hash_key_infinity() {
        let a = Constraints::default();
        let b = Constraints::default();
        assert_eq!(a.hash_key(), b.hash_key());
    }

    #[test]
    fn constraints_eq() {
        let a = Constraints::new(1.0, 2.0, 3.0, 4.0);
        let b = Constraints::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(a, b);
    }

    #[test]
    fn constraints_clone() {
        let a = Constraints::new(1.0, 2.0, 3.0, 4.0);
        let b = a;
        assert_eq!(a, b);
    }
}
