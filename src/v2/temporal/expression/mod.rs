//! Temporal expression DSL parser and evaluator.
//!
//! Expressions are compiled into fixed-size reverse-polish programs so node
//! temporal fields remain `Copy` and cheap to sample per frame.

mod parser;

use std::error::Error;
use std::fmt::{Display, Formatter};

use super::GraphTimeInput;
use parser::parse_ops;

const MAX_EXPR_OPS: usize = 96;
const MAX_EVAL_STACK: usize = 64;

/// Compile-time error for temporal expression parsing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemporalExpressionError {
    message: String,
}

impl TemporalExpressionError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for TemporalExpressionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for TemporalExpressionError {}

/// One compiled temporal expression.
#[derive(Clone, Copy, Debug)]
pub struct TemporalExpression {
    len: u8,
    ops: [ExprOp; MAX_EXPR_OPS],
}

impl TemporalExpression {
    /// Parse and compile one temporal expression.
    ///
    /// Supported variables: `t`, `i`
    ///
    /// Supported constants: `pi`, `tau`
    ///
    /// Supported operators: `+`, `-`, `*`, `/`
    ///
    /// Supported functions:
    /// `sin`, `cos`, `abs`, `fract`, `tri`, `saw`, `min`, `max`, `clamp`
    pub fn parse(source: &str) -> Result<Self, TemporalExpressionError> {
        let ops = parse_ops(source)?;
        validate_program(&ops)?;
        Self::from_ops(&ops)
    }

    /// Evaluate this expression for one frame time sample.
    pub fn sample(self, time: GraphTimeInput) -> f32 {
        let mut stack = [0.0f32; MAX_EVAL_STACK];
        let mut depth = 0usize;

        for op in self.ops[..self.len as usize].iter().copied() {
            if !op.apply(time, &mut stack, &mut depth) {
                return 0.0;
            }
        }

        if depth == 1 {
            stack[0]
        } else {
            0.0
        }
    }

    fn from_ops(ops: &[ExprOp]) -> Result<Self, TemporalExpressionError> {
        if ops.is_empty() {
            return Err(TemporalExpressionError::new("expression cannot be empty"));
        }
        if ops.len() > MAX_EXPR_OPS {
            return Err(TemporalExpressionError::new(format!(
                "expression too complex: {} ops (max {})",
                ops.len(),
                MAX_EXPR_OPS
            )));
        }

        let mut out = [ExprOp::Const(0.0); MAX_EXPR_OPS];
        out[..ops.len()].copy_from_slice(ops);
        Ok(Self {
            len: ops.len() as u8,
            ops: out,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum ExprOp {
    Const(f32),
    Time,
    Intensity,
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Sin,
    Cos,
    Abs,
    Fract,
    Tri,
    Saw,
    Min,
    Max,
    Clamp,
}

impl ExprOp {
    pub(super) fn stack_delta(self) -> isize {
        match self {
            Self::Const(_) | Self::Time | Self::Intensity => 1,
            Self::Neg | Self::Sin | Self::Cos | Self::Abs | Self::Fract | Self::Tri | Self::Saw => {
                0
            }
            Self::Add | Self::Sub | Self::Mul | Self::Div | Self::Min | Self::Max => -1,
            Self::Clamp => -2,
        }
    }

    fn apply(
        self,
        time: GraphTimeInput,
        stack: &mut [f32; MAX_EVAL_STACK],
        depth: &mut usize,
    ) -> bool {
        match self {
            Self::Const(value) => push(stack, depth, value),
            Self::Time => push(stack, depth, time.normalized),
            Self::Intensity => push(stack, depth, time.intensity),
            Self::Add => binary(stack, depth, |a, b| a + b),
            Self::Sub => binary(stack, depth, |a, b| a - b),
            Self::Mul => binary(stack, depth, |a, b| a * b),
            Self::Div => binary(stack, depth, safe_div),
            Self::Neg => unary(stack, depth, |value| -value),
            Self::Sin => unary(stack, depth, |value| value.sin()),
            Self::Cos => unary(stack, depth, |value| value.cos()),
            Self::Abs => unary(stack, depth, |value| value.abs()),
            Self::Fract => unary(stack, depth, |value| value - value.floor()),
            Self::Tri => unary(stack, depth, tri_wave),
            Self::Saw => unary(stack, depth, saw_wave),
            Self::Min => binary(stack, depth, f32::min),
            Self::Max => binary(stack, depth, f32::max),
            Self::Clamp => ternary(stack, depth, |v, lo, hi| v.clamp(lo, hi)),
        }
    }
}

fn validate_program(ops: &[ExprOp]) -> Result<(), TemporalExpressionError> {
    let mut depth = 0isize;
    for op in ops {
        depth += op.stack_delta();
        if depth < 1 {
            return Err(TemporalExpressionError::new(
                "invalid expression stack state",
            ));
        }
    }
    if depth != 1 {
        return Err(TemporalExpressionError::new(
            "expression must reduce to a single output value",
        ));
    }
    Ok(())
}

fn push(stack: &mut [f32; MAX_EVAL_STACK], depth: &mut usize, value: f32) -> bool {
    if *depth >= MAX_EVAL_STACK {
        return false;
    }
    stack[*depth] = value;
    *depth += 1;
    true
}

fn unary(stack: &mut [f32; MAX_EVAL_STACK], depth: &mut usize, op: fn(f32) -> f32) -> bool {
    if *depth < 1 {
        return false;
    }
    let index = *depth - 1;
    stack[index] = op(stack[index]);
    true
}

fn binary(stack: &mut [f32; MAX_EVAL_STACK], depth: &mut usize, op: fn(f32, f32) -> f32) -> bool {
    if *depth < 2 {
        return false;
    }
    let rhs = stack[*depth - 1];
    let lhs = stack[*depth - 2];
    *depth -= 1;
    stack[*depth - 1] = op(lhs, rhs);
    true
}

fn ternary(
    stack: &mut [f32; MAX_EVAL_STACK],
    depth: &mut usize,
    op: fn(f32, f32, f32) -> f32,
) -> bool {
    if *depth < 3 {
        return false;
    }
    let c = stack[*depth - 1];
    let b = stack[*depth - 2];
    let a = stack[*depth - 3];
    *depth -= 2;
    stack[*depth - 1] = op(a, b, c);
    true
}

fn safe_div(lhs: f32, rhs: f32) -> f32 {
    if rhs.abs() <= 1e-6 {
        0.0
    } else {
        lhs / rhs
    }
}

fn saw_wave(value: f32) -> f32 {
    2.0 * (value - value.floor()) - 1.0
}

fn tri_wave(value: f32) -> f32 {
    1.0 - 2.0 * (2.0 * (value - value.floor()) - 1.0).abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_samples_simple_wave() {
        let expr = TemporalExpression::parse("0.5 * sin(t * tau)").unwrap();
        let start = expr.sample(GraphTimeInput {
            normalized: 0.0,
            intensity: 1.0,
        });
        let quarter = expr.sample(GraphTimeInput {
            normalized: 0.25,
            intensity: 1.0,
        });
        assert!(start.abs() < 1e-6);
        assert!((quarter - 0.5).abs() < 1e-4);
    }

    #[test]
    fn supports_intensity_and_clamp() {
        let expr = TemporalExpression::parse("clamp(2.0 * i, 0.0, 1.0)").unwrap();
        let sample = expr.sample(GraphTimeInput {
            normalized: 0.4,
            intensity: 0.7,
        });
        assert!((sample - 1.0).abs() < 1e-6);
    }

    #[test]
    fn reports_unknown_identifier() {
        let error = TemporalExpression::parse("foo + 1.0").unwrap_err();
        assert!(error.to_string().contains("unknown identifier"));
    }
}
