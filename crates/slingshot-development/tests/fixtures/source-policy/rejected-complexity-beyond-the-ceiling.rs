//! A function that branches 10 ways beyond its first path.

/// Limit 1 the value is compared with.
const LIMIT_1: u32 = 1;

/// Branch 1 the value chooses.
const BRANCH_1: u32 = 1;

/// Limit 2 the value is compared with.
const LIMIT_2: u32 = 2;

/// Branch 2 the value chooses.
const BRANCH_2: u32 = 2;

/// Limit 3 the value is compared with.
const LIMIT_3: u32 = 3;

/// Branch 3 the value chooses.
const BRANCH_3: u32 = 3;

/// Limit 4 the value is compared with.
const LIMIT_4: u32 = 4;

/// Branch 4 the value chooses.
const BRANCH_4: u32 = 4;

/// Limit 5 the value is compared with.
const LIMIT_5: u32 = 5;

/// Branch 5 the value chooses.
const BRANCH_5: u32 = 5;

/// Limit 6 the value is compared with.
const LIMIT_6: u32 = 6;

/// Branch 6 the value chooses.
const BRANCH_6: u32 = 6;

/// Limit 7 the value is compared with.
const LIMIT_7: u32 = 7;

/// Branch 7 the value chooses.
const BRANCH_7: u32 = 7;

/// Limit 8 the value is compared with.
const LIMIT_8: u32 = 8;

/// Branch 8 the value chooses.
const BRANCH_8: u32 = 8;

/// Limit 9 the value is compared with.
const LIMIT_9: u32 = 9;

/// Branch 9 the value chooses.
const BRANCH_9: u32 = 9;

/// Limit 10 the value is compared with.
const LIMIT_10: u32 = 10;

/// Branch 10 the value chooses.
const BRANCH_10: u32 = 10;

/// Chooses a branch.
#[must_use]
pub fn choose(value: u32) -> u32 {
    if value == LIMIT_1 {
        return BRANCH_1;
    }
    if value == LIMIT_2 {
        return BRANCH_2;
    }
    if value == LIMIT_3 {
        return BRANCH_3;
    }
    if value == LIMIT_4 {
        return BRANCH_4;
    }
    if value == LIMIT_5 {
        return BRANCH_5;
    }
    if value == LIMIT_6 {
        return BRANCH_6;
    }
    if value == LIMIT_7 {
        return BRANCH_7;
    }
    if value == LIMIT_8 {
        return BRANCH_8;
    }
    if value == LIMIT_9 {
        return BRANCH_9;
    }
    if value == LIMIT_10 {
        return BRANCH_10;
    }
    0
}
