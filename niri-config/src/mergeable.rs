pub trait Mergeable: Clone {
    fn merge_with(&mut self, other: &Self);
}

impl<T: Mergeable + Clone> Mergeable for Option<T> {
    fn merge_with(&mut self, other: &Self) {
        match (self, other) {
            (Some(s), Some(o)) => s.merge_with(o),
            (s @ None, Some(o)) => *s = Some(o.clone()),
            _ => {}
        }
    }
}

impl<T: Clone> Mergeable for Vec<T> {
    fn merge_with(&mut self, other: &Self) {
        self.extend_from_slice(other);
    }
}

impl Mergeable for bool {
    fn merge_with(&mut self, other: &Self) {
        *self = *other;
    }
}

impl Mergeable for String {
    fn merge_with(&mut self, other: &Self) {
        self.clone_from(other);
    }
}

impl Mergeable for u16 {
    fn merge_with(&mut self, other: &Self) {
        *self = *other;
    }
}

impl Mergeable for u32 {
    fn merge_with(&mut self, other: &Self) {
        *self = *other;
    }
}

impl Mergeable for i16 {
    fn merge_with(&mut self, other: &Self) {
        *self = *other;
    }
}

impl Mergeable for i32 {
    fn merge_with(&mut self, other: &Self) {
        *self = *other;
    }
}

impl Mergeable for f32 {
    fn merge_with(&mut self, other: &Self) {
        *self = *other;
    }
}

impl Mergeable for f64 {
    fn merge_with(&mut self, other: &Self) {
        *self = *other;
    }
}

impl Mergeable for u8 {
    fn merge_with(&mut self, other: &Self) {
        *self = *other;
    }
}

impl Mergeable for std::path::PathBuf {
    fn merge_with(&mut self, other: &Self) {
        self.clone_from(other);
    }
}

impl Mergeable for crate::utils::Percent {
    fn merge_with(&mut self, other: &Self) {
        self.0 = other.0;
    }
}

impl Mergeable for crate::utils::RegexEq {
    fn merge_with(&mut self, other: &Self) {
        self.0 = other.0.clone();
    }
}

impl Mergeable for niri_ipc::ColumnDisplay {
    fn merge_with(&mut self, other: &Self) {
        *self = *other;
    }
}

#[cfg(test)]
mod tests {
    use niri_macros::Mergeable;

    use super::*;

    #[test]
    fn test_vec_mergeable() {
        let mut base = vec![1, 2, 3];
        let overlay = vec![4, 5];

        base.merge_with(&overlay);
        assert_eq!(base, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_option_mergeable() {
        let mut base: Option<i32> = None;
        let overlay: Option<i32> = Some(42);

        base.merge_with(&overlay);
        assert_eq!(base, Some(42));

        let mut base = Some(10);
        let overlay = Some(20);

        base.merge_with(&overlay);
        assert_eq!(base, Some(20));

        let mut base = Some(10);
        let overlay = None;

        base.merge_with(&overlay);
        assert_eq!(base, Some(10));
    }

    #[test]
    fn test_basic_types_mergeable() {
        let mut base = false;
        let overlay = true;
        base.merge_with(&overlay);
        assert_eq!(base, true);

        let mut base = 10u32;
        let overlay = 20u32;
        base.merge_with(&overlay);
        assert_eq!(base, 20);

        let mut base = String::from("hello");
        let overlay = String::from("world");
        base.merge_with(&overlay);
        assert_eq!(base, "world");
    }

    #[test]
    fn test_nested_option_mergeable() {
        #[derive(Debug, PartialEq, Clone, Default, Mergeable)]
        struct TestStruct {
            value: i32,
        }

        let mut base: Option<TestStruct> = None;
        let overlay: Option<TestStruct> = Some(TestStruct { value: 42 });

        base.merge_with(&overlay);
        assert_eq!(base, Some(TestStruct { value: 42 }));

        let mut base = Some(TestStruct { value: 10 });
        let overlay = Some(TestStruct { value: 20 });

        base.merge_with(&overlay);
        assert_eq!(base, Some(TestStruct { value: 20 }));
    }

    #[test]
    fn test_complex_nested_merging() {
        #[derive(Debug, PartialEq, Clone, Default, Mergeable)]
        struct InnerStruct {
            name: String,
            values: Vec<i32>,
            maybe_flag: Option<bool>,
        }

        #[derive(Debug, PartialEq, Clone, Default, Mergeable)]
        struct OuterStruct {
            inner: InnerStruct,
            optional_inner: Option<InnerStruct>,
        }

        let mut base = OuterStruct {
            inner: InnerStruct {
                name: "base".to_string(),
                values: vec![1, 2],
                maybe_flag: None,
            },
            optional_inner: None,
        };

        let overlay = OuterStruct {
            inner: InnerStruct {
                name: "overlay".to_string(),
                values: vec![3, 4],
                maybe_flag: Some(true),
            },
            optional_inner: Some(InnerStruct {
                name: "optional".to_string(),
                values: vec![5, 6],
                maybe_flag: Some(false),
            }),
        };

        base.merge_with(&overlay);

        assert_eq!(base.inner.name, "overlay");
        assert_eq!(base.inner.values, vec![1, 2, 3, 4]);
        assert_eq!(base.inner.maybe_flag, Some(true));

        assert!(base.optional_inner.is_some());
        let optional = base.optional_inner.unwrap();
        assert_eq!(optional.name, "optional");
        assert_eq!(optional.values, vec![5, 6]);
        assert_eq!(optional.maybe_flag, Some(false));
    }
}
