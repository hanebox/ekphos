//! Pure, filesystem-independent support for Obsidian Bases files.
//!
//! The crate intentionally returns a typed view model rather than Markdown or
//! HTML so terminal, export, and future editing surfaces can share semantics.

mod expr;
mod model;
mod value;

pub use expr::{parse_expression, BinaryOp, Expr, ExprError, UnaryOp};
pub use model::{parse_base, BaseFile, BaseView, Filter, PropertyConfig, SortDirection, SortSpec};
pub use value::{parse_date, Value};

use chrono::{Datelike, NaiveDateTime, Timelike};
use ekphos_core::NoteId;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone)]
pub struct BaseRecord {
    pub id: NoteId,
    pub path: String,
    pub name: String,
    pub extension: String,
    pub folder: String,
    pub size: u64,
    pub created: Option<NaiveDateTime>,
    pub modified: Option<NaiveDateTime>,
    pub tags: Vec<String>,
    pub links: Vec<String>,
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Default)]
pub struct Corpus {
    pub records: Vec<BaseRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseDiagnostic {
    pub context: String,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct BaseColumn {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone)]
pub struct BaseRow {
    pub id: NoteId,
    pub path: String,
    pub cells: Vec<Value>,
}

#[derive(Debug, Clone)]
struct EvaluatedRow {
    row: BaseRow,
    sort_values: Vec<Value>,
    group_value: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct BaseGroup {
    pub label: Option<String>,
    pub rows: Vec<BaseRow>,
}

#[derive(Debug, Clone)]
pub struct BaseResult {
    pub view_index: usize,
    pub view_name: String,
    pub view_kind: String,
    pub columns: Vec<BaseColumn>,
    pub groups: Vec<BaseGroup>,
    pub summaries: Vec<Option<Value>>,
    pub matched_rows: usize,
    pub diagnostics: Vec<BaseDiagnostic>,
}

#[derive(Debug, Clone)]
enum CompiledFilter {
    Expression { source: String, expression: Result<Expr, ExprError> },
    And(Vec<CompiledFilter>),
    Or(Vec<CompiledFilter>),
    Not(Vec<CompiledFilter>),
}

#[derive(Debug, Clone)]
struct CompiledView {
    filter: Option<CompiledFilter>,
}

#[derive(Debug, Clone)]
pub struct CompiledBase {
    definition: BaseFile,
    filter: Option<CompiledFilter>,
    formulas: BTreeMap<String, Result<Expr, ExprError>>,
    summaries: BTreeMap<String, Result<Expr, ExprError>>,
    views: Vec<CompiledView>,
    diagnostics: Vec<BaseDiagnostic>,
}

impl CompiledBase {
    pub fn compile(definition: BaseFile) -> Self {
        let filter = definition.filters.as_ref().map(compile_filter);
        let views: Vec<CompiledView> = definition.views.iter().map(|view| CompiledView { filter: view.filters.as_ref().map(compile_filter) }).collect();
        let formulas: BTreeMap<String, Result<Expr, ExprError>> = definition.formulas.iter().map(|(name, source)| (name.clone(), parse_expression(source))).collect();
        let summaries: BTreeMap<String, Result<Expr, ExprError>> = definition.summaries.iter().map(|(name, source)| (name.clone(), parse_expression(source))).collect();
        let mut diagnostics = Vec::new();
        collect_filter_diagnostics(filter.as_ref(), "global filter", &mut diagnostics);
        for (index, view) in views.iter().enumerate() {
            collect_filter_diagnostics(view.filter.as_ref(), &format!("view {} filter", index + 1), &mut diagnostics);
        }
        for (name, expression) in &formulas {
            if let Err(error) = expression {
                diagnostics.push(BaseDiagnostic { context: format!("formula.{name}"), message: error.to_string() });
            }
        }
        for (name, expression) in &summaries {
            if let Err(error) = expression {
                diagnostics.push(BaseDiagnostic { context: format!("summary.{name}"), message: error.to_string() });
            }
        }
        Self { definition, filter, formulas, summaries, views, diagnostics }
    }

    pub fn definition(&self) -> &BaseFile {
        &self.definition
    }

    pub fn evaluate_view(&self, corpus: &Corpus, view_index: usize, now: NaiveDateTime) -> BaseResult {
        let view_index = view_index.min(self.definition.views.len().saturating_sub(1));
        let view = &self.definition.views[view_index];
        let compiled_view = &self.views[view_index];
        let columns = self.columns_for(view, corpus);
        let mut diagnostics = self.diagnostics.clone();
        let mut rows = Vec::new();
        for record in &corpus.records {
            let mut context = EvalContext::new(self, corpus, record, now);
            let global_match = self.filter.as_ref().is_none_or(|filter| evaluate_filter(filter, &mut context, &mut diagnostics));
            let view_match = compiled_view.filter.as_ref().is_none_or(|filter| evaluate_filter(filter, &mut context, &mut diagnostics));
            if !global_match || !view_match {
                continue;
            }
            let cells = columns.iter().map(|column| context.resolve_path(&column.key)).collect();
            let sort_values = view.sort.iter().map(|sort| context.resolve_path(&sort.property)).collect();
            let group_value = view.group_by.as_ref().map(|group| context.resolve_path(&group.property));
            rows.push(EvaluatedRow { row: BaseRow { id: record.id, path: record.path.clone(), cells }, sort_values, group_value });
        }
        let matched_rows = rows.len();
        rows.sort_by(|left, right| compare_rows(left, right, view));
        if let Some(limit) = view.limit {
            rows.truncate(limit);
        }
        let summaries = columns.iter().enumerate().map(|(index, column)| view.summaries.get(&column.key).and_then(|name| self.evaluate_summary(name, rows.iter().filter_map(|row| row.row.cells.get(index)).cloned().collect(), corpus, now, &mut diagnostics))).collect();
        let groups = group_rows(rows, view);
        deduplicate_diagnostics(&mut diagnostics);
        BaseResult { view_index, view_name: view.name.clone(), view_kind: view.kind.clone(), columns, groups, summaries, matched_rows, diagnostics }
    }

    fn columns_for(&self, view: &BaseView, corpus: &Corpus) -> Vec<BaseColumn> {
        let keys = if view.order.is_empty() {
            let mut keys = vec!["file.name".to_string()];
            let mut discovered = corpus.records.iter().flat_map(|record| record.properties.keys().cloned()).collect::<Vec<_>>();
            discovered.sort();
            discovered.dedup();
            keys.extend(discovered.into_iter().take(7));
            keys
        } else {
            view.order.clone()
        };
        keys.into_iter()
            .map(|key| {
                let label = self.definition.properties.get(&key).and_then(|property| property.display_name.clone()).unwrap_or_else(|| default_label(&key));
                BaseColumn { key, label }
            })
            .collect()
    }

    fn evaluate_summary(&self, name: &str, values: Vec<Value>, corpus: &Corpus, now: NaiveDateTime, diagnostics: &mut Vec<BaseDiagnostic>) -> Option<Value> {
        if let Some(value) = builtin_summary(name, &values) {
            return Some(value);
        }
        let expression = self.summaries.get(name)?;
        let expression = match expression {
            Ok(expression) => expression,
            Err(_) => return None,
        };
        let placeholder = corpus.records.first()?;
        let mut context = EvalContext::new(self, corpus, placeholder, now);
        context.summary_values = Some(values);
        match context.evaluate(expression) {
            Ok(value) => Some(value),
            Err(message) => {
                diagnostics.push(BaseDiagnostic { context: format!("summary.{name}"), message });
                None
            }
        }
    }
}

fn default_label(key: &str) -> String {
    key.strip_prefix("formula.")
        .or_else(|| key.strip_prefix("note."))
        .or_else(|| key.strip_prefix("file."))
        .unwrap_or(key)
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
        })
        .collect::<Vec<String>>()
        .join(" ")
}

fn compile_filter(filter: &Filter) -> CompiledFilter {
    match filter {
        Filter::Expression(source) => CompiledFilter::Expression { source: source.clone(), expression: parse_expression(source) },
        Filter::And(filters) => CompiledFilter::And(filters.iter().map(compile_filter).collect()),
        Filter::Or(filters) => CompiledFilter::Or(filters.iter().map(compile_filter).collect()),
        Filter::Not(filters) => CompiledFilter::Not(filters.iter().map(compile_filter).collect()),
    }
}

fn collect_filter_diagnostics(filter: Option<&CompiledFilter>, context: &str, diagnostics: &mut Vec<BaseDiagnostic>) {
    let Some(filter) = filter else {
        return;
    };
    match filter {
        CompiledFilter::Expression { source, expression: Err(error) } => diagnostics.push(BaseDiagnostic { context: context.to_string(), message: format!("{error}: {source}") }),
        CompiledFilter::Expression { .. } => {}
        CompiledFilter::And(filters) | CompiledFilter::Or(filters) | CompiledFilter::Not(filters) => {
            for filter in filters {
                collect_filter_diagnostics(Some(filter), context, diagnostics);
            }
        }
    }
}

fn evaluate_filter(filter: &CompiledFilter, context: &mut EvalContext<'_>, diagnostics: &mut Vec<BaseDiagnostic>) -> bool {
    match filter {
        CompiledFilter::Expression { source, expression: Ok(expression) } => match context.evaluate(expression) {
            Ok(value) => value.truthy(),
            Err(message) => {
                diagnostics.push(BaseDiagnostic { context: "filter".to_string(), message: format!("{message}: {source}") });
                false
            }
        },
        CompiledFilter::Expression { .. } => false,
        CompiledFilter::And(filters) => filters.iter().all(|filter| evaluate_filter(filter, context, diagnostics)),
        CompiledFilter::Or(filters) => filters.iter().any(|filter| evaluate_filter(filter, context, diagnostics)),
        CompiledFilter::Not(filters) => !filters.iter().any(|filter| evaluate_filter(filter, context, diagnostics)),
    }
}

fn compare_rows(left: &EvaluatedRow, right: &EvaluatedRow, view: &BaseView) -> Ordering {
    for (index, sort) in view.sort.iter().enumerate() {
        let ordering = left.sort_values[index].compare(&right.sort_values[index]);
        if ordering != Ordering::Equal {
            return if sort.direction == SortDirection::Desc { ordering.reverse() } else { ordering };
        }
    }
    left.row.path.to_lowercase().cmp(&right.row.path.to_lowercase())
}

fn group_rows(rows: Vec<EvaluatedRow>, view: &BaseView) -> Vec<BaseGroup> {
    let Some(group_by) = &view.group_by else {
        return vec![BaseGroup { label: None, rows: rows.into_iter().map(|row| row.row).collect() }];
    };
    let mut groups: Vec<BaseGroup> = Vec::new();
    for row in rows {
        let label = row.group_value.as_ref().map_or_else(String::new, Value::plain_text);
        if let Some(group) = groups.iter_mut().find(|group| group.label.as_deref() == Some(label.as_str())) {
            group.rows.push(row.row);
        } else {
            groups.push(BaseGroup { label: Some(label), rows: vec![row.row] });
        }
    }
    groups.sort_by(|left, right| {
        let ordering = left.label.cmp(&right.label);
        if group_by.direction == SortDirection::Desc {
            ordering.reverse()
        } else {
            ordering
        }
    });
    groups
}

fn builtin_summary(name: &str, values: &[Value]) -> Option<Value> {
    let numbers = || values.iter().filter_map(Value::as_number).collect::<Vec<_>>();
    match name.to_ascii_lowercase().as_str() {
        "average" => {
            let numbers = numbers();
            (!numbers.is_empty()).then(|| Value::Number(numbers.iter().sum::<f64>() / numbers.len() as f64))
        }
        "sum" => Some(Value::Number(numbers().iter().sum())),
        "min" => numbers().into_iter().reduce(f64::min).map(Value::Number),
        "max" => numbers().into_iter().reduce(f64::max).map(Value::Number),
        "range" => {
            let numbers = numbers();
            let min = numbers.iter().copied().reduce(f64::min)?;
            let max = numbers.iter().copied().reduce(f64::max)?;
            Some(Value::Number(max - min))
        }
        "checked" => Some(Value::Number(values.iter().filter(|value| matches!(value, Value::Bool(true))).count() as f64)),
        "unchecked" => Some(Value::Number(values.iter().filter(|value| matches!(value, Value::Bool(false))).count() as f64)),
        "empty" => Some(Value::Number(values.iter().filter(|value| !value.truthy()).count() as f64)),
        "filled" => Some(Value::Number(values.iter().filter(|value| value.truthy()).count() as f64)),
        "unique" => Some(Value::Number(values.iter().map(Value::plain_text).collect::<HashSet<_>>().len() as f64)),
        _ => None,
    }
}

fn deduplicate_diagnostics(diagnostics: &mut Vec<BaseDiagnostic>) {
    let mut seen = HashSet::new();
    diagnostics.retain(|diagnostic| seen.insert((diagnostic.context.clone(), diagnostic.message.clone())));
}

struct EvalContext<'a> {
    base: &'a CompiledBase,
    corpus: &'a Corpus,
    record: &'a BaseRecord,
    now: NaiveDateTime,
    formula_cache: HashMap<String, Value>,
    formula_stack: HashSet<String>,
    summary_values: Option<Vec<Value>>,
    steps: usize,
}

impl<'a> EvalContext<'a> {
    fn new(base: &'a CompiledBase, corpus: &'a Corpus, record: &'a BaseRecord, now: NaiveDateTime) -> Self {
        Self { base, corpus, record, now, formula_cache: HashMap::new(), formula_stack: HashSet::new(), summary_values: None, steps: 0 }
    }

    fn evaluate(&mut self, expression: &Expr) -> Result<Value, String> {
        const MAX_STEPS: usize = 16_384;
        self.steps += 1;
        if self.steps > MAX_STEPS {
            return Err("expression exceeded its evaluation budget".to_string());
        }
        match expression {
            Expr::Null => Ok(Value::Null),
            Expr::Bool(value) => Ok(Value::Bool(*value)),
            Expr::Number(value) => Ok(Value::Number(*value)),
            Expr::String(value) => Ok(Value::String(value.clone())),
            Expr::Reference(name) => Ok(self.resolve_path(name)),
            Expr::Member(_, _) | Expr::Index(_, _) if expression_path(expression).is_some() => Ok(self.resolve_path(&expression_path(expression).expect("path checked"))),
            Expr::Member(value, member) => {
                let value = self.evaluate(value)?;
                Ok(member_value(&value, member))
            }
            Expr::Index(value, index) => {
                let value = self.evaluate(value)?;
                let index = self.evaluate(index)?;
                Ok(index_value(&value, &index))
            }
            Expr::Call(callee, arguments) => self.call(callee, arguments),
            Expr::Unary { op, value } => {
                let value = self.evaluate(value)?;
                Ok(match op {
                    UnaryOp::Not => Value::Bool(!value.truthy()),
                    UnaryOp::Negate => value.as_number().map(|number| Value::Number(-number)).unwrap_or(Value::Null),
                })
            }
            Expr::Binary { left, op, right } => {
                if *op == BinaryOp::And {
                    let left = self.evaluate(left)?;
                    return if left.truthy() { self.evaluate(right).map(|right| Value::Bool(right.truthy())) } else { Ok(Value::Bool(false)) };
                }
                if *op == BinaryOp::Or {
                    let left = self.evaluate(left)?;
                    return if left.truthy() { Ok(Value::Bool(true)) } else { self.evaluate(right).map(|right| Value::Bool(right.truthy())) };
                }
                let left = self.evaluate(left)?;
                let right = self.evaluate(right)?;
                Ok(binary_value(&left, *op, &right))
            }
        }
    }

    fn resolve_path(&mut self, path: &str) -> Value {
        if path == "values" {
            return self.summary_values.clone().map(Value::List).unwrap_or(Value::Null);
        }
        if let Some(name) = path.strip_prefix("formula.") {
            return self.evaluate_formula(name);
        }
        if path == "file" || path == "note" || path == "formula" || path == "this" {
            return Value::Null;
        }
        if let Some(property) = path.strip_prefix("file.") {
            return self.file_property(property);
        }
        let property = path.strip_prefix("note.").unwrap_or(path);
        if let Some(value) = self.record.properties.get(property) {
            return value.clone();
        }
        let mut segments = property.split('.');
        let Some(first) = segments.next() else {
            return Value::Null;
        };
        let mut value = self.record.properties.get(first).cloned().unwrap_or(Value::Null);
        for segment in segments {
            value = member_value(&value, segment);
        }
        value
    }

    fn file_property(&self, property: &str) -> Value {
        match property {
            "name" | "basename" => Value::String(self.record.name.clone()),
            "path" => Value::String(self.record.path.clone()),
            "ext" => Value::String(self.record.extension.clone()),
            "folder" => Value::String(self.record.folder.clone()),
            "size" => Value::Number(self.record.size as f64),
            "ctime" => self.record.created.map(Value::Date).unwrap_or(Value::Null),
            "mtime" => self.record.modified.map(Value::Date).unwrap_or(Value::Null),
            "tags" => Value::List(self.record.tags.iter().cloned().map(Value::String).collect()),
            "links" | "backlinks" | "embeds" => Value::List(self.record.links.iter().cloned().map(|target| Value::Link { target, display: None }).collect()),
            "properties" => Value::Object(self.record.properties.clone()),
            "file" => Value::Link { target: self.record.path.clone(), display: Some(self.record.name.clone()) },
            _ => Value::Null,
        }
    }

    fn evaluate_formula(&mut self, name: &str) -> Value {
        if let Some(value) = self.formula_cache.get(name) {
            return value.clone();
        }
        if !self.formula_stack.insert(name.to_string()) {
            return Value::Null;
        }
        let expression = self.base.formulas.get(name).and_then(|expression| expression.as_ref().ok()).cloned();
        let value = expression.as_ref().and_then(|expression| self.evaluate(expression).ok()).unwrap_or(Value::Null);
        self.formula_stack.remove(name);
        self.formula_cache.insert(name.to_string(), value.clone());
        value
    }

    fn call(&mut self, callee: &Expr, arguments: &[Expr]) -> Result<Value, String> {
        let values = arguments.iter().map(|argument| self.evaluate(argument)).collect::<Result<Vec<_>, _>>()?;
        match callee {
            Expr::Reference(name) => Ok(self.global_function(name, &values)),
            Expr::Member(receiver, method) => {
                if expression_path(receiver).as_deref() == Some("file") {
                    return Ok(self.file_method(method, &values));
                }
                let receiver = self.evaluate(receiver)?;
                Ok(value_method(&receiver, method, &values, self.now, self.corpus))
            }
            _ => Ok(Value::Null),
        }
    }

    fn global_function(&self, name: &str, arguments: &[Value]) -> Value {
        match name {
            "if" => arguments.first().map_or(Value::Null, |condition| if condition.truthy() { arguments.get(1).cloned().unwrap_or(Value::Null) } else { arguments.get(2).cloned().unwrap_or(Value::Null) }),
            "list" => match arguments.first() {
                Some(Value::List(values)) => Value::List(values.clone()),
                Some(value) => Value::List(vec![value.clone()]),
                None => Value::List(Vec::new()),
            },
            "date" => arguments.first().and_then(value_to_date).map(Value::Date).unwrap_or(Value::Null),
            "today" => self.now.date().and_hms_opt(0, 0, 0).map(Value::Date).unwrap_or(Value::Null),
            "now" => Value::Date(self.now),
            "link" => arguments.first().map(|target| Value::Link { target: target.plain_text(), display: arguments.get(1).map(Value::plain_text) }).unwrap_or(Value::Null),
            "min" => arguments.iter().filter_map(Value::as_number).reduce(f64::min).map(Value::Number).unwrap_or(Value::Null),
            "max" => arguments.iter().filter_map(Value::as_number).reduce(f64::max).map(Value::Number).unwrap_or(Value::Null),
            _ => Value::Null,
        }
    }

    fn file_method(&self, method: &str, arguments: &[Value]) -> Value {
        let first = arguments.first().map(Value::plain_text).unwrap_or_default();
        match method {
            "hasTag" => Value::Bool(self.record.tags.iter().any(|tag| normalize_tag(tag) == normalize_tag(&first))),
            "inFolder" => {
                let folder = first.trim_matches('/');
                Value::Bool(self.record.folder == folder || self.record.folder.starts_with(&format!("{folder}/")))
            }
            "hasLink" => Value::Bool(self.record.links.iter().any(|link| link.eq_ignore_ascii_case(first.trim_matches(['[', ']'])))),
            "hasProperty" => Value::Bool(self.record.properties.contains_key(&first)),
            "asLink" => Value::Link { target: self.record.path.clone(), display: arguments.first().map(Value::plain_text).or_else(|| Some(self.record.name.clone())) },
            _ => Value::Null,
        }
    }
}

fn expression_path(expression: &Expr) -> Option<String> {
    match expression {
        Expr::Reference(name) => Some(name.clone()),
        Expr::Member(value, member) => Some(format!("{}.{}", expression_path(value)?, member)),
        Expr::Index(value, index) => match index.as_ref() {
            Expr::String(member) => Some(format!("{}.{}", expression_path(value)?, member)),
            _ => None,
        },
        _ => None,
    }
}

fn member_value(value: &Value, member: &str) -> Value {
    match (value, member) {
        (Value::Object(values), member) => values.get(member).cloned().unwrap_or(Value::Null),
        (Value::List(values), "length") => Value::Number(values.len() as f64),
        (Value::String(value), "length") => Value::Number(value.chars().count() as f64),
        (Value::Date(value), "year") => Value::Number(value.year() as f64),
        (Value::Date(value), "month") => Value::Number(value.month() as f64),
        (Value::Date(value), "day") => Value::Number(value.day() as f64),
        (Value::Date(value), "hour") => Value::Number(value.hour() as f64),
        (Value::Duration(value), "days") => Value::Number(*value as f64 / 86_400_000.0),
        (Value::Duration(value), "hours") => Value::Number(*value as f64 / 3_600_000.0),
        _ => Value::Null,
    }
}

fn index_value(value: &Value, index: &Value) -> Value {
    match (value, index) {
        (Value::Object(values), Value::String(key)) => values.get(key).cloned().unwrap_or(Value::Null),
        (Value::List(values), Value::Number(index)) if *index >= 0.0 => values.get(*index as usize).cloned().unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

fn binary_value(left: &Value, operator: BinaryOp, right: &Value) -> Value {
    match operator {
        BinaryOp::Equal => Value::Bool(values_equal(left, right)),
        BinaryOp::NotEqual => Value::Bool(!values_equal(left, right)),
        BinaryOp::Greater => Value::Bool(!matches!(left, Value::Null) && !matches!(right, Value::Null) && left.compare(right) == Ordering::Greater),
        BinaryOp::GreaterEqual => Value::Bool(!matches!(left, Value::Null) && !matches!(right, Value::Null) && left.compare(right) != Ordering::Less),
        BinaryOp::Less => Value::Bool(!matches!(left, Value::Null) && !matches!(right, Value::Null) && left.compare(right) == Ordering::Less),
        BinaryOp::LessEqual => Value::Bool(!matches!(left, Value::Null) && !matches!(right, Value::Null) && left.compare(right) != Ordering::Greater),
        BinaryOp::Add => match (left, right) {
            (Value::Number(left), Value::Number(right)) => Value::Number(left + right),
            (Value::Date(date), Value::Duration(duration)) | (Value::Duration(duration), Value::Date(date)) => date.checked_add_signed(chrono::Duration::milliseconds(*duration)).map(Value::Date).unwrap_or(Value::Null),
            (Value::String(left), right) => Value::String(format!("{left}{}", right.plain_text())),
            (left, Value::String(right)) => parse_duration(right)
                .and_then(|duration| match left {
                    Value::Date(date) => date.checked_add_signed(chrono::Duration::milliseconds(duration)).map(Value::Date),
                    _ => None,
                })
                .unwrap_or_else(|| Value::String(format!("{}{right}", left.plain_text()))),
            _ => Value::Null,
        },
        BinaryOp::Subtract => match (left, right) {
            (Value::Number(left), Value::Number(right)) => Value::Number(left - right),
            (Value::Date(left), Value::Date(right)) => Value::Duration((*left - *right).num_milliseconds()),
            (Value::Date(date), Value::Duration(duration)) => date.checked_sub_signed(chrono::Duration::milliseconds(*duration)).map(Value::Date).unwrap_or(Value::Null),
            (Value::Date(date), Value::String(duration)) => parse_duration(duration).and_then(|duration| date.checked_sub_signed(chrono::Duration::milliseconds(duration))).map(Value::Date).unwrap_or(Value::Null),
            _ => Value::Null,
        },
        BinaryOp::Multiply => numeric_binary(left, right, |left, right| left * right),
        BinaryOp::Divide => {
            if right.as_number() == Some(0.0) {
                Value::Null
            } else {
                numeric_binary(left, right, |left, right| left / right)
            }
        }
        BinaryOp::Modulo => {
            if right.as_number() == Some(0.0) {
                Value::Null
            } else {
                numeric_binary(left, right, |left, right| left % right)
            }
        }
        BinaryOp::And | BinaryOp::Or => unreachable!("short-circuited by evaluator"),
    }
}

fn numeric_binary(left: &Value, right: &Value, operation: impl FnOnce(f64, f64) -> f64) -> Value {
    left.as_number().zip(right.as_number()).map(|(left, right)| Value::Number(operation(left, right))).unwrap_or(Value::Null)
}

fn values_equal(left: &Value, right: &Value) -> bool {
    left == right || matches!((left, right), (Value::Link { target, .. }, Value::String(value)) | (Value::String(value), Value::Link { target, .. }) if target == value)
}

fn value_method(value: &Value, method: &str, arguments: &[Value], _now: NaiveDateTime, _corpus: &Corpus) -> Value {
    match method {
        "toString" => Value::String(value.plain_text()),
        "isEmpty" => Value::Bool(!value.truthy()),
        "contains" => {
            let Some(needle) = arguments.first() else {
                return Value::Bool(false);
            };
            match value {
                Value::String(value) => Value::Bool(value.contains(&needle.plain_text())),
                Value::List(values) => Value::Bool(values.iter().any(|value| values_equal(value, needle))),
                _ => Value::Bool(false),
            }
        }
        "startsWith" => Value::Bool(matches!(value, Value::String(value) if value.starts_with(&arguments.first().map(Value::plain_text).unwrap_or_default()))),
        "endsWith" => Value::Bool(matches!(value, Value::String(value) if value.ends_with(&arguments.first().map(Value::plain_text).unwrap_or_default()))),
        "lower" | "toLowerCase" => Value::String(value.plain_text().to_lowercase()),
        "upper" | "toUpperCase" => Value::String(value.plain_text().to_uppercase()),
        "trim" => Value::String(value.plain_text().trim().to_string()),
        "round" => value.as_number().map(|value| Value::Number(value.round())).unwrap_or(Value::Null),
        "floor" => value.as_number().map(|value| Value::Number(value.floor())).unwrap_or(Value::Null),
        "ceil" => value.as_number().map(|value| Value::Number(value.ceil())).unwrap_or(Value::Null),
        "toFixed" => {
            let precision = arguments.first().and_then(Value::as_number).unwrap_or(0.0).clamp(0.0, 20.0) as usize;
            value.as_number().map(|value| Value::String(format!("{value:.precision$}"))).unwrap_or(Value::Null)
        }
        "date" => match value {
            Value::Date(value) => value.date().and_hms_opt(0, 0, 0).map(Value::Date).unwrap_or(Value::Null),
            _ => Value::Null,
        },
        "format" => match value {
            Value::Date(value) => Value::String(format_date(value, &arguments.first().map(Value::plain_text).unwrap_or_default())),
            _ => Value::Null,
        },
        "join" => match value {
            Value::List(values) => Value::String(values.iter().map(Value::plain_text).collect::<Vec<_>>().join(&arguments.first().map(Value::plain_text).unwrap_or_else(|| ", ".to_string()))),
            _ => Value::Null,
        },
        "first" => match value {
            Value::List(values) => values.first().cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        },
        "last" => match value {
            Value::List(values) => values.last().cloned().unwrap_or(Value::Null),
            _ => Value::Null,
        },
        "sum" => match value {
            Value::List(values) => Value::Number(values.iter().filter_map(Value::as_number).sum()),
            _ => Value::Null,
        },
        "mean" => match value {
            Value::List(values) => {
                let values = values.iter().filter_map(Value::as_number).collect::<Vec<_>>();
                if values.is_empty() {
                    Value::Null
                } else {
                    Value::Number(values.iter().sum::<f64>() / values.len() as f64)
                }
            }
            _ => Value::Null,
        },
        _ => Value::Null,
    }
}

fn value_to_date(value: &Value) -> Option<NaiveDateTime> {
    match value {
        Value::Date(value) => Some(*value),
        Value::String(value) => parse_date(value),
        _ => None,
    }
}

fn parse_duration(value: &str) -> Option<i64> {
    let value = value.trim();
    let split = value.find(|character: char| !character.is_ascii_digit() && character != '.' && character != '-')?;
    let amount: f64 = value[..split].trim().parse().ok()?;
    let unit = value[split..].trim().to_ascii_lowercase();
    let milliseconds = match unit.as_str() {
        "ms" | "millisecond" | "milliseconds" => 1.0,
        "s" | "second" | "seconds" => 1_000.0,
        "m" | "minute" | "minutes" => 60_000.0,
        "h" | "hour" | "hours" => 3_600_000.0,
        "d" | "day" | "days" => 86_400_000.0,
        "w" | "week" | "weeks" => 604_800_000.0,
        _ => return None,
    };
    Some((amount * milliseconds) as i64)
}

fn format_date(value: &NaiveDateTime, format: &str) -> String {
    let format = format.replace("YYYY", "%Y").replace("MM", "%m").replace("DD", "%d").replace("HH", "%H").replace("mm", "%M").replace("ss", "%S");
    value.format(&format).to_string()
}

fn normalize_tag(tag: &str) -> String {
    tag.trim().trim_start_matches('#').to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn record(id: u32, name: &str, folder: &str, status: &str, price: f64, tags: &[&str]) -> BaseRecord {
        BaseRecord {
            id: NoteId::new(id),
            path: if folder.is_empty() { format!("{name}.md") } else { format!("{folder}/{name}.md") },
            name: name.to_string(),
            extension: "md".to_string(),
            folder: folder.to_string(),
            size: 100,
            created: None,
            modified: None,
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            links: Vec::new(),
            properties: BTreeMap::from([("status".to_string(), Value::String(status.to_string())), ("price".to_string(), Value::Number(price)), ("quantity".to_string(), Value::Number(2.0))]),
        }
    }

    #[test]
    fn evaluates_filters_formulas_sort_group_limit_and_summaries() {
        let base = parse_base(
            r#"
filters:
  and:
    - file.hasTag("book")
    - status != "done"
formulas:
  total: price * quantity
properties:
  formula.total:
    displayName: Total price
views:
  - type: table
    name: Active
    order: [file.name, status, formula.total]
    sort:
      - property: formula.total
        direction: DESC
    groupBy:
      property: status
      direction: ASC
    summaries:
      formula.total: Sum
    limit: 2
"#,
        )
        .unwrap();
        let corpus = Corpus { records: vec![record(1, "Cheap", "Books", "reading", 3.0, &["book"]), record(2, "Done", "Books", "done", 100.0, &["book"]), record(3, "Expensive", "Books", "reading", 9.0, &["book"])] };
        let now = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap().and_hms_opt(12, 0, 0).unwrap();
        let result = CompiledBase::compile(base).evaluate_view(&corpus, 0, now);
        assert_eq!(result.matched_rows, 2);
        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].rows[0].path, "Books/Expensive.md");
        assert_eq!(result.groups[0].rows[0].cells[2], Value::Number(18.0));
        assert_eq!(result.summaries[2], Some(Value::Number(24.0)));
        assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
    }

    #[test]
    fn date_arithmetic_and_nested_properties_work() {
        let base = parse_base(
            r#"
filters: 'due < today() + "7d"'
views:
  - type: table
    order: [file.name, due]
"#,
        )
        .unwrap();
        let mut row = record(1, "Soon", "", "open", 1.0, &[]);
        row.properties.insert("due".to_string(), Value::Date(NaiveDate::from_ymd_opt(2026, 9, 3).unwrap().and_hms_opt(0, 0, 0).unwrap()));
        let corpus = Corpus { records: vec![row] };
        let now = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap().and_hms_opt(12, 0, 0).unwrap();
        let result = CompiledBase::compile(base).evaluate_view(&corpus, 0, now);
        assert_eq!(result.matched_rows, 1);
    }

    #[test]
    fn circular_formulas_settle_to_null_without_recursing_forever() {
        let base = parse_base(
            r#"
formulas:
  a: formula.b
  b: formula.a
views:
  - type: table
    order: [file.name, formula.a]
"#,
        )
        .unwrap();
        let result = CompiledBase::compile(base).evaluate_view(&Corpus { records: vec![record(1, "A", "", "open", 1.0, &[])] }, 0, NaiveDate::from_ymd_opt(2026, 8, 31).unwrap().and_hms_opt(0, 0, 0).unwrap());
        assert_eq!(result.groups[0].rows[0].cells[1], Value::Null);
    }

    #[test]
    fn missing_values_do_not_pass_ordering_filters() {
        let base = parse_base("filters: score > 5\nviews:\n  - type: table\n").unwrap();
        let mut with_score = record(1, "Scored", "", "open", 1.0, &[]);
        with_score.properties.insert("score".to_string(), Value::Number(7.0));
        let without_score = record(2, "Missing", "", "open", 1.0, &[]);
        let corpus = Corpus { records: vec![with_score, without_score] };
        let now = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap().and_hms_opt(0, 0, 0).unwrap();
        let result = CompiledBase::compile(base).evaluate_view(&corpus, 0, now);
        assert_eq!(result.matched_rows, 1);
        assert_eq!(result.groups[0].rows[0].path, "Scored.md");
    }

    #[test]
    fn hidden_properties_can_drive_sorting_and_grouping() {
        let base = parse_base(
            r#"
views:
  - type: table
    order: [file.name]
    sort:
      - property: price
        direction: DESC
    groupBy:
      property: status
      direction: ASC
"#,
        )
        .unwrap();
        let corpus = Corpus { records: vec![record(1, "Cheap", "", "reading", 3.0, &[]), record(2, "Done", "", "done", 100.0, &[]), record(3, "Expensive", "", "reading", 9.0, &[])] };
        let now = NaiveDate::from_ymd_opt(2026, 8, 31).unwrap().and_hms_opt(0, 0, 0).unwrap();
        let result = CompiledBase::compile(base).evaluate_view(&corpus, 0, now);

        assert_eq!(result.columns.len(), 1);
        assert_eq!(result.groups[0].label.as_deref(), Some("done"));
        assert_eq!(result.groups[1].label.as_deref(), Some("reading"));
        assert_eq!(result.groups[1].rows[0].path, "Expensive.md");
        assert_eq!(result.groups[1].rows[1].path, "Cheap.md");
    }
}
