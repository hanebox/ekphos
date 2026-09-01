use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BaseFile {
    pub filters: Option<Filter>,
    pub formulas: BTreeMap<String, String>,
    pub properties: BTreeMap<String, PropertyConfig>,
    pub summaries: BTreeMap<String, String>,
    pub views: Vec<BaseView>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PropertyConfig {
    pub display_name: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BaseView {
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
    pub limit: Option<usize>,
    pub filters: Option<Filter>,
    pub order: Vec<String>,
    pub sort: Vec<SortSpec>,
    pub group_by: Option<SortSpec>,
    pub summaries: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_yaml::Value>,
}

impl Default for BaseView {
    fn default() -> Self {
        Self { kind: "table".to_string(), name: "Table".to_string(), limit: None, filters: None, order: Vec::new(), sort: Vec::new(), group_by: None, summaries: BTreeMap::new(), extra: BTreeMap::new() }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SortSpec {
    pub property: String,
    pub direction: SortDirection,
}

impl Default for SortSpec {
    fn default() -> Self {
        Self { property: String::new(), direction: SortDirection::Asc }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortDirection {
    #[default]
    Asc,
    Desc,
}

impl<'de> Deserialize<'de> for SortDirection {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(if value.eq_ignore_ascii_case("desc") { Self::Desc } else { Self::Asc })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    Expression(String),
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Vec<Filter>),
}

impl<'de> Deserialize<'de> for Filter {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_yaml::Value::deserialize(deserializer)?;
        parse_filter(value).map_err(serde::de::Error::custom)
    }
}

fn parse_filter(value: serde_yaml::Value) -> Result<Filter, String> {
    match value {
        serde_yaml::Value::String(expression) => Ok(Filter::Expression(expression)),
        serde_yaml::Value::Mapping(mapping) if mapping.len() == 1 => {
            let (key, value) = mapping.into_iter().next().expect("mapping length checked");
            let key = key.as_str().ok_or_else(|| "filter operator must be a string".to_string())?;
            let values = match value {
                serde_yaml::Value::Sequence(values) => values.into_iter().map(parse_filter).collect::<Result<Vec<_>, _>>()?,
                value => vec![parse_filter(value)?],
            };
            match key {
                "and" => Ok(Filter::And(values)),
                "or" => Ok(Filter::Or(values)),
                "not" => Ok(Filter::Not(values)),
                _ => Err(format!("unknown filter operator '{key}'")),
            }
        }
        _ => Err("filter must be an expression string or an and/or/not object".to_string()),
    }
}

pub fn parse_base(source: &str) -> Result<BaseFile, serde_yaml::Error> {
    let mut base: BaseFile = serde_yaml::from_str(source)?;
    if base.views.is_empty() {
        base.views.push(BaseView::default());
    }
    for (index, view) in base.views.iter_mut().enumerate() {
        if view.kind.is_empty() {
            view.kind = "table".to_string();
        }
        if view.name.is_empty() {
            view.name = format!("View {}", index + 1);
        }
    }
    Ok(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_official_shape_and_preserves_extension_fields() {
        let source = r#"
filters:
  or:
    - file.hasTag("book")
    - and:
        - status != "done"
        - price > 2
formulas:
  total: price * quantity
properties:
  formula.total:
    displayName: Total
views:
  - type: cards
    name: Books
    order: [file.name, status, formula.total]
    sort:
      - property: status
        direction: DESC
    customSetting: 42
"#;
        let base = parse_base(source).unwrap();
        assert!(matches!(base.filters, Some(Filter::Or(_))));
        assert_eq!(base.formulas["total"], "price * quantity");
        assert_eq!(base.properties["formula.total"].display_name.as_deref(), Some("Total"));
        assert_eq!(base.views[0].kind, "cards");
        assert_eq!(base.views[0].sort[0].direction, SortDirection::Desc);
        assert_eq!(base.views[0].extra["customSetting"], serde_yaml::Value::Number(42.into()));
    }

    #[test]
    fn missing_views_gets_a_table_default() {
        let base = parse_base("filters: file.ext == \"md\"").unwrap();
        assert_eq!(base.views.len(), 1);
        assert_eq!(base.views[0].kind, "table");
    }
}
