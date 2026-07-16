use bitflags::bitflags;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct DirtyFlags: u32 {
        const LAYOUT = 1 << 0;
        const RENDER = 1 << 1;
        const PAINT = 1 << 2;
        const STATE = 1 << 3;
        const CHILDREN = 1 << 4;
        const ANIMATION = 1 << 5;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_empty() {
        let f = DirtyFlags::default();
        assert!(f.is_empty());
    }

    #[test]
    fn individual_flags() {
        assert_eq!(DirtyFlags::LAYOUT.bits(), 1);
        assert_eq!(DirtyFlags::RENDER.bits(), 2);
        assert_eq!(DirtyFlags::PAINT.bits(), 4);
        assert_eq!(DirtyFlags::STATE.bits(), 8);
        assert_eq!(DirtyFlags::CHILDREN.bits(), 16);
        assert_eq!(DirtyFlags::ANIMATION.bits(), 32);
    }

    #[test]
    fn insert_and_contains() {
        let mut f = DirtyFlags::empty();
        f.insert(DirtyFlags::LAYOUT);
        assert!(f.contains(DirtyFlags::LAYOUT));
        assert!(!f.contains(DirtyFlags::RENDER));
    }

    #[test]
    fn remove_flag() {
        let mut f = DirtyFlags::LAYOUT | DirtyFlags::RENDER;
        f.remove(DirtyFlags::LAYOUT);
        assert!(!f.contains(DirtyFlags::LAYOUT));
        assert!(f.contains(DirtyFlags::RENDER));
    }

    #[test]
    fn bitwise_or() {
        let f = DirtyFlags::LAYOUT | DirtyFlags::PAINT;
        assert!(f.contains(DirtyFlags::LAYOUT));
        assert!(f.contains(DirtyFlags::PAINT));
        assert!(!f.contains(DirtyFlags::RENDER));
    }

    #[test]
    fn bitwise_and() {
        let a = DirtyFlags::LAYOUT | DirtyFlags::RENDER;
        let b = DirtyFlags::RENDER | DirtyFlags::PAINT;
        let c = a & b;
        assert!(c.contains(DirtyFlags::RENDER));
        assert!(!c.contains(DirtyFlags::LAYOUT));
        assert!(!c.contains(DirtyFlags::PAINT));
    }

    #[test]
    fn all_flags() {
        let all = DirtyFlags::all();
        assert!(all.contains(DirtyFlags::LAYOUT));
        assert!(all.contains(DirtyFlags::RENDER));
        assert!(all.contains(DirtyFlags::PAINT));
        assert!(all.contains(DirtyFlags::STATE));
        assert!(all.contains(DirtyFlags::CHILDREN));
        assert!(all.contains(DirtyFlags::ANIMATION));
    }

    #[test]
    fn is_empty_after_remove_all() {
        let mut f = DirtyFlags::LAYOUT | DirtyFlags::RENDER;
        f.remove(DirtyFlags::LAYOUT);
        f.remove(DirtyFlags::RENDER);
        assert!(f.is_empty());
    }

    #[test]
    fn not_complement() {
        let f = !DirtyFlags::LAYOUT;
        assert!(!f.contains(DirtyFlags::LAYOUT));
        assert!(f.contains(DirtyFlags::RENDER));
        assert!(f.contains(DirtyFlags::PAINT));
    }

    #[test]
    fn clone_and_eq() {
        let a = DirtyFlags::LAYOUT | DirtyFlags::STATE;
        let b = a;
        assert_eq!(a, b);
    }
}
