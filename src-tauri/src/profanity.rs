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
#[derive(Clone, Copy)]
struct NormChar {
    ch: char,
    leet: bool,
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
            });
            result.push(NormChar {
                ch: 'k',
                leet: true,
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
                });
            } else {
                result.push(NormChar {
                    ch: '(',
                    leet: false,
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
            });
        }
        result.push(NormChar {
            ch: mapped,
            leet: leet || folded_is_lossy,
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
/// word: inflections (`ing`/`er`/`ed`/plurals), insult compounds
/// (`head`) and the glued compounds modern titles use (`boy`/`face`/
/// `wad`/`post` — #579: `fuckboy`, `fuckface`, `shitposting`). Anything
/// else (`pit`, `ens`, `ake`) is a distinct clean word (#328).
fn is_profane_continuation(token: &str) -> bool {
    [
        "ing", "er", "ed", "es", "s", "head", "boy", "face", "wad", "post",
    ]
    .iter()
    .any(|p| token.starts_with(p))
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

fn contains_profanity(text: &str) -> bool {
    let chars = normalize(text);

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
                if is_profane_continuation(&token) {
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
            if is_profane_continuation(&token) {
                return true;
            }
        }
    }

    false
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

pub fn filter_status(text: &str, placeholder: &str, is_playing: bool) -> String {
    if contains_profanity(text) {
        let mut effective_placeholder = if placeholder.trim().is_empty() {
            SAFE_PLACEHOLDER_DEFAULT
        } else {
            placeholder
        };
        if contains_profanity(effective_placeholder) {
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
        assert!(!contains_profanity("Radiohead - Karma Police"));
        assert!(!contains_profanity("Daft Punk - One More Time"));
        assert!(!contains_profanity("Massive Attack - Teardrop"));
        assert!(!contains_profanity("The Beatles - Hey Jude"));
    }

    #[test]
    fn test_filter_returns_placeholder() {
        let result = filter_status("what the fuck", "Custom Placeholder", true);
        assert_eq!(result, "Custom Placeholder");
    }

    #[test]
    fn test_filter_returns_original_when_clean() {
        let result = filter_status("Daft Punk - One More Time", "Placeholder", true);
        assert_eq!(result, "Daft Punk - One More Time");
    }

    #[test]
    fn test_leetspeak_substitutions() {
        assert!(contains_profanity("sh1t"));
        assert!(contains_profanity("$hit"));
        assert!(contains_profanity("d@mn"));
        assert!(contains_profanity("p1ss"));
        assert!(contains_profanity("n1gg3r"));
    }

    #[test]
    fn test_extended_leet_substitutions() {
        assert!(contains_profanity("6itch"));
        assert!(contains_profanity("8itch"));
        assert!(contains_profanity("ni99er"));
        assert!(contains_profanity("shi+"));
        assert!(contains_profanity("(ock"));
        assert_eq!(norm_str("2"), "i");
        assert_eq!(norm_str("\\/"), "v");
    }

    #[test]
    fn test_unicode_confusables() {
        let zwsp: String = char::from_u32(0x200B).into_iter().collect();
        assert!(contains_profanity(&format!("f{}uck", zwsp)));
        assert!(contains_profanity("fück"));
        assert!(contains_profanity("ｆｕｃｋ"));
        assert!(contains_profanity("BİTCH"));
    }

    #[test]
    fn test_separator_insertion_evasion() {
        assert!(contains_profanity("f*ck"));
        assert!(contains_profanity("f.u.c.k"));
        assert!(contains_profanity("f_ck"));
        assert!(contains_profanity("f-ck"));
        assert!(contains_profanity("f u c k"));
        assert!(contains_profanity("s.h.i.t"));
    }

    #[test]
    fn test_issue_refinements() {
        assert!(contains_profanity("sh2t"));
        assert!(contains_profanity("shitty"));
        assert!(contains_profanity("bitchy"));
        assert!(!contains_profanity("Push It"));
        assert!(!contains_profanity("push it"));
        assert!(!contains_profanity("cocky"));
    }

    #[test]
    fn test_mixed_repeat_leet() {
        assert!(contains_profanity("fuu1uck"));
    }

    #[test]
    fn test_repeated_char_collapse() {
        assert!(contains_profanity("shiiit"));
        assert!(contains_profanity("fuuuuck"));
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
        let result = filter_status("fuck", "Now {Emoji} {EMOJI} {emoji}", true);
        assert_eq!(result, "Now 🎵 🎵 🎵");
    }

    #[test]
    fn test_placeholder_empty_falls_back() {
        let result = filter_status("fuck", "", true);
        assert_eq!(result, SAFE_PLACEHOLDER_DEFAULT);
    }

    #[test]
    fn test_profane_placeholder_falls_back() {
        let result = filter_status("fuck you", "my shit mix", true);
        assert_eq!(result, SAFE_PLACEHOLDER_DEFAULT);
    }

    #[test]
    fn test_word_boundary_respects_clean_words() {
        assert!(!contains_profanity("class"));
        assert!(!contains_profanity("assassin"));
        assert!(!contains_profanity("mass"));
        assert!(!contains_profanity("pass"));
        assert!(!contains_profanity("choke"));
        assert!(!contains_profanity("cocktail"));
        assert!(!contains_profanity("cocktail bar"));
        assert!(!contains_profanity("cocktails"));
        assert!(!contains_profanity("cumulative"));
        assert!(!contains_profanity("vacuum"));
        assert!(!contains_profanity("cockpit"));
        assert!(!contains_profanity("Dickens"));
        assert!(!contains_profanity("Spice Girls - Wannabe"));
        assert!(!contains_profanity("shiitake"));
    }

    #[test]
    fn test_word_list_additions() {
        assert!(contains_profanity("asshole"));
        assert!(contains_profanity("tits"));
        assert!(contains_profanity("twat"));
    }

    #[test]
    fn test_profanity_list_exhaustive() {
        for word in PROFANITY_LIST {
            assert!(
                contains_profanity(word),
                "profanity list word '{}' should be detected",
                word
            );
        }
    }

    #[test]
    fn test_filter_with_whitespace_placeholder() {
        assert_eq!(filter_status("fuck", "   ", true), SAFE_PLACEHOLDER_DEFAULT);
        assert_eq!(
            filter_status("fuck", "  \t  ", true),
            SAFE_PLACEHOLDER_DEFAULT
        );
    }

    #[test]
    fn test_profane_substring_in_phrase() {
        assert!(contains_profanity("Listening to shit song"));
        assert!(contains_profanity("The artist is damn good"));
        assert!(contains_profanity("This is fucking great"));
        assert!(contains_profanity("bullshit"));
        assert!(contains_profanity("dipshit"));
        assert!(contains_profanity("horseshit"));
        assert!(contains_profanity("bullshit song"));
        assert!(contains_profanity("dickhead"));
        assert!(contains_profanity("shithead"));
        assert!(contains_profanity("fuckhead"));
    }

    // issue #260: Spotify track/artist names routinely arrive Title Case or ALL
    // CAPS, so normalize()'s to_lowercase() is load-bearing. Removing it left
    // the whole module green because every other test drives lowercase input.
    #[test]
    fn test_case_insensitive_detection() {
        assert!(contains_profanity("FUCK"));
        assert!(contains_profanity("Shit"));
        assert!(contains_profanity("You BITCH"));
        assert!(contains_profanity("Fucking Great"));
        assert_eq!(filter_status("SHIT", "Placeholder", true), "Placeholder");
    }

    // issue #411: `bitch` is a strong stem, so glued compounds like
    // `sonofabitch` flag; tard/cock/spic carve-outs stay exactly as-is.
    #[test]
    fn test_issue_411_sonofabitch() {
        assert!(contains_profanity("sonofabitch"));
        assert!(contains_profanity("SONOFABITCH"));
        assert!(contains_profanity("bullshit"));
        assert!(contains_profanity("bitchy"));
        assert!(!contains_profanity("mustard"));
        assert!(!contains_profanity("peacock"));
        assert!(!contains_profanity("cockpit"));
        assert!(!contains_profanity("spicy"));
        assert!(!contains_profanity("tardy"));
    }

    // issues #377/#470: the most common real-world evasions — `ph` for `f`,
    // dropped-`c` `uk` for `uck`, `x` for `ck`, and `z` for plural `s` —
    // must flag under the same boundary gating as the plain forms, while
    // innocent `ph`/`x`/`z` words stay clean (`skillz` has no profane root:
    // no `kill`/`skill` list entry, so it is a boundary control).
    #[test]
    fn test_issue_377_ph_fuk_x_z_evasions() {
        assert!(contains_profanity("phuck"));
        assert!(contains_profanity("PHUCK"));
        assert!(contains_profanity("fuk"));
        assert!(contains_profanity("fux"));
        assert!(contains_profanity("niggaz"));
        assert!(contains_profanity("bitchez"));
        assert!(contains_profanity("niggas"));
        assert!(contains_profanity("bitches"));
        assert!(!contains_profanity("phone"));
        assert!(!contains_profanity("photo"));
        assert!(!contains_profanity("Phoenix"));
        assert!(!contains_profanity("skillz"));
        assert!(!contains_profanity("Fukushima"));
        assert!(!contains_profanity("Jukebox Hero"));
        assert!(!contains_profanity("Uptown Funk"));
        assert!(!contains_profanity("Explicit"));
        assert!(!contains_profanity("Zombie"));
    }

    // issue #578: the `x`-for-`ck` and `(`-for-`c` folds were applied
    // globally, so real metadata fabricated matches — `Cox` normalized to
    // `cock`, and the parenthesis in `Song (Uncut)` became the `c` of a
    // fabricated `cunt`. Both folds are now scoped to the stem onsets they
    // exist for, while the #377 evasions they were added for still flag.
    #[test]
    fn test_issue_578_scoped_x_and_paren_expansions() {
        assert!(!contains_profanity("Cox"));
        assert!(!contains_profanity("Carl Cox"));
        assert!(!contains_profanity("Coxon"));
        assert!(!contains_profanity("Lynx"));
        assert!(!contains_profanity("Sphinx"));
        assert!(!contains_profanity("Song (Uncut)"));
        // The evasions the folds exist for are untouched.
        assert!(contains_profanity("fux"));
        assert!(contains_profanity("Fux"));
        assert!(contains_profanity("phux"));
        assert!(contains_profanity("(ock"));
        assert!(contains_profanity("(unt"));
        assert!(contains_profanity("(0ck"));
    }

    // issue #579: the glued-right continuation list stopped at
    // `ing/er/ed/es/s/head`, so the compounds modern titles actually use
    // passed the filter. `ake`/`ens`-style distinct words stay clean
    // (#328) — the list is extended, never turned into "flag anything".
    #[test]
    fn test_issue_579_glued_compound_continuations() {
        assert!(contains_profanity("fuckboy"));
        assert!(contains_profanity("Fuckface"));
        assert!(contains_profanity("fuckwad"));
        assert!(contains_profanity("shitpost"));
        assert!(contains_profanity("shitposting"));
        assert!(contains_profanity("bitchboy"));
        assert!(contains_profanity("bullshit"));
        assert!(contains_profanity("horseshit"));
        assert!(contains_profanity("dipshit"));
        assert!(contains_profanity("sonofabitch"));
        // Clean controls: a glued right token is only profane when it is a
        // known continuation.
        assert!(!contains_profanity("shitake"));
        assert!(!contains_profanity("shiitake"));
        assert!(!contains_profanity("Fukushima"));
        assert!(!contains_profanity("cocktail"));
        assert!(!contains_profanity("Push It"));
    }
}
