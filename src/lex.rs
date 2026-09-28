//! One line into tokens.
//!
//! The grammar leans on tokens being self-identifying: a token's first character says what role it
//! plays, so nothing before it needs a name and nothing after it needs an order. That property
//! lives here, in [`Token::classify`], and everything downstream depends on it.

/// Where a token sat, so a diagnostic can point at it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub line: usize,
    /// 1-based, counted in characters rather than bytes so a diagnostic lines up under the text.
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// `[A-Za-z0-9_.:/-]+` with no leading mark. A kind, a node, a lane, a bare label.
    Word(String),
    /// `"…"`, always a label wherever it appears.
    Quoted(String),
    /// `.name` — a class. The name may itself be quoted: `."odd type"`.
    Class(String),
    /// `+d` — an instant, d after this line's own time.
    After(i64),
    /// `@t` — an instant, absolute.
    At(i64),
    /// `->` or `-x`.
    Arrow { eaten: bool },
    /// A trailing `{`, opening a detail block.
    OpenDetail,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lexed {
    pub token: Token,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

/// A bare word's character set.
///
/// Colons are in it on purpose: a cuelight message type is an arbitrary JSON string, and one that
/// carries a colon should not need quoting to survive. `=`, `#` and `,` are in it so that
/// `color=#0072b2` and `dash=2,6` are each one word — a `#` only begins a comment at the *start*
/// of a token, which is tested at the top of the scanning loop, so it is free to appear inside
/// one.
fn is_bare(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '/' | '-' | '=' | '#' | ',')
}

/// Split one line into tokens, stopping at `#` unless it is inside quotes.
pub fn line(text: &str, line_no: usize) -> Result<Vec<Lexed>, LexError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0usize;

    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        // A comment runs to end of line. Inside a quoted string it is ordinary text, which is why
        // this test sits out here rather than inside the scanner.
        if chars[i] == '#' {
            break;
        }

        let span = Span { line: line_no, col: i + 1 };

        // `{` only means "open a detail block" as the last thing on a line; anywhere else it is
        // a stray character, and saying so is more useful than silently accepting it.
        if chars[i] == '{' {
            let rest: String = chars[i + 1..].iter().collect();
            let rest = rest.split('#').next().unwrap_or("").trim().to_string();
            if !rest.is_empty() {
                return Err(LexError {
                    message: "`{` opens a detail block and must be the last thing on the line".into(),
                    span,
                });
            }
            out.push(Lexed { token: Token::OpenDetail, span });
            i = chars.len();
            continue;
        }

        if chars[i] == '"' {
            let (s, next) = quoted(&chars, i, span)?;
            out.push(Lexed { token: Token::Quoted(s), span });
            i = next;
            continue;
        }

        if chars[i] == '.' {
            // A class. `.` followed by a quoted string covers a type that is not a bare word.
            if i + 1 < chars.len() && chars[i + 1] == '"' {
                let (s, next) = quoted(&chars, i + 1, span)?;
                out.push(Lexed { token: Token::Class(s), span });
                i = next;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < chars.len() && is_bare(chars[j]) {
                j += 1;
            }
            if j == start {
                return Err(LexError { message: "`.` must be followed by a class name".into(), span });
            }
            out.push(Lexed { token: Token::Class(chars[start..j].iter().collect()), span });
            i = j;
            continue;
        }

        if chars[i] == '+' || chars[i] == '@' {
            let mark = chars[i];
            let start = i + 1;
            let mut j = start;
            if j < chars.len() && chars[j] == '-' {
                j += 1;
            }
            let digits_from = j;
            while j < chars.len() && chars[j].is_ascii_digit() {
                j += 1;
            }
            if j == digits_from {
                return Err(LexError {
                    message: format!("`{mark}` must be followed by a number"),
                    span,
                });
            }
            let text: String = chars[start..j].iter().collect();
            let n: i64 = text.parse().map_err(|_| LexError {
                message: format!("`{mark}{text}` is not a whole number of ticks"),
                span,
            })?;
            out.push(Lexed {
                token: if mark == '+' { Token::After(n) } else { Token::At(n) },
                span,
            });
            i = j;
            continue;
        }

        // `->` and `-x`. A bare word may also start with `-`, so an arrow is only an arrow when the
        // two characters stand alone.
        if chars[i] == '-' && i + 1 < chars.len() && (chars[i + 1] == '>' || chars[i + 1] == 'x') {
            let ends = i + 2 >= chars.len() || chars[i + 2].is_whitespace();
            if ends {
                out.push(Lexed {
                    token: Token::Arrow { eaten: chars[i + 1] == 'x' },
                    span,
                });
                i += 2;
                continue;
            }
        }

        if is_bare(chars[i]) {
            let start = i;
            while i < chars.len() && is_bare(chars[i]) {
                i += 1;
            }
            out.push(Lexed { token: Token::Word(chars[start..i].iter().collect()), span });
            continue;
        }

        return Err(LexError {
            message: format!("`{}` cannot start a token", chars[i]),
            span,
        });
    }

    Ok(out)
}

/// Scan a quoted string starting at `open`, returning its contents and the index after the
/// closing quote. `\"` and `\\` are the only escapes; everything else is literal, which is what
/// lets a label hold a Windows path or a regex without a table of exceptions.
fn quoted(chars: &[char], open: usize, span: Span) -> Result<(String, usize), LexError> {
    let mut s = String::new();
    let mut i = open + 1;
    while i < chars.len() {
        match chars[i] {
            '"' => return Ok((s, i + 1)),
            '\\' => {
                if i + 1 >= chars.len() {
                    break;
                }
                match chars[i + 1] {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    other => {
                        return Err(LexError {
                            message: format!("`\\{other}` is not an escape; only `\\\"` and `\\\\` are"),
                            span: Span { line: span.line, col: i + 1 },
                        })
                    }
                }
                i += 2;
            }
            c => {
                s.push(c);
                i += 1;
            }
        }
    }
    Err(LexError { message: "unclosed quoted string".into(), span })
}

/// Whether a string survives being written without quotes. The emitter asks this so it quotes
/// exactly when the bare form would not round-trip — never "always quote to be safe", which would
/// make generated documents differ from hand-written ones that mean the same thing.
pub fn needs_quoting(s: &str) -> bool {
    // A leading `#` would be read back as a comment, and a leading mark as something else entirely,
    // so those need quoting however bare the rest of the characters are.
    s.is_empty()
        || !s.chars().all(is_bare)
        || s.starts_with('#')
        || s.starts_with('.')
        || s.starts_with('+')
        || s.starts_with('@')
}

/// Write a string back out, quoted only if it has to be.
pub fn write_word(s: &str) -> String {
    if needs_quoting(s) {
        let mut out = String::from("\"");
        for c in s.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<Token> {
        line(s, 1).unwrap().into_iter().map(|l| l.token).collect()
    }

    fn err(s: &str) -> LexError {
        line(s, 1).unwrap_err()
    }

    #[test]
    fn a_marked_token_is_recognised_by_its_first_character() {
        assert_eq!(
            toks("0 n0 -> n1 .rb \"hello\" +10"),
            vec![
                Token::Word("0".into()),
                Token::Word("n0".into()),
                Token::Arrow { eaten: false },
                Token::Word("n1".into()),
                Token::Class("rb".into()),
                Token::Quoted("hello".into()),
                Token::After(10),
            ]
        );
    }

    /// The whole reason marks exist: order after the positional tokens carries no meaning.
    #[test]
    fn marked_tokens_may_come_in_any_order() {
        let a = toks("0 n0 -> n1 .rb \"hi\" +10");
        let b = toks("0 n0 -> n1 +10 \"hi\" .rb");
        let mut a2 = a[4..].to_vec();
        let mut b2 = b[4..].to_vec();
        a2.sort_by_key(|t| format!("{t:?}"));
        b2.sort_by_key(|t| format!("{t:?}"));
        assert_eq!(a2, b2);
    }

    #[test]
    fn an_absolute_instant_is_distinct_from_a_relative_one() {
        assert_eq!(toks("@35"), vec![Token::At(35)]);
        assert_eq!(toks("+35"), vec![Token::After(35)]);
    }

    #[test]
    fn an_eaten_message_has_its_own_glyph() {
        assert_eq!(toks("-x"), vec![Token::Arrow { eaten: true }]);
        assert_eq!(toks("->"), vec![Token::Arrow { eaten: false }]);
    }

    /// A colon is legal bare, because a cuelight message type is an arbitrary JSON string and one
    /// carrying a colon should not need quoting.
    #[test]
    fn a_colon_does_not_need_quoting() {
        assert_eq!(toks("mx:request"), vec![Token::Word("mx:request".into())]);
        assert!(!needs_quoting("mx:request"));
    }

    #[test]
    fn a_word_with_a_space_has_to_be_quoted_and_survives_the_round_trip() {
        assert_eq!(toks("\"rb round 2\""), vec![Token::Quoted("rb round 2".into())]);
        assert!(needs_quoting("rb round 2"));
        assert_eq!(write_word("rb round 2"), "\"rb round 2\"");
        assert_eq!(write_word("rb"), "rb");
    }

    #[test]
    fn a_class_may_itself_be_quoted() {
        assert_eq!(toks(".\"odd type\""), vec![Token::Class("odd type".into())]);
    }

    #[test]
    fn the_two_escapes_work_and_no_others_do() {
        assert_eq!(toks(r#""a\"b""#), vec![Token::Quoted("a\"b".into())]);
        assert_eq!(toks(r#""a\\b""#), vec![Token::Quoted("a\\b".into())]);
        assert!(err(r#""a\nb""#).message.contains("not an escape"));
    }

    #[test]
    fn a_comment_ends_the_line_but_not_a_quoted_string() {
        assert_eq!(toks("n0 # everything here is gone"), vec![Token::Word("n0".into())]);
        assert_eq!(toks("\"a # b\""), vec![Token::Quoted("a # b".into())]);
    }

    #[test]
    fn a_detail_block_opens_only_at_the_end_of_a_line() {
        assert_eq!(toks("0 n0 -> n1 {"), vec![
            Token::Word("0".into()),
            Token::Word("n0".into()),
            Token::Arrow { eaten: false },
            Token::Word("n1".into()),
            Token::OpenDetail,
        ]);
        assert!(err("0 n0 { x").message.contains("last thing on the line"));
        // A trailing comment after `{` is still fine — it is not content.
        assert_eq!(toks("0 n0 -> n1 { # body").last(), Some(&Token::OpenDetail));
    }

    /// An arrow is only an arrow when its two characters stand alone, so an ordinary word is free
    /// to begin with a dash.
    #[test]
    fn a_word_may_start_with_a_dash_without_becoming_an_arrow() {
        assert_eq!(toks("-alpha"), vec![Token::Word("-alpha".into())]);
        assert_eq!(toks("-xray"), vec![Token::Word("-xray".into())]);
        assert_eq!(toks("-x"), vec![Token::Arrow { eaten: true }]);
    }

    #[test]
    fn diagnostics_carry_a_line_and_a_column() {
        let e = line("0 n0 -> n1 %", 7).unwrap_err();
        assert_eq!(e.span.line, 7);
        assert_eq!(e.span.col, 12);
        assert!(e.message.contains("cannot start a token"));
    }

    #[test]
    fn an_unclosed_quote_is_an_error_rather_than_silent_truncation() {
        assert!(err("\"never ends").message.contains("unclosed"));
    }

    #[test]
    fn a_mark_with_nothing_after_it_is_an_error() {
        assert!(err("+").message.contains("must be followed by a number"));
        assert!(err("@").message.contains("must be followed by a number"));
        assert!(err(".").message.contains("must be followed by a class name"));
    }

    /// `#` begins a comment only at the start of a token, so a colour survives inside a word.
    #[test]
    fn a_hash_inside_a_word_is_not_a_comment() {
        assert_eq!(toks("color=#0072b2"), vec![Token::Word("color=#0072b2".into())]);
        assert_eq!(toks("dash=2,6"), vec![Token::Word("dash=2,6".into())]);
        assert_eq!(toks("style arrow color=#14171c # ink"), vec![
            Token::Word("style".into()),
            Token::Word("arrow".into()),
            Token::Word("color=#14171c".into()),
        ]);
    }

    /// Anything that would be read back as something other than a word has to be quoted, however
    /// bare its characters are.
    #[test]
    fn a_leading_mark_forces_quoting() {
        for s in ["#tag", ".cls", "+10", "@10", ""] {
            assert!(needs_quoting(s), "{s:?} should need quoting");
        }
        assert!(!needs_quoting("a#b"));
    }

    #[test]
    fn an_instant_may_be_negative_even_though_nothing_sensible_uses_one() {
        assert_eq!(toks("@-5"), vec![Token::At(-5)]);
    }
}
