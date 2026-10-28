//! The Settings form: one value of config.toml read or changed in place,
//! the file's comments and layout kept (toml_edit), every change checked as
//! Save checks it. The form and the text are two views of one text.

mod schema;
mod search;

pub use schema::{ACTIONS, ENDPOINTS, SECTIONS, TABLES};
pub use search::{cards, field_matches, table_matches};
use toml_edit::{Array, ArrayOfTables, DocumentMut, Item, Table, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Int,
    Float,
    /// One line of text.
    Text,
    /// Text with lines: a prompt.
    Long,
    /// Hotkey names, comma-separated.
    Keys,
    /// Strings, comma-separated.
    List,
    /// One of these strings; "auto" = the key left out.
    Choice(&'static [&'static str]),
    /// `spoken = written`, one per line.
    Map,
    /// `true` on at most one row of a table: [`pick`] sets it.
    Pick,
    /// A whole number from 0 to 100: a slider.
    Percent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub table: &'static str,
    pub key: &'static str,
    pub label: &'static str,
    pub help: &'static str,
    pub kind: Kind,
    /// What an empty input shows: a value to copy the form of.
    pub example: &'static str,
    /// Folded away under the section's Advanced until asked for.
    pub advanced: bool,
    /// Shown only on this OS ("macos", "linux"); empty = both.
    pub os: &'static str,
}

impl Field {
    pub const fn example(self, example: &'static str) -> Self {
        Field { example, ..self }
    }

    pub const fn advanced(self) -> Self {
        Field {
            advanced: true,
            ..self
        }
    }

    pub const fn only_on(self, os: &'static str) -> Self {
        Field { os, ..self }
    }

    /// Whether the form shows it on this OS.
    pub fn here(&self) -> bool {
        self.os.is_empty() || self.os == std::env::consts::OS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Section {
    pub title: &'static str,
    /// One line under the page's title: what the section is for.
    pub about: &'static str,
    pub fields: &'static [Field],
}

/// A column of an array of tables ([[actions]], [[endpoints]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Column {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: Kind,
}

/// The arrays of tables the form shows as tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rows {
    Actions,
    Endpoints,
}

impl Rows {
    fn key(self) -> &'static str {
        match self {
            Rows::Actions => "actions",
            Rows::Endpoints => "endpoints",
        }
    }

    pub fn columns(self) -> &'static [Column] {
        match self {
            Rows::Actions => ACTIONS,
            Rows::Endpoints => ENDPOINTS,
        }
    }
}

fn doc(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>()
        .map_err(|e| format!("config.toml does not parse: {}", e.message()))
}

/// `text` if it is a config Save would take.
fn checked(doc: &DocumentMut) -> Result<String, String> {
    let text = doc.to_string();
    super::Config::parse(&text).map_err(|e| e.detailed())?;
    Ok(text)
}

fn show(v: &Value, kind: Kind) -> String {
    match v {
        Value::String(s) => s.value().clone(),
        Value::Boolean(b) if matches!(kind, Kind::Choice(_)) => {
            (if *b.value() { "yes" } else { "no" }).into()
        }
        Value::Boolean(b) => b.value().to_string(),
        Value::Integer(i) => i.value().to_string(),
        Value::Float(x) => x.value().to_string(),
        Value::Array(a) => (a.iter())
            .map(|v| show(v, Kind::Text))
            .collect::<Vec<_>>()
            .join(", "),
        Value::InlineTable(t) => (t.iter())
            .map(|(k, v)| format!("{k} = {}", show(v, Kind::Text)))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Datetime(d) => d.value().to_string(),
    }
}

fn show_item(item: Option<&Item>, kind: Kind) -> Option<String> {
    match item? {
        Item::Value(v) => Some(show(v, kind)),
        Item::Table(t) => Some(
            (t.iter())
                .filter_map(|(k, v)| Some(format!("{k} = {}", show(v.as_value()?, Kind::Text))))
                .collect::<Vec<_>>()
                .join("\n"),
        ),
        _ => None,
    }
}

/// What `field` holds in `text`, or the built-in default where it is left out.
pub fn value(text: &str, field: &Field) -> String {
    let get = |text: &str| {
        let doc = text.parse::<DocumentMut>().ok()?;
        show_item(doc.get(field.table)?.get(field.key), field.kind)
    };
    get(text)
        .or_else(|| get(super::DEFAULT_TOML))
        .unwrap_or_else(|| empty(field.kind))
}

fn strings(input: &str, sep: char) -> Value {
    let items = (input.split(sep).map(str::trim)).filter(|s| !s.is_empty());
    Value::Array(items.collect::<Array>())
}

/// A string as it reads best in the file: lines as a literal block.
fn string(s: &str) -> Value {
    if s.contains('\n')
        && !s.contains("'''")
        && let Ok(v) = format!("'''\n{s}'''").parse::<Value>()
    {
        return v;
    }
    Value::from(s)
}

/// `input` as the value `kind` wants; None = leave the key out.
fn parse(input: &str, kind: Kind) -> Result<Option<Value>, String> {
    let t = input.trim();
    Ok(Some(match kind {
        Kind::Bool => Value::from(t == "true"),
        Kind::Pick if t == "true" => Value::from(true),
        Kind::Pick => return Ok(None),
        Kind::Int | Kind::Percent => Value::from(
            t.parse::<i64>()
                .map_err(|_| format!("{t:?} is not a whole number"))?,
        ),
        Kind::Float => Value::from(
            t.parse::<f64>()
                .map_err(|_| format!("{t:?} is not a number"))?,
        ),
        Kind::Text => Value::from(input),
        Kind::Long => string(input),
        Kind::Keys | Kind::List => strings(input, ','),
        Kind::Choice(_) => match t {
            "auto" => return Ok(None),
            "yes" => Value::from(true),
            "no" => Value::from(false),
            other => Value::from(other),
        },
        Kind::Map => {
            let mut map = toml_edit::InlineTable::new();
            for line in input.lines().filter(|l| !l.trim().is_empty()) {
                let (k, v) = line
                    .split_once('=')
                    .ok_or_else(|| format!("{line:?}: write spoken = written"))?;
                map.insert(k.trim(), Value::from(v.trim()));
            }
            Value::InlineTable(map)
        }
    }))
}

/// Put `new` at `key` in `t` in the old value's place: the comment lines
/// above the key (`insert` would clear them) and the one beside it stay.
fn put(t: &mut dyn toml_edit::TableLike, key: &str, new: Option<Value>) {
    let Some(new) = new else {
        t.remove(key);
        return;
    };
    match (t.get_mut(key), new) {
        // a [table.key] of its own stays one, with its lines' comments
        (Some(Item::Table(old)), Value::InlineTable(map)) => {
            old.retain(|k, _| map.contains_key(k));
            for (k, v) in map.iter() {
                put(old, k, Some(v.clone()));
            }
        }
        (Some(old), mut new) => {
            if let Some(v) = old.as_value() {
                *new.decor_mut() = v.decor().clone();
            }
            *old = Item::Value(new);
        }
        (None, mut new) => {
            new.decor_mut().clear();
            t.insert(key, Item::Value(new));
        }
    }
}

/// The form's field for `table.key`.
pub fn field(table: &str, key: &str) -> Option<&'static Field> {
    (schema::SECTIONS.iter())
        .flat_map(|s| s.fields)
        .find(|f| f.table == table && f.key == key)
}

/// `text` with `field` set to `input`, if Save would take it.
pub fn set(text: &str, field: &Field, input: &str) -> Result<String, String> {
    let mut doc = doc(text)?;
    let new = parse(input, field.kind)?;
    let table = doc
        .entry(field.table)
        .or_insert_with(|| Item::Table(Table::new()))
        .as_table_like_mut()
        .ok_or_else(|| format!("[{}] is not a table", field.table))?;
    put(table, field.key, new);
    checked(&doc)
}

/// The rows of `rows`, each cell as the form shows it; no [[actions]] in
/// the file = the built-in Rewrite.
pub fn rows(text: &str, rows: Rows) -> Vec<Vec<String>> {
    let get = |text: &str| -> Option<Vec<Vec<String>>> {
        let doc = text.parse::<DocumentMut>().ok()?;
        let arr = doc.get(rows.key())?.as_array_of_tables()?;
        Some(
            arr.iter()
                .map(|t| {
                    (rows.columns().iter())
                        .map(|c| show_item(t.get(c.key), c.kind).unwrap_or_else(|| empty(c.kind)))
                        .collect()
                })
                .collect(),
        )
    };
    match (get(text), rows) {
        (Some(r), _) => r,
        (None, Rows::Actions) => get(super::DEFAULT_TOML).unwrap_or_default(),
        (None, Rows::Endpoints) => vec![],
    }
}

fn empty(kind: Kind) -> String {
    match kind {
        Kind::Choice(c) => c[0].to_string(),
        _ => String::new(),
    }
}

/// The array of tables `rows` in `doc`, made (with the built-in buttons,
/// for [[actions]]) where the file has none.
fn array(d: &mut DocumentMut, rows: Rows) -> Result<&mut ArrayOfTables, String> {
    if d.get(rows.key()).is_none() {
        let mut arr = ArrayOfTables::new();
        if rows == Rows::Actions {
            let defaults = doc(super::DEFAULT_TOML)?;
            if let Some(d) = defaults.get("actions").and_then(Item::as_array_of_tables) {
                arr = d.clone();
            }
        }
        d.insert(rows.key(), Item::ArrayOfTables(arr));
    }
    d.get_mut(rows.key())
        .and_then(Item::as_array_of_tables_mut)
        .ok_or_else(|| format!("{} is not a list of [[{}]] tables", rows.key(), rows.key()))
}

/// `text` with row `i`'s `col` set to `input`, if Save would take it.
pub fn set_cell(
    text: &str,
    rows: Rows,
    i: usize,
    col: &Column,
    input: &str,
) -> Result<String, String> {
    let mut doc = doc(text)?;
    let new = parse(input, col.kind)?;
    let row = array(&mut doc, rows)?
        .get_mut(i)
        .ok_or_else(|| format!("row {} is gone", i + 1))?;
    put(row, col.key, new);
    checked(&doc)
}

/// `text` with the [`Kind::Pick`] column `col` true on row `i` alone, or on
/// none (Return starts a chat, or sends in one).
pub fn pick(text: &str, rows: Rows, col: &Column, i: Option<usize>) -> Result<String, String> {
    let mut doc = doc(text)?;
    for (n, row) in array(&mut doc, rows)?.iter_mut().enumerate() {
        let on = (Some(n) == i).then(|| Value::from(true));
        put(row, col.key, on);
    }
    checked(&doc)
}

/// `text` with one more row at the end, filled so Save takes it: a server
/// named apart from the others.
pub fn add_row(text: &str, rows: Rows) -> Result<String, String> {
    let mut doc = doc(text)?;
    let arr = array(&mut doc, rows)?;
    let taken = |n: &str| {
        arr.iter()
            .any(|t| t.get("name").and_then(Item::as_str) == Some(n))
    };
    let name = (1..)
        .map(|n| match n {
            1 => "New server".to_string(),
            _ => format!("New server {n}"),
        })
        .find(|n| !taken(n))
        .unwrap_or_default();
    let fill: &[(&str, &str)] = match rows {
        Rows::Actions => &[
            ("label", "Summarize"),
            (
                "prompt",
                "Summarize the text below in one sentence.\n\nTEXT:\n{text}",
            ),
        ],
        Rows::Endpoints => &[
            ("name", &name),
            ("url", "http://127.0.0.1:8080/v1"),
            ("key", ""),
        ],
    };
    let mut t = Table::new();
    for (k, v) in fill {
        t.insert(k, Item::Value(string(v)));
    }
    arr.push(t);
    checked(&doc)
}

/// `text` without row `i`, if Save would take it (one button must stay).
pub fn remove_row(text: &str, rows: Rows, i: usize) -> Result<String, String> {
    let mut doc = doc(text)?;
    let arr = array(&mut doc, rows)?;
    if i >= arr.len() {
        return Err(format!("row {} is gone", i + 1));
    }
    // an empty list is no [[actions]]: the built-in Rewrite would come back unasked
    if rows == Rows::Actions && arr.len() == 1 {
        return Err("keep at least one button: the first is what Return runs".into());
    }
    arr.remove(i);
    checked(&doc)
}

#[cfg(test)]
mod tests;
