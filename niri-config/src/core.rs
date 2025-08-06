use std::ops::Deref;
use std::str::FromStr;

use bitflags::bitflags;
use knuffel::errors::DecodeError;
use miette::miette;
use smithay::reexports::input;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Modifiers : u8 {
        const CTRL = 1;
        const SHIFT = 1 << 1;
        const ALT = 1 << 2;
        const SUPER = 1 << 3;
        const ISO_LEVEL3_SHIFT = 1 << 4;
        const ISO_LEVEL5_SHIFT = 1 << 5;
        const COMPOSITOR = 1 << 6;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickMethod {
    Clickfinger,
    ButtonAreas,
}

impl From<ClickMethod> for input::ClickMethod {
    fn from(value: ClickMethod) -> Self {
        match value {
            ClickMethod::Clickfinger => Self::Clickfinger,
            ClickMethod::ButtonAreas => Self::ButtonAreas,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccelProfile {
    Adaptive,
    Flat,
}

impl From<AccelProfile> for input::AccelProfile {
    fn from(value: AccelProfile) -> Self {
        match value {
            AccelProfile::Adaptive => Self::Adaptive,
            AccelProfile::Flat => Self::Flat,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollMethod {
    NoScroll,
    TwoFinger,
    Edge,
    OnButtonDown,
}

impl From<ScrollMethod> for input::ScrollMethod {
    fn from(value: ScrollMethod) -> Self {
        match value {
            ScrollMethod::NoScroll => Self::NoScroll,
            ScrollMethod::TwoFinger => Self::TwoFinger,
            ScrollMethod::Edge => Self::Edge,
            ScrollMethod::OnButtonDown => Self::OnButtonDown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TapButtonMap {
    LeftRightMiddle,
    LeftMiddleRight,
}

impl From<TapButtonMap> for input::TapButtonMap {
    fn from(value: TapButtonMap) -> Self {
        match value {
            TapButtonMap::LeftRightMiddle => Self::LeftRightMiddle,
            TapButtonMap::LeftMiddleRight => Self::LeftMiddleRight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Percent(pub f64);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ModKey {
    Ctrl,
    Shift,
    Alt,
    Super,
    IsoLevel3Shift,
    IsoLevel5Shift,
}

impl ModKey {
    pub fn to_modifiers(&self) -> Modifiers {
        match self {
            ModKey::Ctrl => Modifiers::CTRL,
            ModKey::Shift => Modifiers::SHIFT,
            ModKey::Alt => Modifiers::ALT,
            ModKey::Super => Modifiers::SUPER,
            ModKey::IsoLevel3Shift => Modifiers::ISO_LEVEL3_SHIFT,
            ModKey::IsoLevel5Shift => Modifiers::ISO_LEVEL5_SHIFT,
        }
    }
}

// MIN and MAX generics are only used during parsing to check the value.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct FloatOrInt<const MIN: i32, const MAX: i32>(pub f64);

impl<S: knuffel::traits::ErrorSpan, const MIN: i32, const MAX: i32> knuffel::DecodeScalar<S>
    for FloatOrInt<MIN, MAX>
{
    fn type_check(
        type_name: &Option<knuffel::span::Spanned<knuffel::ast::TypeName, S>>,
        ctx: &mut knuffel::decode::Context<S>,
    ) {
        if let Some(type_name) = &type_name {
            ctx.emit_error(DecodeError::unexpected(
                type_name,
                "type name",
                "no type name expected for this node",
            ));
        }
    }

    fn raw_decode(
        val: &knuffel::span::Spanned<knuffel::ast::Literal, S>,
        ctx: &mut knuffel::decode::Context<S>,
    ) -> Result<Self, DecodeError<S>> {
        match &**val {
            knuffel::ast::Literal::Int(ref value) => match value.try_into() {
                Ok(v) => {
                    if (MIN..=MAX).contains(&v) {
                        Ok(FloatOrInt(f64::from(v)))
                    } else {
                        ctx.emit_error(DecodeError::conversion(
                            val,
                            format!("value must be between {MIN} and {MAX}"),
                        ));
                        Ok(FloatOrInt::default())
                    }
                }
                Err(e) => {
                    ctx.emit_error(DecodeError::conversion(val, e));
                    Ok(FloatOrInt::default())
                }
            },
            knuffel::ast::Literal::Decimal(ref value) => match value.try_into() {
                Ok(v) => {
                    if (f64::from(MIN)..=f64::from(MAX)).contains(&v) {
                        Ok(FloatOrInt(v))
                    } else {
                        ctx.emit_error(DecodeError::conversion(
                            val,
                            format!("value must be between {MIN} and {MAX}"),
                        ));
                        Ok(FloatOrInt::default())
                    }
                }
                Err(e) => {
                    ctx.emit_error(DecodeError::conversion(val, e));
                    Ok(FloatOrInt::default())
                }
            },
            _ => {
                ctx.emit_error(DecodeError::unsupported(
                    val,
                    "Unsupported value, only numbers are recognized",
                ));
                Ok(FloatOrInt::default())
            }
        }
    }
}

impl FromStr for ClickMethod {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "clickfinger" => Ok(Self::Clickfinger),
            "button-areas" => Ok(Self::ButtonAreas),
            _ => Err(miette!(
                r#"invalid click method, can be "clickfinger" or "button-areas""#
            )),
        }
    }
}

impl FromStr for AccelProfile {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "adaptive" => Ok(Self::Adaptive),
            "flat" => Ok(Self::Flat),
            _ => Err(miette!(
                r#"invalid accel profile, can be "adaptive" or "flat""#
            )),
        }
    }
}

impl FromStr for ScrollMethod {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "no-scroll" => Ok(Self::NoScroll),
            "two-finger" => Ok(Self::TwoFinger),
            "edge" => Ok(Self::Edge),
            "on-button-down" => Ok(Self::OnButtonDown),
            _ => Err(miette!(
                r#"invalid scroll method, can be "no-scroll", "two-finger", "edge", or "on-button-down""#
            )),
        }
    }
}

impl FromStr for TapButtonMap {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "left-right-middle" => Ok(Self::LeftRightMiddle),
            "left-middle-right" => Ok(Self::LeftMiddleRight),
            _ => Err(miette!(
                r#"invalid tap button map, can be "left-right-middle" or "left-middle-right""#
            )),
        }
    }
}

impl FromStr for Percent {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((value, empty)) = s.split_once('%') else {
            return Err(miette!("value must end with '%'"));
        };

        if !empty.is_empty() {
            return Err(miette!("trailing characters after '%' are not allowed"));
        }

        let value: f64 = value.parse().map_err(|_| miette!("error parsing value"))?;
        Ok(Percent(value / 100.))
    }
}

impl FromStr for ModKey {
    type Err = miette::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match &*s.to_ascii_lowercase() {
            "ctrl" | "control" => Ok(Self::Ctrl),
            "shift" => Ok(Self::Shift),
            "alt" => Ok(Self::Alt),
            "super" | "win" => Ok(Self::Super),
            "iso_level3_shift" | "mod5" => Ok(Self::IsoLevel3Shift),
            "iso_level5_shift" | "mod3" => Ok(Self::IsoLevel5Shift),
            _ => Err(miette!("invalid Mod key: {}", s)),
        }
    }
}

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

impl<T> std::ops::DerefMut for MaybeSet<T> {
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
}
