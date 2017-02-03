//! Common types for the Cretonne code generator.

use std::default::Default;
use std::fmt::{self, Display, Debug, Formatter};

// ====--------------------------------------------------------------------------------------====//
//
// Value types
//
// ====--------------------------------------------------------------------------------------====//

/// The type of an SSA value.
///
/// The `VOID` type is only used for instructions that produce no value. It can't be part of a SIMD
/// vector.
///
/// Basic integer types: `I8`, `I16`, `I32`, and `I64`. These types are sign-agnostic.
///
/// Basic floating point types: `F32` and `F64`. IEEE single and double precision.
///
/// Boolean types: `B1`, `B8`, `B16`, `B32`, and `B64`. These all encode 'true' or 'false'. The
/// larger types use redundant bits.
///
/// SIMD vector types have power-of-two lanes, up to 256. Lanes can be any int/float/bool type.
///
#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Type(u8);

/// No type. Used for functions without a return value. Can't be loaded or stored. Can't be part of
/// a SIMD vector.
pub const VOID: Type = Type(0);

// Include code generated by `lib/cretonne/meta/gen_types.py`. This file contains constant
// definitions for all the scalar types as well as common vector types for 64, 128, 256, and
// 512-bit SIMD vectors.
include!(concat!(env!("OUT_DIR"), "/types.rs"));

impl Type {
    /// Get the lane type of this SIMD vector type.
    ///
    /// A scalar type is the same as a SIMD vector type with one lane, so it returns itself.
    pub fn lane_type(self) -> Type {
        Type(self.0 & 0x0f)
    }

    /// Get log_2 of the number of bits in a lane.
    pub fn log2_lane_bits(self) -> u8 {
        match self.lane_type() {
            B1 => 0,
            B8 | I8 => 3,
            B16 | I16 => 4,
            B32 | I32 | F32 => 5,
            B64 | I64 | F64 => 6,
            _ => 0,
        }
    }

    /// Get the number of bits in a lane.
    pub fn lane_bits(self) -> u8 {
        match self.lane_type() {
            B1 => 1,
            B8 | I8 => 8,
            B16 | I16 => 16,
            B32 | I32 | F32 => 32,
            B64 | I64 | F64 => 64,
            _ => 0,
        }
    }

    /// Get a type with the same number of lanes as this type, but with the lanes replaced by
    /// booleans of the same size.
    ///
    /// Scalar types are treated as vectors with one lane, so they are converted to the multi-bit
    /// boolean types.
    pub fn as_bool_pedantic(self) -> Type {
        // Replace the low 4 bits with the boolean version, preserve the high 4 bits.
        let lane = match self.lane_type() {
            B8 | I8 => B8,
            B16 | I16 => B16,
            B32 | I32 | F32 => B32,
            B64 | I64 | F64 => B64,
            _ => B1,
        };
        Type(lane.0 | (self.0 & 0xf0))
    }

    /// Get a type with the same number of lanes as this type, but with the lanes replaced by
    /// booleans of the same size.
    ///
    /// Scalar types are all converted to `b1` which is usually what you want.
    pub fn as_bool(self) -> Type {
        if self.is_scalar() {
            B1
        } else {
            self.as_bool_pedantic()
        }
    }

    /// Get a type with the same number of lanes as this type, but with lanes that are half the
    /// number of bits.
    pub fn half_width(self) -> Option<Type> {
        let lane = match self.lane_type() {
            I16 => I8,
            I32 => I16,
            I64 => I32,
            F64 => F32,
            B16 => B8,
            B32 => B16,
            B64 => B32,
            _ => return None,
        };
        Some(Type(lane.0 | (self.0 & 0xf0)))
    }

    /// Get a type with the same number of lanes as this type, but with lanes that are twice the
    /// number of bits.
    pub fn double_width(self) -> Option<Type> {
        let lane = match self.lane_type() {
            I8 => I16,
            I16 => I32,
            I32 => I64,
            F32 => F64,
            B8 => B16,
            B16 => B32,
            B32 => B64,
            _ => return None,
        };
        Some(Type(lane.0 | (self.0 & 0xf0)))
    }

    /// Is this the VOID type?
    pub fn is_void(self) -> bool {
        self == VOID
    }

    /// Is this a scalar boolean type?
    pub fn is_bool(self) -> bool {
        match self {
            B1 | B8 | B16 | B32 | B64 => true,
            _ => false,
        }
    }

    /// Is this a scalar integer type?
    pub fn is_int(self) -> bool {
        match self {
            I8 | I16 | I32 | I64 => true,
            _ => false,
        }
    }

    /// Is this a scalar floating point type?
    pub fn is_float(self) -> bool {
        match self {
            F32 | F64 => true,
            _ => false,
        }
    }

    /// Get log_2 of the number of lanes in this SIMD vector type.
    ///
    /// All SIMD types have a lane count that is a power of two and no larger than 256, so this
    /// will be a number in the range 0-8.
    ///
    /// A scalar type is the same as a SIMD vector type with one lane, so it return 0.
    pub fn log2_lane_count(self) -> u8 {
        self.0 >> 4
    }

    /// Is this a scalar type? (That is, not a SIMD vector type).
    ///
    /// A scalar type is the same as a SIMD vector type with one lane.
    pub fn is_scalar(self) -> bool {
        self.log2_lane_count() == 0
    }

    /// Get the number of lanes in this SIMD vector type.
    ///
    /// A scalar type is the same as a SIMD vector type with one lane, so it returns 1.
    pub fn lane_count(self) -> u16 {
        1 << self.log2_lane_count()
    }

    /// Get the total number of bits used to represent this type.
    pub fn bits(self) -> u16 {
        self.lane_bits() as u16 * self.lane_count()
    }

    /// Get a SIMD vector type with `n` times more lanes than this one.
    ///
    /// If this is a scalar type, this produces a SIMD type with this as a lane type and `n` lanes.
    ///
    /// If this is already a SIMD vector type, this produces a SIMD vector type with `n *
    /// self.lane_count()` lanes.
    pub fn by(self, n: u16) -> Option<Type> {
        if self.lane_bits() == 0 || !n.is_power_of_two() {
            return None;
        }
        let log2_lanes: u32 = n.trailing_zeros();
        let new_type = self.0 as u32 + (log2_lanes << 4);
        if new_type < 0x90 {
            Some(Type(new_type as u8))
        } else {
            None
        }
    }

    /// Get a SIMD vector with half the number of lanes.
    pub fn half_vector(self) -> Option<Type> {
        if self.is_scalar() {
            None
        } else {
            Some(Type(self.0 - 0x10))
        }
    }

    /// Index of this type, for use with hash tables etc.
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

impl Display for Type {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if self.is_void() {
            write!(f, "void")
        } else if self.is_bool() {
            write!(f, "b{}", self.lane_bits())
        } else if self.is_int() {
            write!(f, "i{}", self.lane_bits())
        } else if self.is_float() {
            write!(f, "f{}", self.lane_bits())
        } else if !self.is_scalar() {
            write!(f, "{}x{}", self.lane_type(), self.lane_count())
        } else {
            panic!("Invalid Type(0x{:x})", self.0)
        }
    }
}

impl Debug for Type {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if self.is_void() {
            write!(f, "types::VOID")
        } else if self.is_bool() {
            write!(f, "types::B{}", self.lane_bits())
        } else if self.is_int() {
            write!(f, "types::I{}", self.lane_bits())
        } else if self.is_float() {
            write!(f, "types::F{}", self.lane_bits())
        } else if !self.is_scalar() {
            write!(f, "{:?}X{}", self.lane_type(), self.lane_count())
        } else {
            write!(f, "Type(0x{:x})", self.0)
        }
    }
}

impl Default for Type {
    fn default() -> Type {
        VOID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_scalars() {
        assert_eq!(VOID, VOID.lane_type());
        assert_eq!(0, VOID.bits());
        assert_eq!(B1, B1.lane_type());
        assert_eq!(B8, B8.lane_type());
        assert_eq!(B16, B16.lane_type());
        assert_eq!(B32, B32.lane_type());
        assert_eq!(B64, B64.lane_type());
        assert_eq!(I8, I8.lane_type());
        assert_eq!(I16, I16.lane_type());
        assert_eq!(I32, I32.lane_type());
        assert_eq!(I64, I64.lane_type());
        assert_eq!(F32, F32.lane_type());
        assert_eq!(F64, F64.lane_type());

        assert_eq!(VOID.lane_bits(), 0);
        assert_eq!(B1.lane_bits(), 1);
        assert_eq!(B8.lane_bits(), 8);
        assert_eq!(B16.lane_bits(), 16);
        assert_eq!(B32.lane_bits(), 32);
        assert_eq!(B64.lane_bits(), 64);
        assert_eq!(I8.lane_bits(), 8);
        assert_eq!(I16.lane_bits(), 16);
        assert_eq!(I32.lane_bits(), 32);
        assert_eq!(I64.lane_bits(), 64);
        assert_eq!(F32.lane_bits(), 32);
        assert_eq!(F64.lane_bits(), 64);
    }

    #[test]
    fn typevar_functions() {
        assert_eq!(VOID.half_width(), None);
        assert_eq!(B1.half_width(), None);
        assert_eq!(B8.half_width(), None);
        assert_eq!(B16.half_width(), Some(B8));
        assert_eq!(B32.half_width(), Some(B16));
        assert_eq!(B64.half_width(), Some(B32));
        assert_eq!(I8.half_width(), None);
        assert_eq!(I16.half_width(), Some(I8));
        assert_eq!(I32.half_width(), Some(I16));
        assert_eq!(I32X4.half_width(), Some(I16X4));
        assert_eq!(I64.half_width(), Some(I32));
        assert_eq!(F32.half_width(), None);
        assert_eq!(F64.half_width(), Some(F32));

        assert_eq!(VOID.double_width(), None);
        assert_eq!(B1.double_width(), None);
        assert_eq!(B8.double_width(), Some(B16));
        assert_eq!(B16.double_width(), Some(B32));
        assert_eq!(B32.double_width(), Some(B64));
        assert_eq!(B64.double_width(), None);
        assert_eq!(I8.double_width(), Some(I16));
        assert_eq!(I16.double_width(), Some(I32));
        assert_eq!(I32.double_width(), Some(I64));
        assert_eq!(I32X4.double_width(), Some(I64X4));
        assert_eq!(I64.double_width(), None);
        assert_eq!(F32.double_width(), Some(F64));
        assert_eq!(F64.double_width(), None);
    }

    #[test]
    fn vectors() {
        let big = F64.by(256).unwrap();
        assert_eq!(big.lane_bits(), 64);
        assert_eq!(big.lane_count(), 256);
        assert_eq!(big.bits(), 64 * 256);

        assert_eq!(big.half_vector().unwrap().to_string(), "f64x128");
        assert_eq!(B1.by(2).unwrap().half_vector().unwrap().to_string(), "b1");
        assert_eq!(I32.half_vector(), None);
        assert_eq!(VOID.half_vector(), None);

        // Check that the generated constants match the computed vector types.
        assert_eq!(I32.by(4), Some(I32X4));
        assert_eq!(F64.by(8), Some(F64X8));
    }

    #[test]
    fn format_scalars() {
        assert_eq!(VOID.to_string(), "void");
        assert_eq!(B1.to_string(), "b1");
        assert_eq!(B8.to_string(), "b8");
        assert_eq!(B16.to_string(), "b16");
        assert_eq!(B32.to_string(), "b32");
        assert_eq!(B64.to_string(), "b64");
        assert_eq!(I8.to_string(), "i8");
        assert_eq!(I16.to_string(), "i16");
        assert_eq!(I32.to_string(), "i32");
        assert_eq!(I64.to_string(), "i64");
        assert_eq!(F32.to_string(), "f32");
        assert_eq!(F64.to_string(), "f64");
    }

    #[test]
    fn format_vectors() {
        assert_eq!(B1.by(8).unwrap().to_string(), "b1x8");
        assert_eq!(B8.by(1).unwrap().to_string(), "b8");
        assert_eq!(B16.by(256).unwrap().to_string(), "b16x256");
        assert_eq!(B32.by(4).unwrap().by(2).unwrap().to_string(), "b32x8");
        assert_eq!(B64.by(8).unwrap().to_string(), "b64x8");
        assert_eq!(I8.by(64).unwrap().to_string(), "i8x64");
        assert_eq!(F64.by(2).unwrap().to_string(), "f64x2");
        assert_eq!(I8.by(3), None);
        assert_eq!(I8.by(512), None);
        assert_eq!(VOID.by(4), None);
    }

    #[test]
    fn as_bool() {
        assert_eq!(I32X4.as_bool(), B32X4);
        assert_eq!(I32.as_bool(), B1);
        assert_eq!(I32X4.as_bool_pedantic(), B32X4);
        assert_eq!(I32.as_bool_pedantic(), B32);
    }
}
