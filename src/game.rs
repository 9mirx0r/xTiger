//! Dealing with which game we are validating

use std::fmt::{Display, Formatter};

use bitflags::bitflags;

use crate::helpers::display_choices;

bitflags! {
    /// A set of bitflags to indicate for which game something is intended,
    /// independent of which game we are validating.
    ///
    /// This way, error messages about things being used in the wrong game can be given at runtime.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct GameFlags: u8 {
        const Ck3 = 0x01;
        const Vic3 = 0x02;
        const Imperator = 0x04;
        const Eu5 = 0x08;
        const Hoi4 = 0x10;
    }
}

impl Display for GameFlags {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        let mut vec = Vec::new();
        if self.contains(Self::Ck3) {
            vec.push("Crusader Kings 3");
        }
        if self.contains(Self::Vic3) {
            vec.push("Victoria 3");
        }
        if self.contains(Self::Imperator) {
            vec.push("Imperator: Rome");
        }
        if self.contains(Self::Eu5) {
            vec.push("Europa Universalis 5");
        }
        if self.contains(Self::Hoi4) {
            vec.push("Hearts of Iron 4");
        }
        display_choices(f, &vec, "and")
    }
}
