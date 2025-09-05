use std::ops::{Deref, DerefMut};

use knuffel::errors::DecodeError;

use crate::mergeable::Mergeable;

#[derive(Debug, Clone, PartialEq)]
pub struct MaybeSet<T> {
    value: T,
    is_set: bool,
}

impl<T> MaybeSet<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            is_set: true,
        }
    }

    pub fn unset(value: T) -> Self {
        Self {
            value,
            is_set: false,
        }
    }

    pub fn is_set(&self) -> bool {
        self.is_set
    }

    pub fn get(&self) -> &T {
        &self.value
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    pub fn into_inner(self) -> T {
        self.value
    }

    pub fn value(&self) -> &T {
        &self.value
    }

    pub fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    pub fn into_value(self) -> T {
        self.value
    }

    pub fn set(&mut self, value: T) {
        self.value = value;
        self.is_set = true;
    }

    pub fn unset_self(&mut self) {
        self.is_set = false;
    }
}

impl<T: Default> Default for MaybeSet<T> {
    fn default() -> Self {
        Self::unset(T::default())
    }
}

impl<T> Deref for MaybeSet<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for MaybeSet<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T> From<T> for MaybeSet<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl From<MaybeSet<u16>> for u64 {
    fn from(maybe_set: MaybeSet<u16>) -> Self {
        u64::from(maybe_set.value)
    }
}

impl<T: Copy> Copy for MaybeSet<T> {}

impl<T: Clone> Mergeable for MaybeSet<T> {
    fn merge_with(&mut self, other: &Self) {
        if other.is_set {
            self.value = other.value.clone();
            self.is_set = true;
        }
    }
}

impl<S, T> knuffel::Decode<S> for MaybeSet<T>
where
    S: knuffel::traits::ErrorSpan,
    T: knuffel::Decode<S>,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        T::decode_node(node, ctx).map(Self::new)
    }
}

/// Wrapper for boolean flags that marks bare flags as true
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoolFlag(pub MaybeSet<bool>);

impl Default for BoolFlag {
    fn default() -> Self {
        Self(MaybeSet::unset(false))
    }
}

impl std::ops::Deref for BoolFlag {
    type Target = bool;

    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

impl Mergeable for BoolFlag {
    fn merge_with(&mut self, other: &Self) {
        self.0.merge_with(&other.0);
    }
}

impl<S> knuffel::Decode<S> for BoolFlag
where
    S: knuffel::traits::ErrorSpan,
{
    fn decode_node(
        node: &knuffel::ast::SpannedNode<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        knuffel::decode::check_flag_node(node, ctx);
        Ok(Self(MaybeSet::new(true)))
    }
}

impl<S, T> knuffel::DecodeScalar<S> for MaybeSet<T>
where
    S: knuffel::traits::ErrorSpan,
    T: knuffel::DecodeScalar<S>,
{
    fn type_check(
        type_name: &Option<knuffel::span::Spanned<knuffel::ast::TypeName, S>>,
        ctx: &mut knuffel::decode::Context<S>,
    ) {
        T::type_check(type_name, ctx);
    }

    fn raw_decode(
        val: &knuffel::span::Spanned<knuffel::ast::Literal, S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        T::raw_decode(val, ctx).map(Self::new)
    }

    fn decode(
        val: &knuffel::ast::Value<S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        T::decode(val, ctx).map(Self::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_set_value() {
        let maybe_set = MaybeSet::new(42);
        assert_eq!(*maybe_set, 42);
        assert!(maybe_set.is_set());
        assert_eq!(maybe_set.value(), &42);
        assert_eq!(maybe_set.into_value(), 42);
    }

    #[test]
    fn unset_creates_unset_value() {
        let maybe_set = MaybeSet::unset(42);
        assert_eq!(*maybe_set, 42);
        assert!(!maybe_set.is_set());
        assert_eq!(maybe_set.value(), &42);
    }

    #[test]
    fn default_is_unset() {
        let maybe_set: MaybeSet<i32> = MaybeSet::default();
        assert_eq!(*maybe_set, 0);
        assert!(!maybe_set.is_set());
    }

    #[test]
    fn from_creates_set_value() {
        let maybe_set: MaybeSet<i32> = 42.into();
        assert_eq!(*maybe_set, 42);
        assert!(maybe_set.is_set());
    }

    #[test]
    fn deref_works() {
        let maybe_set = MaybeSet::new(String::from("test"));
        assert_eq!(maybe_set.len(), 4);
        assert_eq!(&*maybe_set, "test");
    }

    #[test]
    fn value_mut_works() {
        let mut maybe_set = MaybeSet::new(42);
        *maybe_set.value_mut() = 100;
        assert_eq!(*maybe_set, 100);
        assert!(maybe_set.is_set());
    }

    #[test]
    fn set_marks_as_set() {
        let mut maybe_set = MaybeSet::unset(0);
        assert!(!maybe_set.is_set());

        maybe_set.set(42);
        assert_eq!(*maybe_set, 42);
        assert!(maybe_set.is_set());
    }

    #[test]
    fn unset_self_marks_as_unset() {
        let mut maybe_set = MaybeSet::new(42);
        assert!(maybe_set.is_set());

        maybe_set.unset_self();
        assert_eq!(*maybe_set, 42);
        assert!(!maybe_set.is_set());
    }

    #[test]
    fn equality_works() {
        let set1 = MaybeSet::new(42);
        let set2 = MaybeSet::new(42);
        let unset1 = MaybeSet::unset(42);
        let unset2 = MaybeSet::unset(42);

        assert_eq!(set1, set2);
        assert_eq!(unset1, unset2);
        assert_ne!(set1, unset1);
    }

    #[test]
    fn clone_works() {
        let original = MaybeSet::new(String::from("test"));
        let cloned = original.clone();

        assert_eq!(original, cloned);
        assert!(cloned.is_set());
        assert_eq!(*cloned, "test");
    }

    #[test]
    fn get_methods_work() {
        let maybe_set = MaybeSet::new(42);
        assert_eq!(maybe_set.get(), &42);
        assert_eq!(maybe_set.into_inner(), 42);
    }

    #[test]
    fn get_mut_methods_work() {
        let mut maybe_set = MaybeSet::new(42);
        *maybe_set.get_mut() = 100;
        assert_eq!(*maybe_set, 100);
        assert!(maybe_set.is_set());
    }

    #[test]
    fn deref_mut_works() {
        let mut maybe_set = MaybeSet::new(String::from("test"));
        maybe_set.push_str("ing");
        assert_eq!(&*maybe_set, "testing");
        assert!(maybe_set.is_set());
    }

    #[test]
    fn mergeable_only_merges_when_set() {
        let mut base = MaybeSet::new(100);
        let overlay_unset = MaybeSet::unset(200);
        let overlay_set = MaybeSet::new(300);

        // Merging unset value should not change base
        base.merge_with(&overlay_unset);
        assert_eq!(*base, 100);
        assert!(base.is_set());

        // Merging set value should update base
        base.merge_with(&overlay_set);
        assert_eq!(*base, 300);
        assert!(base.is_set());
    }

    #[test]
    fn mergeable_can_set_unset_value() {
        let mut base = MaybeSet::unset(100);
        let overlay = MaybeSet::new(200);

        base.merge_with(&overlay);
        assert_eq!(*base, 200);
        assert!(base.is_set());
    }

    #[test]
    fn mergeable_with_struct_preserves_explicitly_set_values() {
        use niri_macros::Mergeable as MergeableDerive;

        #[derive(Debug, PartialEq, Clone, MergeableDerive)]
        struct TestStruct {
            duration: MaybeSet<u32>,
            name: MaybeSet<String>,
        }

        impl Default for TestStruct {
            fn default() -> Self {
                Self {
                    duration: MaybeSet::unset(200),
                    name: MaybeSet::unset("default".to_string()),
                }
            }
        }

        // Start with defaults (all unset)
        let mut base = TestStruct::default();
        assert!(!base.duration.is_set());
        assert!(!base.name.is_set());
        assert_eq!(*base.duration, 200);
        assert_eq!(*base.name, "default");

        // First config sets duration explicitly
        let overlay1 = TestStruct {
            duration: MaybeSet::new(500),
            name: MaybeSet::unset("default".to_string()),
        };

        base.merge_with(&overlay1);
        assert!(base.duration.is_set());
        assert!(!base.name.is_set());
        assert_eq!(*base.duration, 500);
        assert_eq!(*base.name, "default");

        // Second config sets name explicitly but NOT duration
        let overlay2 = TestStruct {
            duration: MaybeSet::unset(999),
            name: MaybeSet::new("custom".to_string()),
        };

        base.merge_with(&overlay2);
        // Duration should stay 500 (previously set, new config didn't override)
        assert!(base.duration.is_set());
        assert_eq!(*base.duration, 500);

        // Name should be updated to custom
        assert!(base.name.is_set());
        assert_eq!(*base.name, "custom");
    }

    #[test]
    fn mergeable_with_struct_can_override_set_values() {
        use niri_macros::Mergeable as MergeableDerive;

        #[derive(Debug, PartialEq, Clone, MergeableDerive)]
        struct TestStruct {
            duration: MaybeSet<u32>,
            name: MaybeSet<String>,
        }

        let mut base = TestStruct {
            duration: MaybeSet::new(500),
            name: MaybeSet::new("base".to_string()),
        };

        let overlay = TestStruct {
            duration: MaybeSet::new(1000),
            name: MaybeSet::unset("ignored".to_string()),
        };

        base.merge_with(&overlay);
        assert_eq!(*base.duration, 1000); // Overridden
        assert_eq!(*base.name, "base"); // Preserved
    }
}
