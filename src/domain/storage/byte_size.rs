//! Value objects for representing amounts of storage.

use std::fmt;
use std::ops::{Add, AddAssign, Sub};

/// An amount of bytes. Arithmetic is saturating so aggregates can never overflow.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteSize(u64);

/// Which base to use when rendering sizes for humans.
///
/// macOS (Finder, "About This Mac") uses decimal units, Linux tools usually use binary.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SizeBase {
    /// 1 KB = 1000 bytes.
    #[default]
    Decimal,
    /// 1 KiB = 1024 bytes.
    Binary,
}

impl ByteSize {
    pub const ZERO: Self = Self(0);

    pub const fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    pub const fn as_u64(self) -> u64 {
        self.0
    }

    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }

    pub const fn saturating_sub(self, other: Self) -> Self {
        Self(self.0.saturating_sub(other.0))
    }

    /// Fraction (0.0..=1.0) that `self` represents of `whole`. Zero when `whole` is zero.
    pub fn ratio_of(self, whole: Self) -> f64 {
        if whole.0 == 0 { 0.0 } else { (self.0 as f64 / whole.0 as f64).clamp(0.0, 1.0) }
    }

    /// Human readable representation such as `12.4 GB` or `512 B`.
    pub fn format(self, base: SizeBase) -> String {
        let (divisor, units): (f64, &[&str]) = match base {
            SizeBase::Decimal => (1000.0, &["B", "KB", "MB", "GB", "TB", "PB"]),
            SizeBase::Binary => (1024.0, &["B", "KiB", "MiB", "GiB", "TiB", "PiB"]),
        };
        if self.0 < divisor as u64 {
            return format!("{} B", self.0);
        }
        let mut value = self.0 as f64;
        let mut unit_index = 0;
        while value >= divisor && unit_index < units.len() - 1 {
            value /= divisor;
            unit_index += 1;
        }
        if value >= 100.0 {
            format!("{value:.0} {}", units[unit_index])
        } else {
            format!("{value:.1} {}", units[unit_index])
        }
    }
}

impl Add for ByteSize {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self.saturating_add(other)
    }
}

impl AddAssign for ByteSize {
    fn add_assign(&mut self, other: Self) {
        *self = self.saturating_add(other);
    }
}

impl Sub for ByteSize {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        self.saturating_sub(other)
    }
}

impl fmt::Display for ByteSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.format(SizeBase::Decimal))
    }
}

/// Which of the two sizes of a file the user wants to look at.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SizeMode {
    /// Blocks actually allocated on disk. This is what fills a volume.
    #[default]
    Allocated,
    /// Logical length of the file. Sparse files and APFS clones make this misleading.
    Apparent,
}

impl SizeMode {
    pub const fn toggled(self) -> Self {
        match self {
            Self::Allocated => Self::Apparent,
            Self::Apparent => Self::Allocated,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Allocated => "allocated",
            Self::Apparent => "apparent",
        }
    }
}

/// Both sizes of a file or of an aggregate, kept together so they always stay in sync.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MeasuredSize {
    pub allocated: ByteSize,
    pub apparent: ByteSize,
}

impl MeasuredSize {
    pub const ZERO: Self = Self { allocated: ByteSize::ZERO, apparent: ByteSize::ZERO };

    pub const fn new(allocated: ByteSize, apparent: ByteSize) -> Self {
        Self { allocated, apparent }
    }

    pub const fn select(self, mode: SizeMode) -> ByteSize {
        match mode {
            SizeMode::Allocated => self.allocated,
            SizeMode::Apparent => self.apparent,
        }
    }

    pub const fn saturating_add(self, other: Self) -> Self {
        Self {
            allocated: self.allocated.saturating_add(other.allocated),
            apparent: self.apparent.saturating_add(other.apparent),
        }
    }

    pub const fn saturating_sub(self, other: Self) -> Self {
        Self {
            allocated: self.allocated.saturating_sub(other.allocated),
            apparent: self.apparent.saturating_sub(other.apparent),
        }
    }

    pub const fn is_zero(self) -> bool {
        self.allocated.is_zero() && self.apparent.is_zero()
    }
}

impl Add for MeasuredSize {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self.saturating_add(other)
    }
}

impl AddAssign for MeasuredSize {
    fn add_assign(&mut self, other: Self) {
        *self = self.saturating_add(other);
    }
}
