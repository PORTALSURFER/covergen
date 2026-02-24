//! Recursive-descent parser for temporal expression DSL.

use super::{ExprOp, TemporalExpressionError};

/// Parse source string into reverse-polish op sequence.
pub(super) fn parse_ops(source: &str) -> Result<Vec<ExprOp>, TemporalExpressionError> {
    let mut parser = Parser::new(source);
    parser.parse_expression()?;
    parser.skip_ws();
    if !parser.at_end() {
        return Err(TemporalExpressionError::new(format!(
            "unexpected trailing token at byte {}",
            parser.index
        )));
    }
    Ok(parser.ops)
}

struct Parser<'a> {
    source: &'a str,
    index: usize,
    ops: Vec<ExprOp>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            index: 0,
            ops: Vec::with_capacity(24),
        }
    }

    fn parse_expression(&mut self) -> Result<(), TemporalExpressionError> {
        self.parse_term()?;
        loop {
            self.skip_ws();
            if self.consume("+") {
                self.parse_term()?;
                self.ops.push(ExprOp::Add);
                continue;
            }
            if self.consume("-") {
                self.parse_term()?;
                self.ops.push(ExprOp::Sub);
                continue;
            }
            return Ok(());
        }
    }

    fn parse_term(&mut self) -> Result<(), TemporalExpressionError> {
        self.parse_unary()?;
        loop {
            self.skip_ws();
            if self.consume("*") {
                self.parse_unary()?;
                self.ops.push(ExprOp::Mul);
                continue;
            }
            if self.consume("/") {
                self.parse_unary()?;
                self.ops.push(ExprOp::Div);
                continue;
            }
            return Ok(());
        }
    }

    fn parse_unary(&mut self) -> Result<(), TemporalExpressionError> {
        self.skip_ws();
        if self.consume("+") {
            return self.parse_unary();
        }
        if self.consume("-") {
            self.parse_unary()?;
            self.ops.push(ExprOp::Neg);
            return Ok(());
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<(), TemporalExpressionError> {
        self.skip_ws();
        if self.consume("(") {
            self.parse_expression()?;
            self.expect(")")?;
            return Ok(());
        }
        if let Some(number) = self.parse_number()? {
            self.ops.push(ExprOp::Const(number));
            return Ok(());
        }
        if let Some(ident) = self.parse_identifier() {
            return self.parse_identifier_use(ident);
        }
        Err(TemporalExpressionError::new(format!(
            "expected expression token at byte {}",
            self.index
        )))
    }

    fn parse_identifier_use(&mut self, ident: &str) -> Result<(), TemporalExpressionError> {
        self.skip_ws();
        if self.consume("(") {
            return self.parse_function_call(ident);
        }
        match ident {
            "t" | "time" => self.ops.push(ExprOp::Time),
            "i" | "intensity" => self.ops.push(ExprOp::Intensity),
            "pi" => self.ops.push(ExprOp::Const(std::f32::consts::PI)),
            "tau" => self.ops.push(ExprOp::Const(std::f32::consts::TAU)),
            _ => {
                return Err(TemporalExpressionError::new(format!(
                    "unknown identifier '{ident}'"
                )));
            }
        }
        Ok(())
    }

    fn parse_function_call(&mut self, function: &str) -> Result<(), TemporalExpressionError> {
        let arity = function_arity(function).ok_or_else(|| {
            TemporalExpressionError::new(format!("unsupported function '{function}'"))
        })?;

        for argument in 0..arity {
            self.parse_expression()?;
            if argument + 1 < arity {
                self.expect(",")?;
            }
        }
        self.expect(")")?;
        self.ops.push(function_op(function));
        Ok(())
    }

    fn parse_number(&mut self) -> Result<Option<f32>, TemporalExpressionError> {
        self.skip_ws();
        let rest = &self.source[self.index..];
        let mut consumed = 0usize;
        let mut has_digit = false;
        let mut has_dot = false;
        let mut chars = rest.chars().peekable();

        while let Some(ch) = chars.peek().copied() {
            if ch.is_ascii_digit() {
                consumed += ch.len_utf8();
                chars.next();
                has_digit = true;
                continue;
            }
            if ch == '.' && !has_dot {
                consumed += 1;
                chars.next();
                has_dot = true;
                continue;
            }
            break;
        }

        if !has_digit {
            return Ok(None);
        }
        let token = &rest[..consumed];
        let parsed = token.parse::<f32>().map_err(|_| {
            TemporalExpressionError::new(format!("invalid number literal '{token}'"))
        })?;
        self.index += consumed;
        Ok(Some(parsed))
    }

    fn parse_identifier(&mut self) -> Option<&'a str> {
        self.skip_ws();
        let rest = &self.source[self.index..];
        let mut chars = rest.chars();
        let first = chars.next()?;
        if !(first.is_ascii_alphabetic() || first == '_') {
            return None;
        }

        let mut consumed = first.len_utf8();
        for ch in chars {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                consumed += ch.len_utf8();
            } else {
                break;
            }
        }

        let ident = &rest[..consumed];
        self.index += consumed;
        Some(ident)
    }

    fn expect(&mut self, token: &str) -> Result<(), TemporalExpressionError> {
        self.skip_ws();
        if self.consume(token) {
            Ok(())
        } else {
            Err(TemporalExpressionError::new(format!(
                "expected '{token}' at byte {}",
                self.index
            )))
        }
    }

    fn consume(&mut self, token: &str) -> bool {
        if self.source[self.index..].starts_with(token) {
            self.index += token.len();
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while let Some(ch) = self.source[self.index..].chars().next() {
            if ch.is_whitespace() {
                self.index += ch.len_utf8();
            } else {
                break;
            }
        }
    }

    fn at_end(&self) -> bool {
        self.index >= self.source.len()
    }
}

fn function_arity(function: &str) -> Option<usize> {
    match function {
        "sin" | "cos" | "abs" | "fract" | "tri" | "saw" => Some(1),
        "min" | "max" => Some(2),
        "clamp" => Some(3),
        _ => None,
    }
}

fn function_op(function: &str) -> ExprOp {
    match function {
        "sin" => ExprOp::Sin,
        "cos" => ExprOp::Cos,
        "abs" => ExprOp::Abs,
        "fract" => ExprOp::Fract,
        "tri" => ExprOp::Tri,
        "saw" => ExprOp::Saw,
        "min" => ExprOp::Min,
        "max" => ExprOp::Max,
        "clamp" => ExprOp::Clamp,
        _ => unreachable!("function_op called with unsupported function"),
    }
}
