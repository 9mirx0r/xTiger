//! Plain-language descriptions of the report keys, for `explain <key>`.

use strum::IntoEnumIterator as _;

use crate::report::ErrorKey;

/// What a report key means and how to deal with it.
#[derive(Debug, Clone, Copy)]
pub struct KeyInfo {
    /// The key itself, as it appears in the output.
    pub key: ErrorKey,
    /// One line: what kind of problem this is.
    pub summary: &'static str,
    /// What the game does with the problem, and why the report matters.
    pub meaning: &'static str,
    /// How to fix it.
    pub fix: &'static str,
}

/// All keys, in the order they are declared.
pub fn all_keys() -> impl Iterator<Item = ErrorKey> {
    ErrorKey::iter()
}

/// The description of `key`.
#[allow(clippy::too_many_lines)] // one entry per key; the match makes new keys a compile error
pub fn key_info(key: ErrorKey) -> KeyInfo {
    let (summary, meaning, fix) = match key {
        ErrorKey::Config => (
            "Problem in the validator's own configuration",
            "The ck3-tiger.conf file, or a command line option, is wrong. Reports of this key are \
             always shown and stop the run, because nothing else can be trusted until it is fixed.",
            "Open the file named in the report and fix the line it points at. The conf file uses \
             Paradox script syntax; see ck3-tiger.conf in the repository for every option.",
        ),
        ErrorKey::ReadError => (
            "A file could not be read",
            "The file is missing, locked, or not readable as text. The game would skip it too.",
            "Check that the file exists, is not open exclusively in another program, and is saved \
             as text. Remove it if it is a leftover.",
        ),
        ErrorKey::ParseError => (
            "The script does not follow Paradox script syntax",
            "The parser could not understand the text. The game will usually drop the rest of the \
             file or the whole definition, and may write its own error to error.log.",
            "Go to the line in the report. Typical causes: a missing = between key and value, an \
             unclosed quote, a stray character, or a misplaced brace. Fix the first parse error \
             in a file first; later ones are often caused by it.",
        ),
        ErrorKey::BraceError => (
            "Braces do not balance",
            "There is a { without a matching } or the other way round. Everything after the \
             mismatch is read in the wrong block.",
            "Count the braces around the line in the report. An editor that highlights matching \
             braces finds this fastest. Fix the first one in the file, then re-run.",
        ),
        ErrorKey::BracePlacement => (
            "Braces in an unusual place",
            "A brace is placed where the game accepts it but people usually do not expect it, \
             which makes the file hard to read and easy to break later.",
            "Move the brace to the usual place (opening brace on the same line as its key, \
             closing brace on its own line).",
        ),
        ErrorKey::ReaderDirectives => (
            "A @reader directive or script-value definition is wrong",
            "Directives such as @[ ... ] or the REGISTER_/INFO reader lines have an unexpected \
             form, so the game will not apply them.",
            "Compare with a vanilla file that uses the same directive and fix the spelling or the \
             arguments.",
        ),
        ErrorKey::Packaging => (
            "Problem with how the mod is packaged",
            "The .mod file, descriptor, thumbnail or folder layout does not match what the game \
             and the launcher expect. The mod may not load or may not show up correctly.",
            "Fix the descriptor.mod / .mod file fields named in the report (name, version, \
             supported_version, path, tags) and keep the thumbnail and folders in the standard \
             places.",
        ),
        ErrorKey::Validation => (
            "A value or block is not valid here",
            "The most general key: the syntax is fine, but the content breaks a rule of the \
             game's script (wrong type of value, a field that cannot be used in this context, a \
             malformed identifier). The game may ignore the line or misbehave.",
            "Read the message; it names what was expected. Compare with a vanilla example of the \
             same kind of definition.",
        ),
        ErrorKey::Structure => (
            "The shape of the script is wrong",
            "A block was found where a single value was expected, or the other way round, or a \
             required level of nesting is missing. The game will not read the intended meaning.",
            "Check braces and = signs on the line. Typical: `key = { ... }` written as \
             `key = value`, or the reverse.",
        ),
        ErrorKey::Filename => (
            "A file has a name or location the game ignores",
            "The game only loads certain files from certain folders, and sometimes only with \
             certain names. A file in the wrong place is silently never used.",
            "Move or rename the file as the report says. Mind the extension (.txt, .yml, .gui, \
             .dds) and the exact folder.",
        ),
        ErrorKey::Encoding => (
            "The file encoding is wrong",
            "Localization files must be UTF-8 with a BOM; most script files should be UTF-8 or \
             plain ASCII. A wrong encoding garbles accents or hides the whole file from the game.",
            "Re-save the file as UTF-8 (with BOM for .yml localization). Your editor's \
             \"Save with encoding\" option does this.",
        ),
        ErrorKey::Localization => (
            "Problem in a localization (.yml) file",
            "A line has a malformed key, a missing quote, a bad version number or broken markup, \
             so the game will drop or garble the text.",
            "Each line must look like ` key:0 \"text\"`. Fix the quotes and the colon on the line \
             in the report. The first line of the file must be `l_english:` (or the matching \
             language).",
        ),
        ErrorKey::Markup => (
            "Text formatting markup is wrong",
            "A localization or GUI text uses #format, $variable$, [datafunction] or icon markup \
             that the game cannot parse, so the text will show raw markup or break.",
            "Check that every #format has its closing #!, every $...$ pair is closed and every \
             [ ] contains a valid function.",
        ),
        ErrorKey::DuplicateItem => (
            "The same thing is defined twice",
            "Two definitions share an id. The game keeps only one (usually the later one) and \
             the other silently disappears, which is a common source of \"my change does nothing\".",
            "Rename or delete one of them. If the duplicate is intentional because you are \
             overriding a vanilla definition, make sure your file is the one that loads, and \
             silence the report with a #tiger-ignore comment (see `explain` for how).",
        ),
        ErrorKey::ExactDuplicateItem => (
            "The same thing is defined twice, identically",
            "A copy of a definition exists with exactly the same content. It does no harm today \
             but the copies will drift apart when only one is edited.",
            "Delete one copy.",
        ),
        ErrorKey::DuplicateField => (
            "A field appears twice in one block",
            "The game uses only one of the repeated fields, usually the last, or sometimes the \
             first. Either way one of them has no effect.",
            "Remove the one you do not want, or merge them if the field is meant to be a list.",
        ),
        ErrorKey::DuplicateCharacter => (
            "A character id is defined twice in history",
            "Two history/characters entries use the same id. Only one survives and the other \
             character's family links break.",
            "Give each character a unique id.",
        ),
        ErrorKey::NameConflict => (
            "A name clashes with another name",
            "Two things that must have distinct names share one (for example a scripted effect \
             and a scripted trigger, or a trait and a modifier). The game picks one at random \
             for lookups.",
            "Rename one of them.",
        ),
        ErrorKey::EventNamespace => (
            "Event id does not match its namespace",
            "Events must be named `namespace.number`, and the namespace must be declared at the \
             top of the file. Otherwise the event cannot be fired.",
            "Add `namespace = my_namespace` at the top of the file and make every event id start \
             with `my_namespace.`.",
        ),
        ErrorKey::MissingLocalization => (
            "A localization key is used but not defined",
            "The game will show the raw key text (like `my_event.0001.t`) instead of a message. \
             Reported once per key and per language.",
            "Add the key to a .yml file for each language you ship. If the key is not meant to \
             be translated, point it at an existing key or remove the use. With very many \
             missing keys, add the base language first and use --only-key to focus.",
        ),
        ErrorKey::SuggestLocalization => (
            "A localization key could be added to improve the mod",
            "Nothing is broken, but the game would show a generic text. Adding the key gives \
             players a better description.",
            "Add the suggested key if you want the improved text; otherwise silence the key.",
        ),
        ErrorKey::MissingFile => (
            "A referenced file does not exist",
            "The script points at a texture, mesh, sound bank, animation or other file that is \
             not in the mod or in the game. The game will show a blank or fallback, or log an \
             error.",
            "Check the path and the spelling (including case and the extension). Put the file \
             in the folder the path says, or correct the path.",
        ),
        ErrorKey::MissingSound => (
            "A sound is used but not defined",
            "The script names a sound event that is not in any sound bank, so the player hears \
             nothing.",
            "Fix the name, or define the sound in the mod's sound setup.",
        ),
        ErrorKey::MissingItem => (
            "A name refers to something that is not defined",
            "The script uses an id of a trait, event, scripted effect, title, culture, etc. \
             that the game does not know. Often one missing definition is used from hundreds of \
             places, so a mod can show thousands of these from one cause.",
            "Find the definition: it may be misspelled, removed or renamed in this game version \
             (see the `removed` key), or provided by another mod you must load first (use \
             load_mod in ck3-tiger.conf). Fix the definition or the references. Use --summary \
             to see which names cause most of the noise.",
        ),
        ErrorKey::MissingPerspective => (
            "A character is missing a required portrait perspective",
            "A portrait animation or modifier lacks one of the camera perspectives the game \
             requires.",
            "Add the missing perspective block, copying from a vanilla entry of the same kind.",
        ),
        ErrorKey::ExtraFile => (
            "A file is present that the game will not use",
            "The file is in a folder the game scans but its name or format means it is never \
             loaded, so it only takes up space.",
            "Delete it or move it out of the mod.",
        ),
        ErrorKey::WrongGender => (
            "A gendered thing is used with the wrong gender",
            "A portrait, trait or localization form for one gender is used on a character of \
             the other.",
            "Use the form that matches the character, or the neutral form.",
        ),
        ErrorKey::Conflict => (
            "Two settings contradict each other",
            "Two fields, flags or traits in the same place cannot both apply, so one is ignored \
             or the result is unpredictable.",
            "Pick one. The message names the pair.",
        ),
        ErrorKey::ImageFormat => (
            "An image is not in the format the game wants",
            "The game needs specific DDS settings (format, mipmaps, size multiples) or a \
             specific PNG type for a given use. A wrong format shows black, pink or garbled.",
            "Re-export the image with the format named in the report (for example DXT5 with \
             mipmaps for most icons, or an 8-bit indexed PNG for rivers.png).",
        ),
        ErrorKey::ImageSize => (
            "An image has the wrong dimensions",
            "The game expects a particular width and height, or a size that is a multiple of a \
             number, and stretches or rejects it otherwise.",
            "Resize the image to the size the report states.",
        ),
        ErrorKey::Unneeded => (
            "Something is not needed",
            "The code or file is legal but does nothing: a default value restated, a block \
             that is empty, a replace_path that matches nothing. Safe to ignore, but removing \
             it makes the mod easier to read.",
            "Delete the unneeded part.",
        ),
        ErrorKey::Scopes => (
            "A trigger or effect is used in the wrong scope",
            "The script uses something that only works on another type of object (for example a \
             character effect inside a province scope). The game logs an error and the line \
             does nothing.",
            "Change the scope with the right link (`liege = { ... }`, `capital_province = { \
             ... }`) before the line, or use the version of the effect that exists for this \
             scope. The message says what scope was found and what was expected.",
        ),
        ErrorKey::StrictScopes => (
            "A scope is only probably wrong",
            "Like `scopes`, but the validator had to guess the scope (for example inside a \
             reusable scripted effect), so the report may be a false alarm.",
            "Check the line. If it is fine in every place the effect is used, silence it. You \
             can also tell the validator the right scope with scope_override in ck3-tiger.conf.",
        ),
        ErrorKey::Crash => (
            "This is known to crash the game",
            "The code matches a pattern that makes CK3 crash or freeze (for example certain \
             recursions or invalid portrait data). Fix before releasing.",
            "Remove or rewrite the construct named in the report.",
        ),
        ErrorKey::Range => (
            "A number or date is out of range",
            "The value is outside what the game accepts (for example a percentage above 100, a \
             negative duration, an end date before the start date). The game clamps it or \
             ignores the line.",
            "Use a value inside the stated range.",
        ),
        ErrorKey::Tooltip => (
            "The tooltip will not show what the player expects",
            "A trigger or effect lacks text, or uses a construct (like custom_tooltip placement) \
             that hides or garbles the explanation the player sees.",
            "Add a custom_tooltip or a localized description as the report suggests.",
        ),
        ErrorKey::IfElse => (
            "if / else_if / else is used wrongly",
            "An `else` without a preceding `if`, a limit block in the wrong place, or similar. \
             The game logs an error and the branch is ignored.",
            "Put `else_if` / `else` right after the `if` it belongs to, and put `limit` first in \
             the block.",
        ),
        ErrorKey::Rivers => (
            "Problem in the rivers map image",
            "rivers.png has pixels or structure the game's river system cannot follow.",
            "Open the image at the coordinates in the report and fix the river pixels; each \
             river must be one connected path ending at the sea or a larger river.",
        ),
        ErrorKey::Modifiers => (
            "A modifier is not valid",
            "The modifier name does not exist or is applied to the wrong kind of object, so it \
             has no effect.",
            "Check the spelling and which scopes the modifier supports.",
        ),
        ErrorKey::Macro => (
            "A script macro is used wrongly",
            "A scripted effect or trigger that takes arguments is called without them, or with \
             an argument it does not define ($NAME$).",
            "Pass every $ARG$ the definition uses, with the exact names.",
        ),
        ErrorKey::History => (
            "Problem in the history files",
            "History entries (titles, characters, provinces, cultures) have an invalid or \
             contradictory date, holder or value, so the 867/start state is wrong.",
            "Check the dates and ids in the file named in the report.",
        ),
        ErrorKey::Logic => (
            "The logic is probably not what you meant",
            "The syntax is fine but the condition is always true, always false, redundant, or \
             contradicts itself.",
            "Re-read the condition. The message says what the validator found.",
        ),
        ErrorKey::Bugs => (
            "A known game bug is triggered",
            "The code uses a feature that is known to behave wrongly in the game, so results \
             may differ from what the script says.",
            "Use the workaround the message suggests.",
        ),
        ErrorKey::Datafunctions => (
            "A data function ([ ... ]) in text is wrong",
            "Localization or GUI text calls a function that does not exist, with the wrong \
             arguments, or on the wrong kind of object, so the text shows empty or an error.",
            "Check the function name and the chain, for example `[Character.GetName]`, against \
             the game's own documentation dump (start the game with -debug_mode and type              `script_docs` in the console; the files appear in the logs folder).",
        ),
        ErrorKey::Removed => (
            "This was removed or renamed in a game update",
            "The script uses an effect, trigger, field or id that the current game version no \
             longer has. It does nothing or logs an error.",
            "Use the replacement the message names. For mechanical renames the `migrate` tools \
             of xTiger can apply them for you.",
        ),
        ErrorKey::FieldMissing => (
            "A required field is missing",
            "A definition lacks a field the game needs. The game logs an error or uses a default \
             that is probably not what you want.",
            "Add the field named in the message.",
        ),
        ErrorKey::UnknownField => (
            "A field name is not known here",
            "The block contains a key that is not valid for this type of definition. It may be \
             a typo, a field from another game version, or a field the game never had. The game \
             ignores it.",
            "Check the spelling and compare with a vanilla definition of the same kind. If the \
             field came from another mod, it may be harmless; use --skip-key unknown-field only \
             after checking.",
        ),
        ErrorKey::TitleTier => (
            "A title is used at the wrong tier",
            "Barony, county, duchy, kingdom or empire was expected but another tier was given.",
            "Use a title of the stated tier.",
        ),
        ErrorKey::Colors => (
            "A color is written wrongly",
            "RGB / HSV color values have the wrong number of components or are out of range.",
            "Use `rgb { r g b }`, `hsv { h s v }` or hex as the report says, with values in the \
             allowed range.",
        ),
        ErrorKey::UnusedLocalization => (
            "A localization key is defined but never used",
            "Only reported with --unused. Harmless, but it clutters the files. Many are false \
             positives, because keys can be built dynamically.",
            "Remove the key if you are sure it is unused.",
        ),
        ErrorKey::LocalizationKeyCollision => (
            "The same localization key is defined twice",
            "Two .yml files define the same key; the game uses one and the other text never \
             shows.",
            "Delete one definition or rename it.",
        ),
        ErrorKey::UnusedFile => (
            "A file is never used",
            "Only reported in some checks. The file exists in the mod but nothing refers to it.",
            "Delete it or use it.",
        ),
        ErrorKey::UnknownList => (
            "A list iterator is not known",
            "An every_*, any_*, random_* or ordered_* list does not exist for this scope.",
            "Check the spelling and the scope; the game's script_docs dump lists the available              lists.",
        ),
        ErrorKey::UnknownVariable => (
            "A variable is read but never set",
            "A variable is checked in a trigger, but no code in the mod or the game ever sets \
             it, so the check never passes.",
            "Fix the name, or add the effect that sets the variable.",
        ),
        ErrorKey::Choice => (
            "A value is not one of the allowed choices",
            "The field accepts a fixed set of words (for example a type or a flag) and the word \
             given is not one of them.",
            "Use one of the words the message lists.",
        ),
        ErrorKey::UseOfThis => (
            "`this` is used where it does nothing",
            "`this` refers to the current scope, which makes the line redundant.",
            "Remove the `this`.",
        ),
        ErrorKey::CharacterId => (
            "A character id is wrong",
            "The id in history does not match the expected form, or refers to nobody.",
            "Use a unique id defined in history/characters.",
        ),
        ErrorKey::Loop => (
            "Something refers to itself in a loop",
            "Definitions that include each other never end, which can hang the game.",
            "Break the cycle by removing one of the references.",
        ),
        ErrorKey::Bookmarks => (
            "Problem in a bookmark definition",
            "A bookmark (start date) is missing a piece it needs or points at missing data, so \
             it may not show or may not start.",
            "Complete the bookmark as the message says.",
        ),
        ErrorKey::WrongGame => (
            "This belongs to another Paradox game",
            "The code uses names or syntax from Imperator, Victoria 3 or another game.",
            "Port it to CK3 or delete it.",
        ),
        ErrorKey::Gui => (
            "Problem in a GUI file",
            "A .gui file uses a widget, template, property or type that does not exist, or in \
             the wrong place, so the interface will be missing or broken.",
            "Fix the widget or template named; compare with the vanilla gui files.",
        ),
        ErrorKey::Performance => (
            "This is slow",
            "The construct works but is expensive and, used often, can lag the game.",
            "Use the cheaper alternative the message suggests, or accept it if rare.",
        ),
        ErrorKey::Overflow => (
            "A value may overflow",
            "A number is large enough to go past the game's limits and wrap around.",
            "Use a smaller value.",
        ),
        ErrorKey::Deprecated => (
            "This still works, but is deprecated",
            "The game says this form will be removed in a future version.",
            "Switch to the newer form the message names.",
        ),
        ErrorKey::Variables => (
            "A variable is used wrongly",
            "The variable name or its scope prefix (var:, local_var:, global_var:) is malformed \
             or points at the wrong kind of value.",
            "Check the prefix and spelling, and that the variable is set in the same scope kind.",
        ),
        ErrorKey::DefinitionName => (
            "The name of a definition breaks the naming rules",
            "A definition id uses characters or a form the game does not allow or handle.",
            "Rename it, using only letters, digits and underscores.",
        ),
        ErrorKey::Prefixes => (
            "A prefix is missing or wrong",
            "Names of a given kind must start with a fixed prefix (for example for modifiers or \
             tags); without it the lookups fail.",
            "Add the prefix the message states.",
        ),
        ErrorKey::WrongUse => (
            "This is used in the wrong way",
            "The element is valid but is being used for something it is not meant for.",
            "Follow the message and use the right alternative.",
        ),
        ErrorKey::TemporaryScope => (
            "A temporary scope is used after it has ended",
            "A saved scope with `save_temporary_scope_as` is read where it no longer exists.",
            "Use `save_scope_as` instead, or read it inside the block where it was saved.",
        ),
        ErrorKey::PrincesOfDarkness => (
            "Check specific to the Princes of Darkness mod",
            "Only shown with --pod. Rules for that mod, not for CK3 in general.",
            "Ignore unless you work on Princes of Darkness.",
        ),
        ErrorKey::Internal => (
            "Internal note of the validator",
            "The validator found a situation it does not handle. This is a bug or a gap in the \
             tool, not a problem in your mod.",
            "Report it at the project page with the file and line, and skip it for now.",
        ),
    };
    KeyInfo { key, summary, meaning, fix }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_has_text() {
        for key in all_keys() {
            let info = key_info(key);
            assert!(!info.summary.is_empty(), "{key}");
            assert!(!info.meaning.is_empty(), "{key}");
            assert!(!info.fix.is_empty(), "{key}");
        }
    }
}
