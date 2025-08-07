use niri_ipc::{ConfiguredMode, Transform};

use crate::appearance::Color;
use crate::core::FloatOrInt;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Outputs(pub Vec<Output>);

#[derive(knuffel::Decode, Debug, Clone, PartialEq)]
pub struct Output {
    #[knuffel(child)]
    pub off: bool,
    #[knuffel(argument)]
    pub name: String,
    #[knuffel(child, unwrap(argument))]
    pub scale: Option<FloatOrInt<0, 10>>,
    #[knuffel(child, unwrap(argument, str), default = Transform::Normal)]
    pub transform: Transform,
    #[knuffel(child)]
    pub position: Option<Position>,
    #[knuffel(child, unwrap(argument, str))]
    pub mode: Option<ConfiguredMode>,
    #[knuffel(child)]
    pub variable_refresh_rate: Option<Vrr>,
    #[knuffel(child)]
    pub focus_at_startup: bool,
    #[knuffel(child)]
    pub background_color: Option<Color>,
    #[knuffel(child)]
    pub backdrop_color: Option<Color>,
}

impl Output {
    pub fn is_vrr_always_on(&self) -> bool {
        self.variable_refresh_rate == Some(Vrr { on_demand: false })
    }

    pub fn is_vrr_on_demand(&self) -> bool {
        self.variable_refresh_rate == Some(Vrr { on_demand: true })
    }

    pub fn is_vrr_always_off(&self) -> bool {
        self.variable_refresh_rate.is_none()
    }
}

impl Default for Output {
    fn default() -> Self {
        Self {
            off: false,
            focus_at_startup: false,
            name: String::new(),
            scale: None,
            transform: Transform::Normal,
            position: None,
            mode: None,
            variable_refresh_rate: None,
            background_color: None,
            backdrop_color: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputName {
    pub connector: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
}

#[derive(knuffel::Decode, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    #[knuffel(property)]
    pub x: i32,
    #[knuffel(property)]
    pub y: i32,
}

#[derive(knuffel::Decode, Debug, Clone, PartialEq, Default)]
pub struct Vrr {
    #[knuffel(property, default = false)]
    pub on_demand: bool,
}

impl FromIterator<Output> for Outputs {
    fn from_iter<T: IntoIterator<Item = Output>>(iter: T) -> Self {
        Self(Vec::from_iter(iter))
    }
}

impl Outputs {
    pub fn find(&self, name: &OutputName) -> Option<&Output> {
        self.0.iter().find(|o| name.matches(&o.name))
    }

    pub fn find_mut(&mut self, name: &OutputName) -> Option<&mut Output> {
        self.0.iter_mut().find(|o| name.matches(&o.name))
    }
}

impl OutputName {
    pub fn from_ipc_output(output: &niri_ipc::Output) -> Self {
        Self {
            connector: output.name.clone(),
            make: (output.make != "Unknown").then(|| output.make.clone()),
            model: (output.model != "Unknown").then(|| output.model.clone()),
            serial: output.serial.clone(),
        }
    }

    /// Returns an output description matching what Smithay's `Output::new()` does.
    pub fn format_description(&self) -> String {
        format!(
            "{} - {} - {}",
            self.make.as_deref().unwrap_or("Unknown"),
            self.model.as_deref().unwrap_or("Unknown"),
            self.connector,
        )
    }

    /// Returns an output name that will match by make/model/serial or, if they are missing, by
    /// connector.
    pub fn format_make_model_serial_or_connector(&self) -> String {
        if self.make.is_none() && self.model.is_none() && self.serial.is_none() {
            self.connector.to_string()
        } else {
            self.format_make_model_serial()
        }
    }

    pub fn format_make_model_serial(&self) -> String {
        let make = self.make.as_deref().unwrap_or("Unknown");
        let model = self.model.as_deref().unwrap_or("Unknown");
        let serial = self.serial.as_deref().unwrap_or("Unknown");
        format!("{make} {model} {serial}")
    }

    pub fn matches(&self, target: &str) -> bool {
        // Match by connector.
        if target.eq_ignore_ascii_case(&self.connector) {
            return true;
        }

        // If no other fields are available, don't try to match by them.
        //
        // This is used by niri msg output.
        if self.make.is_none() && self.model.is_none() && self.serial.is_none() {
            return false;
        }

        // Match by "make model serial" with Unknown if something is missing.
        let make = self.make.as_deref().unwrap_or("Unknown");
        let model = self.model.as_deref().unwrap_or("Unknown");
        let serial = self.serial.as_deref().unwrap_or("Unknown");

        let Some(target_make) = target.get(..make.len()) else {
            return false;
        };
        let rest = &target[make.len()..];
        if !target_make.eq_ignore_ascii_case(make) {
            return false;
        }
        if !rest.starts_with(' ') {
            return false;
        }
        let rest = &rest[1..];

        let Some(target_model) = rest.get(..model.len()) else {
            return false;
        };
        let rest = &rest[model.len()..];
        if !target_model.eq_ignore_ascii_case(model) {
            return false;
        }
        if !rest.starts_with(' ') {
            return false;
        }

        let rest = &rest[1..];
        if !rest.eq_ignore_ascii_case(serial) {
            return false;
        }

        true
    }

    // Similar in spirit to Ord, but I don't want to derive Eq to avoid mistakes (you should use
    // `Self::match`, not Eq).
    pub fn compare(&self, other: &Self) -> std::cmp::Ordering {
        let self_missing_mms = self.make.is_none() && self.model.is_none() && self.serial.is_none();
        let other_missing_mms =
            other.make.is_none() && other.model.is_none() && other.serial.is_none();

        match (self_missing_mms, other_missing_mms) {
            (true, true) => self.connector.cmp(&other.connector),
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => self
                .make
                .cmp(&other.make)
                .then_with(|| self.model.cmp(&other.model))
                .then_with(|| self.serial.cmp(&other.serial))
                .then_with(|| self.connector.cmp(&other.connector)),
        }
    }
}
