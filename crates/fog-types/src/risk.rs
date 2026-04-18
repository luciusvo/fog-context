//! Risk tier and defense matrix types.
//!
//! These types encode the FoG Risk Tier system (T0–T3) that governs
//! testing requirements, review gates, and data model constraints.

use serde::{Deserialize, Serialize};

/// Risk tier level for a module.
///
/// Higher tier = higher consequence of failure = stricter constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum RiskTier {
    /// UI, display, cosmetic. Failure: UX degradation.
    #[serde(rename = "T0")]
    T0 = 0,
    /// API, CRUD, standard logic. Failure: visible but recoverable.
    #[serde(rename = "T1")]
    T1 = 1,
    /// Business logic, financial calculations. Failure: business impact.
    #[serde(rename = "T2")]
    T2 = 2,
    /// Payment, auth, PII. Failure: money/data/legal.
    #[serde(rename = "T3")]
    T3 = 3,
}

impl RiskTier {
    pub fn from_u8(val: u8) -> Option<Self> {
        match val {
            0 => Some(Self::T0),
            1 => Some(Self::T1),
            2 => Some(Self::T2),
            3 => Some(Self::T3),
            _ => None,
        }
    }

    pub fn as_u8(&self) -> u8 {
        *self as u8
    }
}

/// Constraints enforced by the defense matrix for a given tier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConstraints {
    pub tier: RiskTier,
    /// Maximum lines of code per file.
    pub max_loc: u32,
    /// Maximum cyclomatic complexity per function.
    pub max_cc: u32,
    /// Maximum nesting depth.
    pub max_nesting: u32,
    /// Whether human review is required.
    pub requires_review: bool,
    /// Percentage of code requiring review (0-100).
    pub review_sampling_pct: u8,
    /// Required test types for this tier.
    pub required_tests: Vec<TestLevel>,
    /// Minimum number of tests required.
    pub min_test_count: u32,
}

/// Test levels in the defense matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestLevel {
    Contract,
    Unit,
    Snapshot,
    Mutation,
    E2e,
}

impl TierConstraints {
    /// Default constraints for a given tier.
    pub fn for_tier(tier: RiskTier) -> Self {
        match tier {
            RiskTier::T0 => Self {
                tier,
                max_loc: 500,
                max_cc: 15,
                max_nesting: 4,
                requires_review: false,
                review_sampling_pct: 0,
                required_tests: vec![TestLevel::Contract],
                min_test_count: 1,
            },
            RiskTier::T1 => Self {
                tier,
                max_loc: 400,
                max_cc: 15,
                max_nesting: 4,
                requires_review: false,
                review_sampling_pct: 0,
                required_tests: vec![TestLevel::Contract, TestLevel::Unit],
                min_test_count: 4,
            },
            RiskTier::T2 => Self {
                tier,
                max_loc: 300,
                max_cc: 10,
                max_nesting: 3,
                requires_review: true,
                review_sampling_pct: 20,
                required_tests: vec![
                    TestLevel::Contract,
                    TestLevel::Unit,
                    TestLevel::Snapshot,
                    TestLevel::Mutation,
                ],
                min_test_count: 8,
            },
            RiskTier::T3 => Self {
                tier,
                max_loc: 200,
                max_cc: 8,
                max_nesting: 3,
                requires_review: true,
                review_sampling_pct: 100,
                required_tests: vec![
                    TestLevel::Contract,
                    TestLevel::Unit,
                    TestLevel::Snapshot,
                    TestLevel::Mutation,
                    TestLevel::E2e,
                ],
                min_test_count: 12,
            },
        }
    }
}

/// Pattern resolution level (1–5) from FoG hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PatternLevel {
    PureFunction = 1,
    Composition = 2,
    HofDict = 3,
    SimpleClass = 4,
    DesignPattern = 5,
}
