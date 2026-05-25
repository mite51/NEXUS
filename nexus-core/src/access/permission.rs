//! Permission bitmask — extensible access levels
//!
//! Uses bitflags for future-proof extension without breaking existing grants.

use serde::{Deserialize, Serialize};

bitflags::bitflags! {
    /// Permission bitmask for access control.
    ///
    /// Bits are additive: WRITE implies READ, MODIFY implies WRITE+READ.
    /// Use `contains()` for checking specific bits, or `satisfies()` for level checks.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct Permission: u8 {
        /// No access (blocked/revoked)
        const NONE   = 0b0000_0000;
        /// Pull assets shared with them
        const READ   = 0b0000_0001;
        /// Push new assets to a node/folder
        const WRITE  = 0b0000_0010;
        /// Overwrite/delete existing assets
        const MODIFY = 0b0000_0100;
        // Future: SHARE = 0x08, ADMIN = 0x10
    }
}

// Serialize as u8 for JSON storage
impl Serialize for Permission {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.bits())
    }
}

impl<'de> Deserialize<'de> for Permission {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bits = u8::deserialize(deserializer)?;
        Permission::from_bits(bits)
            .ok_or_else(|| serde::de::Error::custom(format!("invalid permission bits: {:#04x}", bits)))
    }
}

impl Permission {
    /// Common alias: read-only
    pub const READ_ONLY: Self = Self::READ;
    /// Common alias: read + write
    pub const READ_WRITE: Self = Self::READ.union(Self::WRITE);
    /// Common alias: full (read + write + modify)
    pub const FULL: Self = Self::READ.union(Self::WRITE).union(Self::MODIFY);

    /// Check if this permission level is sufficient for a required level.
    /// e.g., `actual.satisfies(Permission::WRITE)` checks that WRITE bit is set.
    pub fn satisfies(self, required: Permission) -> bool {
        self.contains(required)
    }
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_empty() {
            write!(f, "none")
        } else {
            let mut parts = Vec::new();
            if self.contains(Self::READ) {
                parts.push("read");
            }
            if self.contains(Self::WRITE) {
                parts.push("write");
            }
            if self.contains(Self::MODIFY) {
                parts.push("modify");
            }
            write!(f, "{}", parts.join("+"))
        }
    }
}

impl PartialOrd for Permission {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Permission {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.bits().cmp(&other.bits())
    }
}

impl Default for Permission {
    fn default() -> Self {
        Self::NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_bits() {
        assert_eq!(Permission::READ.bits(), 0x01);
        assert_eq!(Permission::WRITE.bits(), 0x02);
        assert_eq!(Permission::MODIFY.bits(), 0x04);
        assert_eq!(Permission::READ_WRITE.bits(), 0x03);
        assert_eq!(Permission::FULL.bits(), 0x07);
    }

    #[test]
    fn test_satisfies() {
        let rw = Permission::READ_WRITE;
        assert!(rw.satisfies(Permission::READ));
        assert!(rw.satisfies(Permission::WRITE));
        assert!(!rw.satisfies(Permission::MODIFY));

        let full = Permission::FULL;
        assert!(full.satisfies(Permission::READ));
        assert!(full.satisfies(Permission::WRITE));
        assert!(full.satisfies(Permission::MODIFY));
    }

    #[test]
    fn test_display() {
        assert_eq!(Permission::NONE.to_string(), "none");
        assert_eq!(Permission::READ.to_string(), "read");
        assert_eq!(Permission::READ_WRITE.to_string(), "read+write");
        assert_eq!(Permission::FULL.to_string(), "read+write+modify");
    }

    #[test]
    fn test_serde_roundtrip() {
        let perm = Permission::READ_WRITE;
        let json = serde_json::to_string(&perm).unwrap();
        assert_eq!(json, "3"); // 0x03
        let back: Permission = serde_json::from_str(&json).unwrap();
        assert_eq!(perm, back);
    }

    #[test]
    fn test_serde_all_values() {
        for bits in 0u8..=7u8 {
            let perm = Permission::from_bits(bits).unwrap();
            let json = serde_json::to_string(&perm).unwrap();
            let back: Permission = serde_json::from_str(&json).unwrap();
            assert_eq!(perm, back);
        }
    }
}
