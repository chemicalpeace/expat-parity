//! Billion-laughs accounting (`ACCOUNTING`, `accountingDiffTolerated`).
//!
//! libexpat counts every byte it processes, as *direct* (read from the
//! document) or *indirect* (produced by entity expansion), and stops with
//! `XML_ERROR_AMPLIFICATION_LIMIT_BREACH` once the output exceeds the
//! activation threshold and the amplification factor
//! `(direct + indirect) / direct` exceeds the maximum. Without entities
//! (module 10) the factor stays close to 1; the counters are kept exactly
//! like C so that module 11 can rely on them.

use crate::tok::Scan;

/// `enum XML_Account`: how the bytes of a token are counted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Account {
    /// `XML_ACCOUNT_DIRECT`: bytes of the document.
    Direct,
    /// `XML_ACCOUNT_ENTITY_EXPANSION`: bytes produced by entity expansion.
    EntityExpansion,
    /// `XML_ACCOUNT_NONE`: already counted.
    None,
}

/// `EXPAT_BILLION_LAUGHS_ATTACK_PROTECTION_MAXIMUM_AMPLIFICATION_DEFAULT`.
pub const MAXIMUM_AMPLIFICATION_DEFAULT: f32 = 100.0;

/// `EXPAT_BILLION_LAUGHS_ATTACK_PROTECTION_ACTIVATION_THRESHOLD_DEFAULT`
/// (8 MiB).
pub const ACTIVATION_THRESHOLD_DEFAULT: u64 = 8_388_608;

/// `sizeof("<!ENTITY a SYSTEM 'b'>") - 1`: the shortest document that can
/// include an external entity, used when nothing was read directly.
const LEN_OF_SHORTEST_INCLUDE: u64 = 22;

/// `ACCOUNTING` of the root parser.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Accounting {
    /// `countBytesDirect`
    pub count_bytes_direct: u64,
    /// `countBytesIndirect`
    pub count_bytes_indirect: u64,
    /// `maximumAmplificationFactor` (>= 1.0)
    pub maximum_amplification_factor: f32,
    /// `activationThresholdBytes`
    pub activation_threshold_bytes: u64,
}

impl Default for Accounting {
    fn default() -> Self {
        Accounting {
            count_bytes_direct: 0,
            count_bytes_indirect: 0,
            maximum_amplification_factor: MAXIMUM_AMPLIFICATION_DEFAULT,
            activation_threshold_bytes: ACTIVATION_THRESHOLD_DEFAULT,
        }
    }
}

impl Accounting {
    /// `accountingGetCurrentAmplification`, computed in `float` like C.
    pub fn current_amplification(&self) -> f32 {
        let output = self
            .count_bytes_direct
            .wrapping_add(self.count_bytes_indirect);
        // Integer to float conversions round exactly like C's casts.
        if self.count_bytes_direct != 0 {
            output as f32 / self.count_bytes_direct as f32
        } else {
            LEN_OF_SHORTEST_INCLUDE.wrapping_add(self.count_bytes_indirect) as f32
                / LEN_OF_SHORTEST_INCLUDE as f32
        }
    }

    /// `accountingDiffTolerated` for `bytes` bytes of a token of kind `tok`
    /// read by the root parser (`false` = stop with
    /// `XML_ERROR_AMPLIFICATION_LIMIT_BREACH`).
    pub fn diff_tolerated(&mut self, tok: Scan, bytes: usize, account: Account) -> bool {
        if matches!(
            tok,
            Scan::Invalid | Scan::Partial | Scan::PartialChar | Scan::None
        ) {
            return true;
        }
        let target = match account {
            Account::None => return true,
            Account::Direct => &mut self.count_bytes_direct,
            Account::EntityExpansion => &mut self.count_bytes_indirect,
        };
        let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
        let Some(sum) = target.checked_add(bytes) else {
            return false;
        };
        *target = sum;
        let output = self
            .count_bytes_direct
            .wrapping_add(self.count_bytes_indirect);
        output < self.activation_threshold_bytes
            || self.current_amplification() <= self.maximum_amplification_factor
    }
}
