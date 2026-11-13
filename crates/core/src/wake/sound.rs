//! How a word sounds and how far two words are apart: the matching under
//! the wake text rules.

/// A word's rough sound: its consonants, voiced and voiceless alike (b/p,
/// d/t, g/k, z/s, v/w/f), j read as y, no vowels, no doubles.
pub(super) fn sound(word: &str) -> String {
    let mut out = String::new();
    let latin: String = word
        .chars()
        .flat_map(char::to_lowercase)
        .map(latin)
        .collect();
    // a German "th" is an s: "that" said as "set"
    let latin = latin.replace("th", "s");
    for c in latin.chars() {
        let c = match c {
            'b' => 'p',
            'd' => 't',
            'g' | 'c' | 'q' => 'k',
            'z' => 's',
            'v' | 'w' => 'f',
            'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'j' | 'ä' | 'ö' | 'ü' => continue,
            c if c.is_alphabetic() => c,
            _ => continue,
        };
        if !out.ends_with(c) {
            out.push(c);
        }
    }
    out
}

/// A Cyrillic letter in Latin ones, else the letter: the speech model wrote
/// a German speaker's "hey blitz" as "Хей, плют".
fn latin(c: char) -> String {
    const FROM: &str = "абвгдеёжзийклмнопрстуфхцчшщъыьэюя";
    const TO: [&str; 33] = [
        "a", "b", "v", "g", "d", "e", "e", "zh", "z", "i", "y", "k", "l", "m", "n", "o", "p", "r",
        "s", "t", "u", "f", "h", "ts", "ch", "sh", "sch", "", "y", "", "e", "yu", "ya",
    ];
    match FROM.chars().position(|f| f == c) {
        Some(i) => TO[i].to_string(),
        None => c.to_string(),
    }
}

/// Whether `said` is a command's later word `want` as an accent leaves it:
/// one sound and one letter off at most, "th" read as s, a last t as d, a
/// leading vowel dropped. "Ascend", "Ascent", "sind" pass for "send"; "ist",
/// "end", "Ende", "wind" do not (but see `command`'s "Blitz end."): "Blitz ist Qursch" once pressed Return.
pub(super) fn heard_as(said: &str, want: &str) -> bool {
    let spell = |w: &str| {
        let s = letters(w).replace("th", "s");
        match s.strip_suffix('t') {
            Some(head) => format!("{head}d"),
            None => s,
        }
    };
    let (a, b) = (spell(said), spell(want));
    let bare = a.trim_start_matches(['a', 'e', 'i', 'o', 'u']);
    // and the same first letter: "end", "hat", "pad" are not "send", "that"
    let near = |x: &str| x.chars().next() == b.chars().next() && distance(x, &b) <= 1;
    let spelt = near(&a) || near(bare);
    spelt && distance(&sound(said), &sound(want)) <= 1
}

pub(super) fn letters(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Edit distance in characters.
pub(super) fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let next = (row[j + 1] + 1)
                .min(row[j] + 1)
                .min(prev + usize::from(ca != *cb));
            (prev, row[j + 1]) = (row[j + 1], next);
        }
    }
    row[b.len()]
}
