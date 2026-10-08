//! Validator for the `[ ... ]` code blocks in localization and gui files.
//! The main entry points are the [`validate_datatypes`] function and the [`Datatype`] enum.

use std::fmt::{Display, Formatter};
use std::str::FromStr;

use phf::phf_map;
use strum_macros::{Display, EnumString};

use crate::item::Item;

// Load the game-specific datatype definitions
include!("ck3/include/datatypes.rs");

/// All the object types used in `[...]` code in localization and gui files.
///
/// The names exactly match the ones in the `data_types` logs from the games,
/// which is why some of them are lowercase.
/// Most of the variants are generated directly from those logs.
///
/// The enum is divided into the "generic" datatypes, which are valid for all games and which can
/// be referenced directly in code, and the per-game lists of datatypes which are in game-specific
/// wrappers. With a few exceptions, the per-game datatypes are only referenced in the per-game tables
/// of datafunctions and promotes.
///
/// The game-specific datatypes are wrapped because otherwise they would still have name
/// collisions. This is because the list of generic datatypes is only a small selection; there are
/// many more datatypes that are in effect generic but separating them out would be pointless work.
/// (Separating them out would be made harder because the lists of variants are generated from the docs).
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
#[allow(non_camel_case_types)]
pub enum Datatype {
    // Synthetic datatypes for our typechecking
    Unknown,
    AnyScope,

    // The generic datatypes
    CFixedPoint,
    CString,
    CUTF8String,
    CVector2f,
    CVector2i,
    CVector3f,
    CVector3i,
    CVector4f,
    CVector4i,
    Date,
    Scope,
    TopScope,
    bool,
    double,
    float,
    int16,
    int32,
    int64,
    int8,
    uint16,
    uint32,
    uint64,
    uint8,
    void,

    // Wrappers for the per-game datatypes
    Ck3(Ck3Datatype),
}

static STR_DATATYPE_MAP: phf::Map<&'static str, Datatype> = phf_map! {
    "Unknown" => Datatype::Unknown,
    "AnyScope" => Datatype::AnyScope,
    "CFixedPoint" => Datatype::CFixedPoint,
    "CString" => Datatype::CString,
    "CUTF8String" => Datatype::CUTF8String,
    "CVector2f" => Datatype::CVector2f,
    "CVector2i" => Datatype::CVector2i,
    "CVector3f" => Datatype::CVector3f,
    "CVector3i" => Datatype::CVector3i,
    "CVector4f" => Datatype::CVector4f,
    "CVector4i" => Datatype::CVector4i,
    "Date" => Datatype::Date,
    "Scope" => Datatype::Scope,
    "TopScope" => Datatype::TopScope,
    "bool" => Datatype::bool,
    "double" => Datatype::double,
    "float" => Datatype::float,
    "int16" => Datatype::int16,
    "int32" => Datatype::int32,
    "int64" => Datatype::int64,
    "int8" => Datatype::int8,
    "uint16" => Datatype::uint16,
    "uint32" => Datatype::uint32,
    "uint64" => Datatype::uint64,
    "uint8" => Datatype::uint8,
    "void" => Datatype::void,
};

impl FromStr for Datatype {
    type Err = strum::ParseError;
    /// Read a Datatype from a string, without requiring the string to use the game-specific wrappers.
    fn from_str(s: &str) -> Result<Self, strum::ParseError> {
        STR_DATATYPE_MAP
            .get(s)
            .copied()
            .ok_or(strum::ParseError::VariantNotFound)
            .or_else(|_| Ck3Datatype::from_str(s).map(Datatype::Ck3))
    }
}

impl Display for Datatype {
    /// Convert a `Datatype` to string format, while leaving out the game-specific wrappers.
    fn fmt(&self, f: &mut Formatter) -> Result<(), std::fmt::Error> {
        // Have to do the generic variants by hand, so that the per-game variants can be done with the macro.
        match *self {
            Datatype::Unknown => write!(f, "Unknown"),
            Datatype::AnyScope => write!(f, "AnyScope"),
            Datatype::CFixedPoint => write!(f, "CFixedPoint"),
            Datatype::CString => write!(f, "CString"),
            Datatype::CUTF8String => write!(f, "CUTF8String"),
            Datatype::CVector2f => write!(f, "CVector2f"),
            Datatype::CVector2i => write!(f, "CVector2i"),
            Datatype::CVector3f => write!(f, "CVector3f"),
            Datatype::CVector3i => write!(f, "CVector3i"),
            Datatype::CVector4f => write!(f, "CVector4f"),
            Datatype::CVector4i => write!(f, "CVector4i"),
            Datatype::Date => write!(f, "Date"),
            Datatype::Scope => write!(f, "Scope"),
            Datatype::TopScope => write!(f, "TopScope"),
            Datatype::bool => write!(f, "bool"),
            Datatype::double => write!(f, "double"),
            Datatype::float => write!(f, "float"),
            Datatype::int16 => write!(f, "int16"),
            Datatype::int32 => write!(f, "int32"),
            Datatype::int64 => write!(f, "int64"),
            Datatype::int8 => write!(f, "int8"),
            Datatype::uint16 => write!(f, "uint16"),
            Datatype::uint32 => write!(f, "uint32"),
            Datatype::uint64 => write!(f, "uint64"),
            Datatype::uint8 => write!(f, "uint8"),
            Datatype::void => write!(f, "void"),
            Datatype::Ck3(dt) => dt.fmt(f),
        }
    }
}

/// `Arg` represents what kind of argument is expected by a promote or function.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Arg {
    /// The argument is expected to be a code chain whose final function returns this [`Datatype`],
    /// or a literal that is encoded to be of the expected type.
    DType(Datatype),
    /// The argument is expected to be a literal containing a key to this [`Item`] type, or a code
    /// chain that returns a `CString` (in which case the `Item` lookup is not checked).
    IType(Item),
    /// The argument is considered to be one of the literals in this array, or a code chain that
    /// returns a `CString`.
    #[allow(dead_code)]
    Choice(&'static [&'static str]),
}

/// [`Args`] is the list of arguments expected by a given promote or function.
/// The special value `Args::Unknown` means that all arguments are accepted.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Args {
    Unknown,
    Args(&'static [Arg]),
}

pub use crate::ck3::datafunctions::*;
