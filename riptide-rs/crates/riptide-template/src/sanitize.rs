//! Filesystem-safe path segment cleanup.

/// Cleans a rendered path segment using riptide-rs filesystem rules.
pub fn clean_segment(input: &str) -> String {
    let mut without_forbidden = String::with_capacity(input.len());
    for ch in input.chars() {
        if matches!(ch, '\\' | '/' | ':' | '"' | '*' | '?' | '<' | '>' | '|') {
            continue;
        }
        without_forbidden.push(ch);
    }

    let mut collapsed_dots = String::with_capacity(without_forbidden.len());
    let mut previous_dot = false;
    for ch in without_forbidden.chars() {
        if ch == '.' {
            if !previous_dot {
                collapsed_dots.push(ch);
            }
            previous_dot = true;
        } else {
            collapsed_dots.push(ch);
            previous_dot = false;
        }
    }

    let mut collapsed_spaces = String::with_capacity(collapsed_dots.len());
    let mut previous_space = false;
    for ch in collapsed_dots.chars() {
        if ch.is_whitespace() {
            if !previous_space {
                collapsed_spaces.push(' ');
            }
            previous_space = true;
        } else {
            collapsed_spaces.push(ch);
            previous_space = false;
        }
    }

    let cleaned = collapsed_spaces.trim().trim_end_matches(['.', ' ']);
    if cleaned.is_empty() {
        "_".to_owned()
    } else {
        cleaned.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_forbidden_characters() {
        assert_eq!(clean_segment(r#"a/b\c:d"*?<>|"#), "abcd");
    }

    #[test]
    fn collapses_dots_and_spaces() {
        assert_eq!(clean_segment("a...  b. "), "a. b");
    }

    #[test]
    fn empty_segment_becomes_underscore() {
        assert_eq!(clean_segment("... "), "_");
    }
}
