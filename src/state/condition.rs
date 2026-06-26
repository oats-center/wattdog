//! Numeric threshold comparisons.

use crate::config::ComparisonOp;

/// Returns whether `actual` satisfies `op threshold`.
#[must_use]
pub fn matches(op: ComparisonOp, actual: f64, threshold: f64) -> bool {
    match op {
        ComparisonOp::Lt => actual < threshold,
        ComparisonOp::Le => actual <= threshold,
        ComparisonOp::Gt => actual > threshold,
        ComparisonOp::Ge => actual >= threshold,
    }
}

#[cfg(test)]
mod tests {
    use crate::config::ComparisonOp;

    use super::matches;

    #[test]
    fn comparison_operators_match_expected_boundaries() {
        assert!(matches(ComparisonOp::Lt, 1.0, 2.0));
        assert!(!matches(ComparisonOp::Lt, 2.0, 2.0));
        assert!(matches(ComparisonOp::Le, 2.0, 2.0));
        assert!(!matches(ComparisonOp::Le, 3.0, 2.0));
        assert!(matches(ComparisonOp::Gt, 3.0, 2.0));
        assert!(!matches(ComparisonOp::Gt, 2.0, 2.0));
        assert!(matches(ComparisonOp::Ge, 2.0, 2.0));
        assert!(!matches(ComparisonOp::Ge, 1.0, 2.0));
    }
}
