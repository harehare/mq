//! `# @type((string, [number]) -> bool)` annotations: the sorts a function has in SMT-LIB.
//!
//! The notation follows `mq-check`: `number`, `bool`, `string`, `[T]` for an array and
//! `(A, B) -> R` for a function. A function without an annotation takes and returns numbers.

use super::term::{Result, Signature, SmtError, Sort};

/// Parses the text inside `@type(...)`, e.g. `(string, string) -> string`.
pub(super) fn parse_signature(text: &str) -> Result<Signature> {
    let mut p = Parser { rest: text.trim() };
    let params = p.params()?;
    p.expect("->")?;
    let ret = p.sort()?;
    p.end()?;
    Ok(Signature { params, ret })
}

struct Parser<'a> {
    rest: &'a str,
}

impl Parser<'_> {
    fn bad(&self, what: &str) -> SmtError {
        SmtError::Invalid(format!("@type: expected {what} near `{}`", self.rest))
    }

    fn eat(&mut self, token: &str) -> bool {
        match self.rest.strip_prefix(token) {
            Some(rest) => {
                self.rest = rest.trim_start();
                true
            }
            None => false,
        }
    }

    fn expect(&mut self, token: &str) -> Result<()> {
        if self.eat(token) {
            Ok(())
        } else {
            Err(self.bad(&format!("`{token}`")))
        }
    }

    fn end(&self) -> Result<()> {
        if self.rest.is_empty() {
            Ok(())
        } else {
            Err(self.bad("the end"))
        }
    }

    fn params(&mut self) -> Result<Vec<Sort>> {
        self.expect("(")?;
        let mut params = Vec::new();
        if self.eat(")") {
            return Ok(params);
        }
        loop {
            params.push(self.sort()?);
            if self.eat(")") {
                return Ok(params);
            }
            self.expect(",")?;
        }
    }

    fn sort(&mut self) -> Result<Sort> {
        if self.eat("[") {
            let elem = self.sort()?;
            self.expect("]")?;
            return Ok(Sort::Seq(Box::new(elem)));
        }
        for (name, sort) in [("number", Sort::Int), ("bool", Sort::Bool), ("string", Sort::Str)] {
            if self.eat(name) {
                return Ok(sort);
            }
        }
        let word: String = self
            .rest
            .chars()
            .take_while(|c| c.is_alphanumeric() || *c == '_')
            .collect();
        if word.is_empty() {
            Err(self.bad("a type"))
        } else {
            Err(SmtError::Unsupported(format!(
                "type `{word}` (use number, bool, string or [T])"
            )))
        }
    }
}
