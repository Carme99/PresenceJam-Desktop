const PROFANITY_LIST: &[&str] = &[
    "fuck", "shit", "damn", "bitch", "bastard", "crap", "piss", "dick", "cock", "pussy", "cunt",
    "whore", "slut", "fag", "nigger", "nigga", "retard", "spic", "chink", "kike", "dyke", "tard",
    "faggot", "douche",
];

const SAFE_PLACEHOLDER_DEFAULT: &str = "Currently Listening to Spotify";

pub fn safe_placeholder_default() -> &'static str {
    SAFE_PLACEHOLDER_DEFAULT
}

fn collapse_repeated_chars(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev = char::MAX;
    let mut count = 0u32;

    for c in text.chars() {
        if c != prev {
            count = 1;
            result.push(c);
            prev = c;
        } else {
            count += 1;
            if count <= 2 {
                result.push(c);
            }
        }
    }

    result
}

fn normalize(text: &str) -> String {
    let mut result = String::with_capacity(text.len());

    for c in text.to_lowercase().chars() {
        let normalized = match c {
            '1' => 'i',
            '3' => 'e',
            '$' => 's',
            '@' => 'a',
            '0' => 'o',
            '5' => 's',
            '4' => 'a',
            '7' => 't',
            '!' => 'i',
            '|' => 'i',
            _ => c,
        };
        result.push(normalized);
    }

    collapse_repeated_chars(&result)
}

fn is_word_boundary(chars: &[char], start: usize, word_len: usize) -> bool {
    let at_start = start == 0;

    let char_before_ok = at_start || !chars[start - 1].is_alphanumeric();

    let at_end = start + word_len == chars.len();
    let char_after_ok = at_end || !chars[start + word_len].is_alphanumeric();

    char_before_ok && char_after_ok
}

fn matches_at_pos(text: &[char], word: &[char], start: usize) -> Option<usize> {
    let mut si = start;
    let mut wi = 0;

    while wi < word.len() && si < text.len() {
        if text[si] == word[wi] {
            si += 1;
            wi += 1;
        } else if si + 1 < text.len() && text[si + 1] == word[wi] {
            si += 2;
            wi += 1;
        } else {
            return None;
        }
    }

    if wi == word.len() {
        Some(si)
    } else {
        None
    }
}

fn is_valid_suffix_word(s: &str) -> bool {
    matches!(
        s,
        "tail"
            | "head"
            | "hand"
            | "foot"
            | "back"
            | "land"
            | "band"
            | "stand"
            | "find"
            | "mind"
            | "wind"
            | "kind"
            | "line"
            | "time"
            | "home"
            | "name"
            | "case"
            | "place"
            | "life"
            | "work"
            | "part"
            | "sort"
            | "form"
            | "turn"
            | "hold"
            | "keep"
            | "let"
            | "set"
            | "give"
            | "take"
            | "make"
            | "come"
            | "just"
            | "only"
            | "over"
            | "very"
    )
}

fn contains_profanity(text: &str) -> bool {
    let normalized = normalize(text);
    let chars: Vec<char> = normalized.chars().collect();

    for &word in PROFANITY_LIST {
        let word_chars: Vec<char> = word.chars().collect();
        let word_len = word_chars.len();

        if word_len > chars.len() {
            continue;
        }

        for start in 0..=(chars.len() - word_len) {
            let Some(end) = matches_at_pos(&chars, &word_chars, start) else {
                continue;
            };

            let at_start = start == 0;
            let at_end = end >= chars.len();

            if at_end {
                return true;
            }

            let is_fucking_variant = word == "fuck" && end < chars.len() && {
                let suffix: String = chars[end..].iter().collect();
                suffix.starts_with("ing") || suffix.starts_with("er") || suffix.starts_with("ed")
            };

            if is_fucking_variant {
                return true;
            }

            if at_start {
                let suffix_start = end;
                let suffix_len = chars.len() - suffix_start;
                if suffix_len >= 4 {
                    let suffix: String = chars[suffix_start..].iter().collect();
                    if is_valid_suffix_word(&suffix) {
                        continue;
                    }
                }
                return true;
            }

            if is_word_boundary(&chars, start, end - start) {
                return true;
            }
        }
    }

    false
}

fn apply_placeholder(template: &str, is_playing: bool) -> String {
    let emoji = if is_playing { "🎵" } else { "⏸️" };
    template.replace("{emoji}", emoji)
}

pub fn filter_status(text: &str, placeholder: &str, is_playing: bool) -> String {
    if contains_profanity(text) {
        let effective_placeholder = if placeholder.trim().is_empty() {
            SAFE_PLACEHOLDER_DEFAULT
        } else {
            placeholder
        };
        apply_placeholder(effective_placeholder, is_playing)
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_placeholder_empty_falls_back() {
        let result = filter_status("fuck", "", true);
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
        assert!(!contains_profanity("cumulative"));
        assert!(!contains_profanity("vacuum"));
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
    }
}
