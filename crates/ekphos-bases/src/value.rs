use chrono::{NaiveDate, NaiveDateTime};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, Default)]
pub enum Value {
    #[default]
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Date(NaiveDateTime),
    Duration(i64),
    List(Vec<Value>),
    Object(BTreeMap<String, Value>),
    Link {
        target: String,
        display: Option<String>,
    },
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Bool(left), Self::Bool(right)) => left == right,
            (Self::Number(left), Self::Number(right)) => left.to_bits() == right.to_bits(),
            (Self::String(left), Self::String(right)) => left == right,
            (Self::Date(left), Self::Date(right)) => left == right,
            (Self::Duration(left), Self::Duration(right)) => left == right,
            (Self::List(left), Self::List(right)) => left == right,
            (Self::Object(left), Self::Object(right)) => left == right,
            (Self::Link { target: left, .. }, Self::Link { target: right, .. }) => left == right,
            _ => false,
        }
    }
}

impl Value {
    pub fn from_yaml(value: &serde_yaml::Value) -> Self {
        match value {
            serde_yaml::Value::Null => Self::Null,
            serde_yaml::Value::Bool(value) => Self::Bool(*value),
            serde_yaml::Value::Number(value) => value.as_f64().map(Self::Number).unwrap_or(Self::Null),
            serde_yaml::Value::String(value) => parse_date(value).map(Self::Date).unwrap_or_else(|| parse_wikilink(value).unwrap_or_else(|| Self::String(value.clone()))),
            serde_yaml::Value::Sequence(values) => Self::List(values.iter().map(Self::from_yaml).collect()),
            serde_yaml::Value::Mapping(values) => Self::Object(values.iter().filter_map(|(key, value)| key.as_str().map(|key| (key.to_string(), Self::from_yaml(value)))).collect()),
            serde_yaml::Value::Tagged(value) => Self::from_yaml(&value.value),
        }
    }

    pub fn truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::String(value) => !value.is_empty(),
            Self::List(value) => !value.is_empty(),
            Self::Object(value) => !value.is_empty(),
            Self::Date(_) | Self::Duration(_) | Self::Link { .. } => true,
        }
    }

    pub fn as_number(&self) -> Option<f64> {
        match self {
            Self::Number(value) => Some(*value),
            Self::Duration(value) => Some(*value as f64),
            _ => None,
        }
    }

    pub fn plain_text(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Bool(value) => value.to_string(),
            Self::Number(value) if value.fract() == 0.0 => format!("{value:.0}"),
            Self::Number(value) => value.to_string(),
            Self::String(value) => value.clone(),
            Self::Date(value) if value.time() == chrono::NaiveTime::MIN => value.date().format("%Y-%m-%d").to_string(),
            Self::Date(value) => value.format("%Y-%m-%d %H:%M:%S").to_string(),
            Self::Duration(value) => value.to_string(),
            Self::List(values) => values.iter().map(Self::plain_text).collect::<Vec<_>>().join(", "),
            Self::Object(_) => "[object]".to_string(),
            Self::Link { target, display } => display.clone().unwrap_or_else(|| target.clone()),
        }
    }

    pub fn compare(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Null, Self::Null) => Ordering::Equal,
            (Self::Null, _) => Ordering::Greater,
            (_, Self::Null) => Ordering::Less,
            (Self::Number(left), Self::Number(right)) => left.partial_cmp(right).unwrap_or(Ordering::Equal),
            (Self::Duration(left), Self::Duration(right)) => left.cmp(right),
            (Self::Date(left), Self::Date(right)) => left.cmp(right),
            (Self::Bool(left), Self::Bool(right)) => left.cmp(right),
            _ => self.plain_text().to_lowercase().cmp(&other.plain_text().to_lowercase()),
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.plain_text())
    }
}

pub fn parse_date(value: &str) -> Option<NaiveDateTime> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok().and_then(|date| date.and_hms_opt(0, 0, 0)).or_else(|| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S").ok()).or_else(|| NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S").ok())
}

fn parse_wikilink(value: &str) -> Option<Value> {
    let inner = value.strip_prefix("[[")?.strip_suffix("]]")?;
    let (target, display) = inner.split_once('|').map_or((inner, None), |(target, display)| (target, Some(display.to_string())));
    (!target.is_empty()).then(|| Value::Link { target: target.to_string(), display })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_values_remain_typed() {
        let yaml: serde_yaml::Value = serde_yaml::from_str("[true, 3.5, 2026-08-31, '[[Book|Dune]]']").unwrap();
        let Value::List(values) = Value::from_yaml(&yaml) else { panic!("expected list") };
        assert!(matches!(values[0], Value::Bool(true)));
        assert!(matches!(values[1], Value::Number(value) if value == 3.5));
        assert!(matches!(values[2], Value::Date(_)));
        assert!(matches!(&values[3], Value::Link { target, display: Some(display) } if target == "Book" && display == "Dune"));
    }
}
