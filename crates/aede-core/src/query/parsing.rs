//! Parsing and strict validation of query expressions.

use super::*;

// --------------------------------------------------------------------------
// Reading a query
// --------------------------------------------------------------------------

/// Splits a query into its words, keeping quoted runs whole.
///
/// Parentheses are words of their own so that `(a OR b)` needs no spaces around
/// them, which nobody would remember to type.
fn tokenize(input: &str) -> Result<Vec<String>, QueryError> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for ch in input.chars() {
        match quote {
            Some(q) if ch == q => quote = None,
            Some(_) => current.push(ch),
            None if ch == '"' || ch == '\'' => quote = Some(ch),
            None if ch.is_whitespace() => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
            }
            None if ch == '(' || ch == ')' => {
                if !current.is_empty() {
                    out.push(std::mem::take(&mut current));
                }
                out.push(ch.to_string());
            }
            None => current.push(ch),
        }
    }
    if quote.is_some() {
        return Err(error("a quotation mark is left open"));
    }
    if !current.is_empty() {
        out.push(current);
    }
    Ok(out)
}

/// Reads a query, rejecting incomplete operators and invalid numeric bounds.
///
/// Juxtaposition means AND. Durations accept seconds or `minutes:seconds`;
/// all numeric bounds must be finite and nonnegative.
///
/// ```
/// use aede_core::query::parse;
/// assert!(parse("(artist:ozzy OR artist:dio) duration:..5:00").is_ok());
/// assert!(parse("rating:NaN").is_err());
/// assert!(parse("title:Track AND").is_err());
/// ```
pub fn parse(input: &str) -> Result<Query, QueryError> {
    let words = tokenize(input)?;
    if words.is_empty() {
        return Ok(Query::All);
    }
    let mut at = 0usize;
    let query = parse_or(&words, &mut at, 0)?;
    if at < words.len() {
        return Err(error(format!("\"{}\" is one bracket too many", words[at])));
    }
    Ok(query)
}

fn parse_or(words: &[String], at: &mut usize, depth: usize) -> Result<Query, QueryError> {
    let mut parts = vec![parse_and(words, at, depth)?];
    while *at < words.len() && is_or(&words[*at]) {
        *at += 1;
        parts.push(parse_and(words, at, depth)?);
    }
    Ok(if parts.len() == 1 {
        parts.remove(0)
    } else {
        Query::Or(parts)
    })
}

fn is_or(word: &str) -> bool {
    word.eq_ignore_ascii_case("or") || word == "|" || word == "||"
}

fn parse_and(words: &[String], at: &mut usize, depth: usize) -> Result<Query, QueryError> {
    let mut parts: Vec<Query> = Vec::new();
    while *at < words.len() && words[*at] != ")" && !is_or(&words[*at]) {
        // `AND` may be written, and is what juxtaposition already means.
        if words[*at].eq_ignore_ascii_case("and") {
            if parts.is_empty() {
                return Err(error("AND needs an expression before it"));
            }
            *at += 1;
            if *at >= words.len()
                || words[*at] == ")"
                || is_or(&words[*at])
                || words[*at].eq_ignore_ascii_case("and")
            {
                return Err(error("AND needs an expression after it"));
            }
            parts.push(parse_unary(words, at, depth)?);
            continue;
        }
        parts.push(parse_unary(words, at, depth)?);
    }
    if parts.is_empty() {
        return Err(error("something is missing between the brackets"));
    }
    Ok(if parts.len() == 1 {
        parts.remove(0)
    } else {
        Query::And(parts)
    })
}

fn parse_unary(words: &[String], at: &mut usize, depth: usize) -> Result<Query, QueryError> {
    let Some(word) = words.get(*at) else {
        return Err(error("an expression is missing"));
    };
    parse_unary_word(words, at, depth, word)
}

fn parse_unary_word(
    words: &[String],
    at: &mut usize,
    depth: usize,
    word: &str,
) -> Result<Query, QueryError> {
    if depth > 128 {
        return Err(error("a query exceeds 128 nested brackets or negations"));
    }
    if word.eq_ignore_ascii_case("and") || is_or(word) {
        return Err(error(format!("{word} needs an expression on both sides")));
    }
    if word == "-" || word.eq_ignore_ascii_case("not") {
        *at += 1;
        if *at >= words.len() {
            return Err(error("nothing follows the minus sign"));
        }
        return Ok(Query::Not(Box::new(parse_unary(words, at, depth + 1)?)));
    }
    if let Some(rest) = word.strip_prefix('-')
        && !rest.is_empty()
    {
        // `-genre:metal`, the common spelling, with no space after the sign.
        // Borrow the token suffix. Cloning all remaining words for every
        // negative term makes a flat, shallow query quadratic in its length.
        let negated = parse_unary_word(words, at, depth + 1, rest)?;
        return Ok(Query::Not(Box::new(negated)));
    }
    if word == "(" {
        *at += 1;
        let inside = parse_or(words, at, depth + 1)?;
        if *at >= words.len() || words[*at] != ")" {
            return Err(error("a bracket is left open"));
        }
        *at += 1;
        return Ok(inside);
    }
    if word == ")" {
        return Err(error("a closing bracket has nothing to close"));
    }
    let term = parse_term(word)?;
    *at += 1;
    Ok(Query::Term(term))
}

fn parse_term(word: &str) -> Result<Term, QueryError> {
    let Some((name, value)) = word.split_once(':') else {
        // A bare word is a question when it names a field that can be asked
        // one, and a search otherwise.
        if let Some(field) = field_named(word)
            && asks_whether_it_holds_anything(&field)
        {
            return Ok(Term {
                field,
                test: Test::Set,
            });
        }
        return Ok(Term {
            field: Field::Anything,
            test: Test::Contains(word.to_string()),
        });
    };
    let Some(field) = field_named(name) else {
        return Err(error(format!(
            "\"{name}\" is not a field.\nFields: {}",
            FIELD_NAMES
                .iter()
                .map(|(n, _)| *n)
                .collect::<Vec<_>>()
                .join(", ")
        )));
    };
    let test = parse_test(&field, value)?;
    Ok(Term { field, test })
}

fn parse_test(field: &Field, value: &str) -> Result<Test, QueryError> {
    if value.is_empty() {
        return Err(error("a field needs something after the colon"));
    }
    // A flag asked with a word: `lossless:false` reads better than `-lossless`
    // and means the same, so both are accepted rather than one being a trap.
    if is_flag(field) {
        match value.to_lowercase().as_str() {
            "true" | "yes" | "1" => return Ok(Test::Set),
            "false" | "no" | "0" => return Ok(Test::Unset),
            other => {
                return Err(error(format!(
                    "\"{other}\" is not a yes or a no: try true or false"
                )));
            }
        }
    }
    if is_numeric(field) {
        // A range, either end of which may be left open: `1990..`, `..1999`.
        if let Some((low, high)) = value.split_once("..") {
            let low = parse_number(field, low)?;
            let high = parse_number(field, high)?;
            if low.is_none() && high.is_none() {
                return Err(error("a range needs at least one bound"));
            }
            if let (Some(low), Some(high)) = (low, high)
                && low > high
            {
                return Err(error("the lower range bound exceeds the upper bound"));
            }
            return Ok(Test::Between(low, high));
        }
        for (prefix, compare) in [
            (">=", Compare::AtLeast),
            ("<=", Compare::AtMost),
            (">", Compare::Greater),
            ("<", Compare::Less),
            ("=", Compare::Equal),
        ] {
            if let Some(rest) = value.strip_prefix(prefix) {
                let Some(number) = parse_number(field, rest)? else {
                    return Err(error(format!("\"{rest}\" is not a number")));
                };
                return Ok(Test::Compare(compare, number));
            }
        }
        let Some(number) = parse_number(field, value)? else {
            return Err(error(format!("\"{value}\" is not a number")));
        };
        return Ok(Test::Compare(Compare::Equal, number));
    }
    if let Some(exact) = value.strip_prefix('=') {
        return Ok(Test::Is(exact.to_string()));
    }
    Ok(Test::Contains(value.to_string()))
}

/// Reads a number, accepting `3:45` wherever a duration is expected.
fn parse_number(field: &Field, raw: &str) -> Result<Option<f64>, QueryError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }
    if *field == Field::Duration
        && let Some((minutes, seconds)) = raw.split_once(':')
    {
        let minutes: f64 = minutes
            .parse()
            .map_err(|_| error(format!("\"{raw}\" is not a length")))?;
        let seconds: f64 = seconds
            .parse()
            .map_err(|_| error(format!("\"{raw}\" is not a length")))?;
        if !minutes.is_finite()
            || minutes < 0.0
            || minutes.fract() != 0.0
            || !seconds.is_finite()
            || !(0.0..60.0).contains(&seconds)
        {
            return Err(error(format!(
                "\"{raw}\" needs whole minutes and seconds below 60"
            )));
        }
        return finite_number((minutes * 60.0 + seconds) * 1000.0, raw).map(Some);
    }
    let number: f64 = raw
        .parse()
        .map_err(|_| error(format!("\"{raw}\" is not a number")))?;
    // Durations are stored in milliseconds and typed in seconds.
    let number = if *field == Field::Duration {
        number * 1000.0
    } else {
        number
    };
    finite_number(number, raw).map(Some)
}

fn finite_number(number: f64, raw: &str) -> Result<f64, QueryError> {
    if !number.is_finite() || number < 0.0 {
        return Err(error(format!("\"{raw}\" must be finite and nonnegative")));
    }
    Ok(number)
}
