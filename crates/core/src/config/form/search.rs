//! The Settings form's search: which inputs show for a query. Every word of
//! it must appear (any case) in an input's section title, label, help or
//! config key; the two tables match by their columns.

use super::{ACTIONS, ENDPOINTS, Field, Rows, SECTIONS, TABLES};

/// Every word of `query` in `hay`, any case; an empty query matches all.
fn all_words(hay: &str, query: &str) -> bool {
    let hay = hay.to_lowercase();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|w| hay.contains(w))
}

/// Whether input `f` of section `section` shows for `query` (and on this OS).
pub fn field_matches(f: &Field, section: &str, query: &str) -> bool {
    let hay = format!("{section} {} {} {}.{}", f.label, f.help, f.table, f.key);
    f.here() && all_words(&hay, query)
}

/// Whether the table of `rows` (Buttons, Servers) shows for `query`.
pub fn table_matches(rows: Rows, query: &str) -> bool {
    let (title, columns) = match rows {
        Rows::Actions => (TABLES[0], ACTIONS),
        Rows::Endpoints => (TABLES[1], ENDPOINTS),
    };
    let names = columns.iter().map(|c| format!("{} {}", c.label, c.key));
    let hay = format!(
        "{title} {} {}",
        rows.key(),
        names.collect::<Vec<_>>().join(" ")
    );
    all_words(&hay, query)
}

/// The form's cards that show for `query`, in its order: the two tables,
/// then each of SECTIONS with any input that matches.
pub fn cards(query: &str) -> Vec<bool> {
    let mut shown = vec![
        table_matches(Rows::Actions, query),
        table_matches(Rows::Endpoints, query),
    ];
    shown.extend(
        SECTIONS
            .iter()
            .map(|s| s.fields.iter().any(|f| field_matches(f, s.title, query))),
    );
    shown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_keeps_the_inputs_that_have_all_its_words() {
        assert!(cards("").iter().all(|&s| s), "nothing typed: everything");
        let hotkeys = SECTIONS.iter().find(|s| s.title == "Hotkeys").unwrap();
        let shown: Vec<&str> = (hotkeys.fields.iter())
            .filter(|f| field_matches(f, hotkeys.title, "quick TRANSLATE"))
            .map(|f| f.key)
            .collect();
        assert_eq!(shown, ["translate"], "any case, every word");
        let all = hotkeys
            .fields
            .iter()
            .filter(|f| field_matches(f, "Hotkeys", "hotkeys"));
        assert_eq!(
            all.count(),
            hotkeys.fields.len(),
            "the section's title: all of it"
        );
        assert!(table_matches(Rows::Endpoints, "url"), "a server's column");
        assert!(table_matches(Rows::Actions, "prompt"));
        assert!(!cards("zzz-nothing").iter().any(|&s| s));
    }
}
