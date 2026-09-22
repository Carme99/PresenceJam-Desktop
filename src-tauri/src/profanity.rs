const PROFANITY_LIST: &[&str] = &[
    "fuck", "shit", "damn", "bitch", "bastard", "crap", "piss", "dick", "cock", "pussy", "cunt",
    "whore", "slut", "fag", "nigger", "nigga", "retard", "spic", "chink", "kike", "dyke", "tard",
    "faggot", "douche", "asshole", "tits", "twat",
];

const SAFE_PLACEHOLDER_DEFAULT: &str = "Currently Listening to Spotify";

pub fn safe_placeholder_default() -> &'static str {
    SAFE_PLACEHOLDER_DEFAULT
}

/// A normalized character plus whether it arrived via a lossy fold
/// (leet glyph, confusable decomposition, fullwidth fold). Lossy-origin
/// chars are skippable inside a match (#337: the `1` in `fuu1uck`).
/// `via_x_fold` marks the `c`/`k` pair from the scoped `x`-for-`ck`
/// fold (#827): a `dick` fabricated from `Dix` needs a glued profane
/// continuation to flag.
#[derive(Clone, Copy)]
struct NormChar {
    ch: char,
    leet: bool,
    via_x_fold: bool,
}

fn collapse_repeated_chars(text: &[NormChar]) -> Vec<NormChar> {
    let mut result = Vec::with_capacity(text.len());
    let mut prev = char::MAX;
    let mut count = 0u32;

    for &nc in text {
        if nc.ch != prev {
            count = 1;
            result.push(nc);
            prev = nc.ch;
        } else {
            count += 1;
            if count <= 2 {
                result.push(nc);
            }
        }
    }

    result
}

/// Combining marks (Mn): strip after lowercasing so e.g. U+0130
/// (`İ` → `i` + U+0307) folds to plain ASCII.
fn is_combining_mark(c: char) -> bool {
    matches!(
        c as u32,
        0x0300..=0x036F
            | 0x1AB0..=0x1AFF
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20FF
            | 0xFE20..=0xFE2F
    )
}

/// Format chars (Cf), including the zero-width space family.
fn is_format_char(c: char) -> bool {
    matches!(
        c as u32,
        0x00AD
            | 0x061C
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180E
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFF0..=0xFFF8
            | 0xE0000..=0xE0FFF
    )
}

/// Narrow hand-rolled NFKD-ish fold for precomposed Latin letters
/// (`ü` → `u`, `ß` → `ss`). Covers what mark-stripping alone cannot.
/// Zero-dependency tradeoff is explicit: Greek/Cyrillic confusables
/// are out of scope.
fn strip_diacritic(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' | 'ǎ' | 'ȁ' | 'ȃ' | 'ạ' | 'ả' | 'ấ'
        | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' | 'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' => "a",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' | 'ȅ' | 'ȇ' | 'ẹ' | 'ẻ' | 'ẽ' | 'ế'
        | 'ề' | 'ể' | 'ễ' | 'ệ' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ǐ' | 'ȉ' | 'ȋ' | 'ị' | 'ỉ' => {
            "i"
        }
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' | 'ǒ' | 'ȍ' | 'ȏ' | 'ọ' | 'ỏ' | 'ố'
        | 'ồ' | 'ổ' | 'ỗ' | 'ộ' | 'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' => "o",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' | 'ǔ' | 'ȕ' | 'ȗ' | 'ụ' | 'ủ' => {
            "u"
        }
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => "s",
        'ź' | 'ż' | 'ž' => "z",
        'ď' | 'đ' | 'ð' => "d",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
        'ŵ' => "w",
        'ß' => "ss",
        'æ' => "ae",
        'œ' => "oe",
        'þ' => "th",
        'ŋ' => "n",
        _ => return None,
    })
}

/// Leet map. `6` folds to `b` (covers `6itch`); `2` folds to `i`
/// (covers `sh2t`; no list word contains `z`, so `2` -> `z` was dead weight).
/// `z` folds to `s` (covers `niggaz`/`bitchez` plurals; still no `z` in the list).
/// `(` is deliberately absent: it folds to `c` only next to a stem
/// onset, which needs lookahead (`normalize`, #578).
fn leet_fold(c: char) -> (char, bool) {
    match c {
        '1' | '!' | '|' => ('i', true),
        '3' => ('e', true),
        '$' | '5' => ('s', true),
        '@' | '4' => ('a', true),
        '0' => ('o', true),
        '7' => ('t', true),
        '2' => ('i', true),
        '6' | '8' => ('b', true),
        '9' => ('g', true),
        '+' => ('t', true),
        'z' => ('s', true),
        _ => (c, false),
    }
}

/// Stem onsets an `x`-for-`ck` spelling can be hiding (#377): `fux`,
/// `shix`, `bix`, `bax`, `dix`, `pix`, `cux`, `nix`, `pux`. Applied
/// unconditionally the fold also rewrote innocent names — `Cox`
/// normalized to `cock` — so it only fires after one of these (#578).
fn x_reads_as_ck(prev: &[NormChar]) -> bool {
    const ONSETS: [&str; 9] = ["fu", "shi", "bi", "ba", "di", "pi", "cu", "ni", "pu"];
    let take = prev.len().min(3);
    let recent: String = prev[prev.len() - take..].iter().map(|n| n.ch).collect();
    ONSETS.iter().any(|onset| recent.ends_with(onset))
}

/// The stems a leading `(` was folded to `c` for (#377): `(ock` (`cock`),
/// `(unt` (`cunt`), `(rap` (`crap`), `(um` (`cum`). Applied unconditionally
/// the fold also prefixed innocent parentheses — `Song (Uncut)`
/// normalized to `cuncut` — so it only fires when the text actually
/// spells one of these (#578). The lookahead runs through the single-char
/// leet map so `(0ck` counts too.
fn paren_reads_as_c(rest: impl Iterator<Item = char>) -> bool {
    const STEMS: [&str; 4] = ["ock", "unt", "rap", "um"];
    let look: String = rest.take(3).map(|c| leet_fold(c).0).collect();
    STEMS.iter().any(|stem| look.starts_with(stem))
}

fn normalize(text: &str) -> Vec<NormChar> {
    let mut result = Vec::with_capacity(text.len());
    let lowered = text.to_lowercase();
    let mut chars = lowered.chars().peekable();

    while let Some(c) = chars.next() {
        // Multi-char leet: `\/` reads as `v`.
        if c == '\\' && chars.peek() == Some(&'/') {
            chars.next();
            result.push(NormChar {
                ch: 'v',
                leet: true,
                via_x_fold: false,
            });
            continue;
        }
        // Multi-char digraph: `ph` reads as `f` (#377: `phuck`).
        // Runs on the lowered text, mirroring the `\/` fold above.
        if c == 'p' && chars.peek() == Some(&'h') {
            chars.next();
            result.push(NormChar {
                ch: 'f',
                leet: true,
                via_x_fold: false,
            });
            continue;
        }
        if is_format_char(c) || is_combining_mark(c) {
            continue;
        }
        // Fullwidth U+FF00 block folds to ASCII by a fixed offset.
        let mut folded = c;
        if ('\u{FF01}'..='\u{FF5E}').contains(&c) {
            if let Some(ascii) = char::from_u32(c as u32 - 0xFEE0) {
                folded = ascii;
            }
        }
        // Multi-char leet: `x` reads as `ck` (#377: `fux`), but only after
        // a stem onset — applied globally it rewrote innocent names
        // (`Cox` reads as `co` + `ck`) (#578). Placed after the fullwidth
        // fold so fullwidth `ｘ` expands too.
        if folded == 'x' && x_reads_as_ck(&result) {
            result.push(NormChar {
                ch: 'c',
                leet: true,
                via_x_fold: true,
            });
            result.push(NormChar {
                ch: 'k',
                leet: true,
                via_x_fold: true,
            });
            continue;
        }
        // `(` reads as `c` only when it spells one of the stems it was
        // added for (#377: `(ock`); applied globally it prefixed innocent
        // parentheses with a `c` (#578: `Song (Uncut)`). Outside those
        // stems it stays a literal, non-alphanumeric separator.
        if folded == '(' {
            if paren_reads_as_c(chars.clone()) {
                result.push(NormChar {
                    ch: 'c',
                    leet: true,
                    via_x_fold: false,
                });
            } else {
                result.push(NormChar {
                    ch: '(',
                    leet: false,
                    via_x_fold: false,
                });
            }
            continue;
        }
        let folded_is_lossy = folded != c;
        if let Some(base) = strip_diacritic(folded) {
            for b in base.chars() {
                let (mapped, _) = leet_fold(b);
                result.push(NormChar {
                    ch: mapped,
                    leet: true,
                    via_x_fold: false,
                });
            }
            continue;
        }
        let (mapped, leet) = leet_fold(folded);
        // Dropped-`c` evasion: `uk` reads as `uck` (#377: `fuk`).
        // Scoped to `u` so innocent `k` words (`like`, `book`) are untouched.
        if mapped == 'k' && result.last().is_some_and(|n| n.ch == 'u') {
            result.push(NormChar {
                ch: 'c',
                leet: true,
                via_x_fold: false,
            });
        }
        result.push(NormChar {
            ch: mapped,
            leet: leet || folded_is_lossy,
            via_x_fold: false,
        });
    }

    collapse_repeated_chars(&result)
}

/// Recursive matcher with backtracking. At a mismatch the matcher may:
/// - skip a separator (insertion reading, e.g. `f.u.c.k`), or consume it
///   as a single-char wildcard (substitution reading, e.g. `f*ck`);
/// - skip a stretched char: duplicate of the previous char, leet-origin
///   char (#337), or a repeat of an already-matched word char.
///
/// Separator skips are formatting, not stretching; every other skip sets
/// `stretched`, which callers must gate on a right-side word boundary
/// (#332: `shiitake` must stay clean). Any separator skip (or wildcard
/// consumption) also sets `sep_skipped`, which callers must gate on
/// original-string word boundaries on BOTH sides (`Push It` joins to
/// `pushit`, which fabricates `shit`).
fn match_from(
    text: &[NormChar],
    word: &[char],
    si: usize,
    wi: usize,
    stretched: bool,
    sep_skipped: bool,
) -> Option<(usize, bool, bool)> {
    if wi == word.len() {
        return Some((si, stretched, sep_skipped));
    }
    if si == text.len() {
        return None;
    }
    let t = text[si];
    if t.ch == word[wi] {
        return match_from(text, word, si + 1, wi + 1, stretched, sep_skipped);
    }
    if !t.ch.is_alphanumeric() {
        if let Some(found) = match_from(text, word, si + 1, wi, stretched, true) {
            return Some(found);
        }
        if let Some((end, _, _)) = match_from(text, word, si + 1, wi + 1, true, true) {
            return Some((end, true, true));
        }
        return None;
    }
    if si > 0 && t.ch == text[si - 1].ch {
        if let Some(found) = match_from(text, word, si + 1, wi, true, sep_skipped) {
            return Some(found);
        }
    }
    if t.leet {
        if let Some(found) = match_from(text, word, si + 1, wi, true, sep_skipped) {
            return Some(found);
        }
    }
    if word[..wi].contains(&t.ch) {
        if let Some(found) = match_from(text, word, si + 1, wi, true, sep_skipped) {
            return Some(found);
        }
    }
    None
}

fn matches_at_pos(text: &[NormChar], word: &[char], start: usize) -> Option<(usize, bool, bool)> {
    match_from(text, word, start, 0, false, false)
}

/// Whitelist scoped per stem: only `cock` + `tail` is a known-clean
/// compound. `head`/`hand`/etc. after any stem was immunizing real
/// insults (#330: `dickhead`).
fn is_clean_compound(stem: &str, token: &str) -> bool {
    stem == "cock" && (token == "tail" || token == "tails")
}

/// Continuations that extend a stem into profanity rather than a new
/// word: whole inflections (`s`/`es`/`ed`/`ing`/`er`/`ers`/`ings`) and
/// the glued compounds modern titles use (`head`/`boy`/`face`/`wad`/
/// `post` — #579: `fuckboy`, `fuckface`, `shitposting`). Plain tails
/// require a whole inflection: a mere prefix (`eria` for `er`) is a
/// distinct clean word (#812: `Pizzeria`), while whole-inflection
/// collisions on innocent words (`Spices`, `Spiced`, `crapes`) are
/// carved per stem below. Compound tails keep the prefix form, where
/// `headed`/`posting` really are the inflected forms. Anything else
/// (`pit`, `ake`) is a distinct clean word (#328).
fn is_profane_continuation(stem: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    // Compound tails keep the prefix form (`headed`, `posting`).
    if ["head", "boy", "face", "wad", "post"]
        .iter()
        .any(|p| token.starts_with(p))
    {
        return true;
    }
    // Plain tails require a whole inflection.
    if !matches!(token, "s" | "es" | "ed" | "ing" | "er" | "ers" | "ings") {
        return false;
    }
    // Per-stem carve-outs (#812), mirroring how `is_y_tail` scopes `y`:
    // `spic` rejects `es`/`ed` (`Spices`/`Spiced`, while `spics` still
    // flags), `crap` rejects `es` (`crapes`, while `craps` still flags),
    // and `piss` rejects `er` (defence in depth: the `z`-to-`s`
    // fold reads `Pizzeria` as `pisseria`, whose remainder `eria` is
    // already not a whole inflection).
    if stem == "spic" && matches!(token, "es" | "ed") {
        return false;
    }
    if stem == "crap" && token == "es" {
        return false;
    }
    if stem == "piss" && token == "er" {
        return false;
    }
    true
}

/// `y`-tail scoped per stem: `shitty`/`bitchy`/`fucky` flag, while
/// `cocky` (cock), `spicy` (spic) and `tardy` (tard) stay clean.
/// `shitty` doubles the `t`, so its remainder reads `ty`.
fn is_y_tail(stem: &str, token: &str) -> bool {
    matches!(stem, "shit" | "bitch" | "fuck") && (token == "y" || token == "ty")
}

/// Stems unambiguous enough that a clean right edge suffices even when
/// the left side is glued (#331: `bullshit`). Deliberately narrow:
/// `tard` must stay out (`mustard`), `cock` must stay out (`peacock`).
fn is_strong_stem(word: &str) -> bool {
    matches!(word, "shit" | "fuck" | "bitch")
}

/// First alphanumeric token at `idx`, skipping separators.
fn first_token(chars: &[NormChar], mut idx: usize) -> String {
    while idx < chars.len() && !chars[idx].ch.is_alphanumeric() {
        idx += 1;
    }
    let mut token = String::new();
    while idx < chars.len() && chars[idx].ch.is_alphanumeric() {
        token.push(chars[idx].ch);
        idx += 1;
    }
    token
}

/// Alphanumeric run starting AT `idx`, with no separator skipping (#827:
/// `Dix's` has `'` at `end`, so its glued remainder is empty and stays
/// clean, while `dixs` flags on the glued `s`).
fn glued_token(chars: &[NormChar], idx: usize) -> String {
    let mut token = String::new();
    let mut i = idx;
    while i < chars.len() && chars[i].ch.is_alphanumeric() {
        token.push(chars[i].ch);
        i += 1;
    }
    token
}

fn contains_profanity(text: &str, extra_words: &[String]) -> bool {
    let chars = normalize(text);

    // Issue #538 / CfgDiag#3(b): the user's own lexicon is matched against the
    // SAME normalized text with the same evasion machinery (separator skipping,
    // leet folding, stretch collapsing, span checks) — only the stem-scoped
    // carve-outs below are built-in-only, because they are empirically tuned for
    // the built-in stems (`not` + `ing` must not flag `noting`).
    if contains_extra_word(&chars, extra_words) {
        return true;
    }

    for &word in PROFANITY_LIST {
        let word_chars: Vec<char> = word.chars().collect();
        let word_len = word_chars.len();

        if word_len > chars.len() {
            continue;
        }

        for start in 0..=(chars.len() - word_len) {
            let Some((end, stretched, sep_skipped)) = matches_at_pos(&chars, &word_chars, start)
            else {
                continue;
            };

            let right_clean = end >= chars.len() || !chars[end].ch.is_alphanumeric();
            if stretched && !right_clean {
                continue;
            }

            // #827: the `di` onset of the scoped `x`-for-`ck` fold fabricates
            // `dick` out of the surname `Dix`. Such a match only flags on a
            // glued profane continuation (`dixs`, `dixhead`); `Dix`, `Dix's`
            // (separator at `end`, so the glued token is empty) and `Dixon`
            // stay clean. Scoped to the `di` onset — the other eight keep
            // their current coverage (`fux` still reads as `fuck`).
            if word == "dick" {
                if let Some(rel) = chars[start..end].iter().position(|n| n.via_x_fold) {
                    let fold_at = start + rel;
                    let di_fold = fold_at >= 2
                        && chars[fold_at - 2].ch == 'd'
                        && chars[fold_at - 1].ch == 'i';
                    // Veto only: a non-profane remainder stays clean, while a
                    // profane one falls through to the usual boundary checks
                    // below (so glued-left `adixs` keeps its verdict).
                    if di_fold && !is_profane_continuation(word, &glued_token(&chars, end)) {
                        continue;
                    }
                }
            }

            // Separator-spanning matches join across formatting (`Push It`
            // reads `pushit`, which fabricates `shit`): require
            // original-string word boundaries on BOTH sides. Standalone
            // evasions (`f u c k`, `s.h.i.t`, `f*ck`) satisfy this;
            // mid-word fabrications (`Push It`) do not. Matches spanning
            // no separator keep the glued strong-stem rule below.
            if sep_skipped {
                let left_boundary = start == 0 || !chars[start - 1].ch.is_alphanumeric();
                // A separator-spanning match may substitute or skip
                // formatting, but it must not ALSO swallow alphabetic
                // characters: `Song (Uncut)` reads a leading `c` out of the
                // parenthesis and then drops the real `cu` of `uncut`,
                // fabricating `cunt` (#578). The span covers every char the
                // matcher consumed, so more alphabetic chars in it than the
                // word has means at least one was skipped.
                let alnum_in_span = chars[start..end]
                    .iter()
                    .filter(|n| n.ch.is_alphanumeric())
                    .count();
                if alnum_in_span > word_len {
                    continue;
                }
                if !(left_boundary && right_clean) {
                    continue;
                }
                return true;
            }

            // Legacy fuck-derivation carve-out (`motherfucker`):
            // position-independent, predates the boundary rework.
            if word == "fuck" && !right_clean {
                let token = first_token(&chars, end);
                if ["ing", "er", "ed"].iter().any(|p| token.starts_with(p)) {
                    return true;
                }
            }

            if is_strong_stem(word) && right_clean {
                return true;
            }

            if end >= chars.len() {
                let char_before_ok = start == 0 || !chars[start - 1].ch.is_alphanumeric();
                if char_before_ok {
                    return true;
                }
                continue;
            }

            if start == 0 {
                if right_clean {
                    return true;
                }
                let token = first_token(&chars, end);
                if is_clean_compound(word, &token) {
                    continue;
                }
                if is_y_tail(word, &token) {
                    return true;
                }
                if is_profane_continuation(word, &token) {
                    return true;
                }
                continue;
            }

            if chars[start - 1].ch.is_alphanumeric() {
                continue;
            }
            if right_clean {
                return true;
            }
            let token = first_token(&chars, end);
            if is_clean_compound(word, &token) {
                continue;
            }
            if is_y_tail(word, &token) {
                return true;
            }
            if is_profane_continuation(word, &token) {
                return true;
            }
        }
    }

    false
}

/// Issue #538 / CfgDiag#3(b): match the user's lexicon against the already
/// normalized text.
///
/// A user entry is an arbitrary word or phrase, so it is matched with the
/// BUILT-IN boundary and evasion rules — separators may be skipped or read as a
/// single-char wildcard, leet/stretch evasions fold the same way, a
/// separator-spanning match must sit on original-string word boundaries on both
/// sides, and an unchanged match must have clean edges — while the stem-scoped
/// carve-outs (`is_clean_compound` / `is_y_tail` / `is_profane_continuation` /
/// `is_strong_stem`) stay built-in-only: they encode which CONTINUATIONS of a
/// given stem are profane (`shitpost`, `fuckboy`) and would flag the innocent
/// inflections of an arbitrary user word (`not` + `ing`).
fn contains_extra_word(text: &[NormChar], extra_words: &[String]) -> bool {
    for raw in extra_words {
        let word_chars = extra_word_chars(raw);
        let word_len = word_chars.len();
        if word_len == 0 || word_len > text.len() {
            continue;
        }

        for start in 0..=(text.len() - word_len) {
            let Some((end, stretched, sep_skipped)) = matches_at_pos(text, &word_chars, start)
            else {
                continue;
            };

            let right_clean = end >= text.len() || !text[end].ch.is_alphanumeric();
            if stretched && !right_clean {
                continue;
            }
            let left_clean = start == 0 || !text[start - 1].ch.is_alphanumeric();

            if sep_skipped {
                // Same span rule as the built-in list: a separator-spanning
                // match may substitute or skip formatting, but must not also
                // swallow alphabetic characters (`Song (Uncut)` fabricating a
                // word out of a parenthetical).
                let alnum_in_span = text[start..end]
                    .iter()
                    .filter(|n| n.ch.is_alphanumeric())
                    .count();
                if alnum_in_span > word_len {
                    continue;
                }
            }

            if left_clean && right_clean {
                return true;
            }
        }
    }

    false
}

/// Normalize one user-supplied lexicon entry the way the built-in list is
/// stored: lower-cased and folded through the same `normalize` pass (so a
/// pasted `Fück` or `fuuuck` behaves like its plain form), with non-alphanumeric
/// characters dropped — an entry is a word or phrase, not a pattern.
fn extra_word_chars(word: &str) -> Vec<char> {
    normalize(&word.to_lowercase())
        .into_iter()
        .map(|n| n.ch)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn apply_placeholder(template: &str, is_playing: bool) -> String {
    let emoji = if is_playing { "🎵" } else { "⏸️" };
    let bytes = template.as_bytes();
    let mut out = String::with_capacity(template.len() + 4);
    let mut i = 0;
    while i < template.len() {
        if bytes[i] == b'{'
            && template.len() - i >= 7
            && bytes[i + 1..i + 7].eq_ignore_ascii_case(b"emoji}")
        {
            out.push_str(emoji);
            i += 7;
        } else if let Some(ch) = template[i..].chars().next() {
            out.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    out
}

/// Renders the placeholder when `text` is profane, else returns `text`.
///
/// `extra_words` is the user's own lexicon (`teams.profanity_extra_words`,
/// issue #538): it is matched with the same boundary/evasion rules as the
/// built-in list (see [`contains_extra_word`]). An empty slice reproduces the
/// pre-#538 behaviour exactly.
pub fn filter_status(
    text: &str,
    placeholder: &str,
    is_playing: bool,
    extra_words: &[String],
) -> String {
    if contains_profanity(text, extra_words) {
        let mut effective_placeholder = if placeholder.trim().is_empty() {
            SAFE_PLACEHOLDER_DEFAULT
        } else {
            placeholder
        };
        if contains_profanity(effective_placeholder, extra_words) {
            log::debug!("[PROFANITY] placeholder flagged; falling back to default");
            effective_placeholder = SAFE_PLACEHOLDER_DEFAULT;
        }
        apply_placeholder(effective_placeholder, is_playing)
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn norm_str(s: &str) -> String {
        normalize(s).iter().map(|n| n.ch).collect()
    }

    #[test]
    fn test_clean_text_passes() {
        assert!(!contains_profanity("Radiohead - Karma Police", &[]));
        assert!(!contains_profanity("Daft Punk - One More Time", &[]));
        assert!(!contains_profanity("Massive Attack - Teardrop", &[]));
        assert!(!contains_profanity("The Beatles - Hey Jude", &[]));
    }

    #[test]
    fn test_filter_returns_placeholder() {
        let result = filter_status("what the fuck", "Custom Placeholder", true, &[]);
        assert_eq!(result, "Custom Placeholder");
    }

    #[test]
    fn test_filter_returns_original_when_clean() {
        let result = filter_status("Daft Punk - One More Time", "Placeholder", true, &[]);
        assert_eq!(result, "Daft Punk - One More Time");
    }

    #[test]
    fn test_leetspeak_substitutions() {
        assert!(contains_profanity("sh1t", &[]));
        assert!(contains_profanity("$hit", &[]));
        assert!(contains_profanity("d@mn", &[]));
        assert!(contains_profanity("p1ss", &[]));
        assert!(contains_profanity("n1gg3r", &[]));
    }

    #[test]
    fn test_extended_leet_substitutions() {
        assert!(contains_profanity("6itch", &[]));
        assert!(contains_profanity("8itch", &[]));
        assert!(contains_profanity("ni99er", &[]));
        assert!(contains_profanity("shi+", &[]));
        assert!(contains_profanity("(ock", &[]));
        assert_eq!(norm_str("2"), "i");
        assert_eq!(norm_str("\\/"), "v");
    }

    #[test]
    fn test_unicode_confusables() {
        let zwsp: String = char::from_u32(0x200B).into_iter().collect();
        assert!(contains_profanity(&format!("f{}uck", zwsp), &[]));
        assert!(contains_profanity("fück", &[]));
        assert!(contains_profanity("ｆｕｃｋ", &[]));
        assert!(contains_profanity("BİTCH", &[]));
    }

    #[test]
    fn test_separator_insertion_evasion() {
        assert!(contains_profanity("f*ck", &[]));
        assert!(contains_profanity("f.u.c.k", &[]));
        assert!(contains_profanity("f_ck", &[]));
        assert!(contains_profanity("f-ck", &[]));
        assert!(contains_profanity("f u c k", &[]));
        assert!(contains_profanity("s.h.i.t", &[]));
    }

    #[test]
    fn test_issue_refinements() {
        assert!(contains_profanity("sh2t", &[]));
        assert!(contains_profanity("shitty", &[]));
        assert!(contains_profanity("bitchy", &[]));
        assert!(!contains_profanity("Push It", &[]));
        assert!(!contains_profanity("push it", &[]));
        assert!(!contains_profanity("cocky", &[]));
    }

    #[test]
    fn test_mixed_repeat_leet() {
        assert!(contains_profanity("fuu1uck", &[]));
    }

    #[test]
    fn test_repeated_char_collapse() {
        assert!(contains_profanity("shiiit", &[]));
        assert!(contains_profanity("fuuuuck", &[]));
    }

    #[test]
    fn test_placeholder_emoji_substitution() {
        let playing = apply_placeholder("Listening to 🎵", true);
        assert_eq!(playing, "Listening to 🎵");

        let paused = apply_placeholder("Listening to ⏸️", false);
        assert_eq!(paused, "Listening to ⏸️");
    }

    #[test]
    fn test_placeholder_emoji_token_substitution() {
        let playing = apply_placeholder("Listening {emoji}", true);
        assert_eq!(playing, "Listening 🎵");

        let paused = apply_placeholder("Listening {emoji}", false);
        assert_eq!(paused, "Listening ⏸️");
    }

    #[test]
    fn test_placeholder_emoji_token_case_insensitive() {
        let result = filter_status("fuck", "Now {Emoji} {EMOJI} {emoji}", true, &[]);
        assert_eq!(result, "Now 🎵 🎵 🎵");
    }

    #[test]
    fn test_placeholder_empty_falls_back() {
        let result = filter_status("fuck", "", true, &[]);
        assert_eq!(result, SAFE_PLACEHOLDER_DEFAULT);
    }

    #[test]
    fn test_profane_placeholder_falls_back() {
        let result = filter_status("fuck you", "my shit mix", true, &[]);
        assert_eq!(result, SAFE_PLACEHOLDER_DEFAULT);
    }

    #[test]
    fn test_word_boundary_respects_clean_words() {
        assert!(!contains_profanity("class", &[]));
        assert!(!contains_profanity("assassin", &[]));
        assert!(!contains_profanity("mass", &[]));
        assert!(!contains_profanity("pass", &[]));
        assert!(!contains_profanity("choke", &[]));
        assert!(!contains_profanity("cocktail", &[]));
        assert!(!contains_profanity("cocktail bar", &[]));
        assert!(!contains_profanity("cocktails", &[]));
        assert!(!contains_profanity("cumulative", &[]));
        assert!(!contains_profanity("vacuum", &[]));
        assert!(!contains_profanity("cockpit", &[]));
        assert!(!contains_profanity("Dickens", &[]));
        assert!(!contains_profanity("Spice Girls - Wannabe", &[]));
        assert!(!contains_profanity("shiitake", &[]));
    }

    // -----------------------------------------------------------------
    // Issue #538 / CfgDiag#3(b): the user-supplied lexicon.
    // -----------------------------------------------------------------

    /// A configured extra word flags through the same path as the built-in
    /// list, and — the point of the feature — it flags the text the user
    /// actually cares about while a substring of an unrelated clean word stays
    /// clean (the boundary gates are the built-in ones, not a naive contains).
    #[test]
    fn test_extra_words_flag_with_built_in_boundaries() {
        let words = vec![
            "fuckface".to_string(),
            "poop".to_string(),
            "not".to_string(),
        ];
        // Explicit hits.
        assert_eq!(
            filter_status("Nothing - fuckface", "Placeholder", true, &words),
            "Placeholder"
        );
        assert_eq!(
            filter_status("Artist - Poop Song", "Placeholder", true, &words),
            "Placeholder"
        );
        // Substrings of unrelated words must NOT flag: 'poop' inside
        // 'pooping'? no — inside a different word, and 'not' inside 'noting'
        // (the built-in stem carve-outs are deliberately not applied to a
        // user's word, so an inflection of it is not invented for them).
        assert_eq!(
            filter_status("Nobody - Mississippi Queen", "Placeholder", true, &words),
            "Nobody - Mississippi Queen"
        );
        assert_eq!(
            filter_status("Artist - Noting Aloud", "Placeholder", true, &words),
            "Artist - Noting Aloud"
        );
        assert_eq!(
            filter_status("Artist - Class Act", "Placeholder", true, &[]),
            "Artist - Class Act"
        );
        // The evasions the built-in list handles are handled for extra words
        // too: separators (standalone, both sides bounded) and leet.
        assert_eq!(
            filter_status("Artist - p.o.o.p", "Placeholder", true, &words),
            "Placeholder"
        );
        assert_eq!(
            filter_status("Artist - fückface", "Placeholder", true, &words),
            "Placeholder"
        );
        // The #578 shape stays clean for an extra word too: `Song (Uncut)`
        // must not fabricate `cunt` by reading the parenthesis as a `c` and
        // dropping the real `cu` — the span check is shared with the built-ins.
        assert_eq!(
            filter_status("Song (Uncut)", "Placeholder", true, &["cunt".to_string()]),
            "Song (Uncut)"
        );
        // ... while the real standalone evasion it exists for still flags.
        assert_eq!(
            filter_status("(unt", "Placeholder", true, &["cunt".to_string()]),
            "Placeholder"
        );
        // Multi-word and punctuation-bearing entries are normalized to a word.
        let phrase = vec!["Bad Word!".to_string()];
        assert_eq!(
            filter_status("Artist - A Bad Word!", "Placeholder", true, &phrase),
            "Placeholder"
        );
        // Extra words are checked on the placeholder too, so a lexicon entry
        // cannot be smuggled in through the replacement text.
        assert_eq!(
            filter_status("fuckface", "my poop mix", true, &words),
            SAFE_PLACEHOLDER_DEFAULT
        );
        // An empty lexicon is exactly the pre-#538 behaviour: a word that is
        // only in the USER's list passes ("fuckface" above is flagged by the
        // built-in list too, via the #579 continuation carve-out).
        assert_eq!(
            filter_status("Artist - flurble", "Placeholder", true, &[]),
            "Artist - flurble"
        );
        assert_eq!(
            filter_status(
                "Artist - flurble",
                "Placeholder",
                true,
                &["flurble".to_string()]
            ),
            "Placeholder"
        );
    }

    #[test]
    fn test_word_list_additions() {
        assert!(contains_profanity("asshole", &[]));
        assert!(contains_profanity("tits", &[]));
        assert!(contains_profanity("twat", &[]));
    }

    #[test]
    fn test_profanity_list_exhaustive() {
        for word in PROFANITY_LIST {
            assert!(
                contains_profanity(word, &[]),
                "profanity list word '{}' should be detected",
                word
            );
        }
    }

    #[test]
    fn test_filter_with_whitespace_placeholder() {
        assert_eq!(
            filter_status("fuck", "   ", true, &[]),
            SAFE_PLACEHOLDER_DEFAULT
        );
        assert_eq!(
            filter_status("fuck", "  \t  ", true, &[]),
            SAFE_PLACEHOLDER_DEFAULT
        );
    }

    #[test]
    fn test_profane_substring_in_phrase() {
        assert!(contains_profanity("Listening to shit song", &[]));
        assert!(contains_profanity("The artist is damn good", &[]));
        assert!(contains_profanity("This is fucking great", &[]));
        assert!(contains_profanity("bullshit", &[]));
        assert!(contains_profanity("dipshit", &[]));
        assert!(contains_profanity("horseshit", &[]));
        assert!(contains_profanity("bullshit song", &[]));
        assert!(contains_profanity("dickhead", &[]));
        assert!(contains_profanity("shithead", &[]));
        assert!(contains_profanity("fuckhead", &[]));
    }

    // issue #260: Spotify track/artist names routinely arrive Title Case or ALL
    // CAPS, so normalize()'s to_lowercase() is load-bearing. Removing it left
    // the whole module green because every other test drives lowercase input.
    #[test]
    fn test_case_insensitive_detection() {
        assert!(contains_profanity("FUCK", &[]));
        assert!(contains_profanity("Shit", &[]));
        assert!(contains_profanity("You BITCH", &[]));
        assert!(contains_profanity("Fucking Great", &[]));
        assert_eq!(
            filter_status("SHIT", "Placeholder", true, &[]),
            "Placeholder"
        );
    }

    // issue #411: `bitch` is a strong stem, so glued compounds like
    // `sonofabitch` flag; tard/cock/spic carve-outs stay exactly as-is.
    #[test]
    fn test_issue_411_sonofabitch() {
        assert!(contains_profanity("sonofabitch", &[]));
        assert!(contains_profanity("SONOFABITCH", &[]));
        assert!(contains_profanity("bullshit", &[]));
        assert!(contains_profanity("bitchy", &[]));
        assert!(!contains_profanity("mustard", &[]));
        assert!(!contains_profanity("peacock", &[]));
        assert!(!contains_profanity("cockpit", &[]));
        assert!(!contains_profanity("spicy", &[]));
        assert!(!contains_profanity("tardy", &[]));
    }

    // issues #377/#470: the most common real-world evasions — `ph` for `f`,
    // dropped-`c` `uk` for `uck`, `x` for `ck`, and `z` for plural `s` —
    // must flag under the same boundary gating as the plain forms, while
    // innocent `ph`/`x`/`z` words stay clean (`skillz` has no profane root:
    // no `kill`/`skill` list entry, so it is a boundary control).
    #[test]
    fn test_issue_377_ph_fuk_x_z_evasions() {
        assert!(contains_profanity("phuck", &[]));
        assert!(contains_profanity("PHUCK", &[]));
        assert!(contains_profanity("fuk", &[]));
        assert!(contains_profanity("fux", &[]));
        assert!(contains_profanity("niggaz", &[]));
        assert!(contains_profanity("bitchez", &[]));
        assert!(contains_profanity("niggas", &[]));
        assert!(contains_profanity("bitches", &[]));
        assert!(!contains_profanity("phone", &[]));
        assert!(!contains_profanity("photo", &[]));
        assert!(!contains_profanity("Phoenix", &[]));
        assert!(!contains_profanity("skillz", &[]));
        assert!(!contains_profanity("Fukushima", &[]));
        assert!(!contains_profanity("Jukebox Hero", &[]));
        assert!(!contains_profanity("Uptown Funk", &[]));
        assert!(!contains_profanity("Explicit", &[]));
        assert!(!contains_profanity("Zombie", &[]));
    }

    // issue #578: the `x`-for-`ck` and `(`-for-`c` folds were applied
    // globally, so real metadata fabricated matches — `Cox` normalized to
    // `cock`, and the parenthesis in `Song (Uncut)` became the `c` of a
    // fabricated `cunt`. Both folds are now scoped to the stem onsets they
    // exist for, while the #377 evasions they were added for still flag.
    #[test]
    fn test_issue_578_scoped_x_and_paren_expansions() {
        assert!(!contains_profanity("Cox", &[]));
        assert!(!contains_profanity("Carl Cox", &[]));
        assert!(!contains_profanity("Coxon", &[]));
        assert!(!contains_profanity("Lynx", &[]));
        assert!(!contains_profanity("Sphinx", &[]));
        assert!(!contains_profanity("Song (Uncut)", &[]));
        // The evasions the folds exist for are untouched.
        assert!(contains_profanity("fux", &[]));
        assert!(contains_profanity("Fux", &[]));
        assert!(contains_profanity("phux", &[]));
        assert!(contains_profanity("(ock", &[]));
        assert!(contains_profanity("(unt", &[]));
        assert!(contains_profanity("(0ck", &[]));
        // Issue #827: the `di` onset fabricates `dick` out of the surname
        // `Dix`, so a bare or possessive `Dix` (and its longer forms) stays
        // clean while a glued profane continuation still flags.
        assert!(!contains_profanity("Dix", &[]));
        assert!(!contains_profanity("Dix's", &[]));
        assert!(!contains_profanity("Dixon", &[]));
        assert!(!contains_profanity("Dixie Chicks", &[]));
        assert!(!contains_profanity("Dickens", &[]));
        assert!(contains_profanity("dixs", &[]));
    }

    // issue #812: the stem-continuation list matched any token that merely
    // STARTED WITH an inflection, so ordinary words whose tails collide
    // with one of those inflections were replaced by the placeholder —
    // `Spices` (spic + es), `Spiced` (spic + ed), `crapes` (crap + es)
    // and `Pizzeria` (the `z`-to-`s` fold reads piss + eria). Plain tails
    // now require a whole inflection while the compound tails (`head`,
    // `boy`, `face`, `wad`, `post`) keep the prefix form (`headed`,
    // `posting`), and the remaining collisions are scoped per stem the
    // way `is_y_tail` already does.
    #[test]
    fn test_issue_812_scoped_stem_continuations() {
        assert!(!contains_profanity("Spices", &[]));
        assert!(!contains_profanity("Spiced", &[]));
        assert!(!contains_profanity("crapes", &[]));
        assert!(!contains_profanity("Pizzeria", &[]));
        assert_eq!(
            filter_status("Artist - Spices", "Placeholder", true, &[]),
            "Artist - Spices"
        );
        // The scoping only carves out the clean collisions: real
        // inflections of the same stems still flag.
        assert!(contains_profanity("spics", &[]));
        assert!(contains_profanity("pisses", &[]));
        assert!(contains_profanity("pissed", &[]));
        assert!(contains_profanity("pissing", &[]));
        assert!(contains_profanity("craps", &[]));
        assert!(contains_profanity("fuckers", &[]));
    }

    // issue #579: the glued-right continuation list stopped at
    // `ing/er/ed/es/s/head`, so the compounds modern titles actually use
    // passed the filter. `ake`/`ens`-style distinct words stay clean
    // (#328) — the list is extended, never turned into "flag anything".
    #[test]
    fn test_issue_579_glued_compound_continuations() {
        assert!(contains_profanity("fuckboy", &[]));
        assert!(contains_profanity("Fuckface", &[]));
        assert!(contains_profanity("fuckwad", &[]));
        assert!(contains_profanity("shitpost", &[]));
        assert!(contains_profanity("shitposting", &[]));
        assert!(contains_profanity("bitchboy", &[]));
        assert!(contains_profanity("bullshit", &[]));
        assert!(contains_profanity("horseshit", &[]));
        assert!(contains_profanity("dipshit", &[]));
        assert!(contains_profanity("sonofabitch", &[]));
        // Clean controls: a glued right token is only profane when it is a
        // known continuation.
        assert!(!contains_profanity("shitake", &[]));
        assert!(!contains_profanity("shiitake", &[]));
        assert!(!contains_profanity("Fukushima", &[]));
        assert!(!contains_profanity("cocktail", &[]));
        assert!(!contains_profanity("Push It", &[]));
    }

    /// Issue #538 / CfgDiag#3(b): the user's own lexicon is applied with the
    /// same evasion machinery as the built-in list, so a word they added is
    /// caught in the forms a real track title uses — and only as a word.
    #[test]
    fn test_extra_words_flag_clean_titles() {
        let extra = ["darn".to_string()];
        assert!(!contains_profanity("Darn it", &[]));
        assert!(contains_profanity("Darn it", &extra));
        assert!(contains_profanity("DARN", &extra));
        assert_eq!(
            filter_status("Darn it", "Placeholder", true, &extra),
            "Placeholder"
        );
        // A title with no extra word is untouched.
        assert!(!contains_profanity("Daft Punk - One More Time", &extra));
    }

    /// Boundaries are respected: a user entry must not fire inside a longer
    /// word, exactly like a built-in stem with an ambiguous edge.
    #[test]
    fn test_extra_words_respect_word_boundaries() {
        let extra = ["spam".to_string()];
        assert!(contains_profanity("spam", &extra));
        assert!(contains_profanity("Spam sandwich", &extra));
        assert!(!contains_profanity("spamalot", &extra));
        assert!(!contains_profanity("mispam", &extra));
        assert!(!contains_profanity("Spammy", &extra));
    }

    /// The separator-skipping / leet machinery applies to the user's words too,
    /// otherwise an added word would be trivially evaded by `s.p.a.m`.
    #[test]
    fn test_extra_words_keep_the_evasion_rules() {
        let extra = ["spam".to_string()];
        assert!(contains_profanity("s.p.a.m", &extra));
        assert!(contains_profanity("s p a m", &extra));
        assert!(contains_profanity("5pam", &extra));
    }

    /// A placeholder that only the user's lexicon flags falls back to the
    /// canonical safe placeholder, like the built-in case (#342).
    #[test]
    fn test_extra_word_placeholder_falls_back_to_default() {
        let extra = ["darn".to_string()];
        assert_eq!(
            filter_status("Darn it", "darn placeholder", true, &extra),
            SAFE_PLACEHOLDER_DEFAULT
        );
    }

    /// Degenerate entries are inert: an empty lexicon, an empty string and a
    /// punctuation-only entry must not panic or match everything.
    #[test]
    fn test_extra_word_entries_are_sanitised() {
        let empty: [String; 0] = [];
        assert!(!contains_profanity("", &empty));
        let junk = ["".to_string(), "!!!".to_string()];
        assert!(!contains_profanity("", &junk));
        assert!(!contains_profanity("Daft Punk - One More Time", &junk));
        assert!(!contains_profanity("hello", &junk));
    }
}
