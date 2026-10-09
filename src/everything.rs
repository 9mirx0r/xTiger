//! Stores everything known about the game and mod being validated.
//!
//! References to [`Everything`] are passed down through nearly all of the validation logic, so
//! that individual functions can access all the defined game items.

use std::borrow::Cow;
use std::fmt::Debug;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use anyhow::Result;
use rayon::{Scope, scope};
use strum::IntoEnumIterator;
use thiserror::Error;

use crate::block::BV;
use crate::block::Block;
use crate::ck3::data::{
    characters::Characters,
    climate::Climate,
    gameconcepts::GameConcepts,
    interaction_cats::CharacterInteractionCategories,
    maa::MenAtArmsTypes,
    prov_history::ProvinceHistories,
    prov_terrain::{ProvinceProperties, ProvinceTerrains},
    provinces::Ck3Provinces,
    title_history::TitleHistories,
    titles::Titles,
    traits::Traits,
    wars::Wars,
};
use crate::ck3::tables::misc::*;
use crate::config_load::{check_for_legacy_ignore, load_filter};
use crate::context::ScopeContext;
use crate::data::data_binding::DataBindings;
use crate::data::{
    assets::Assets,
    defines::Defines,
    gui::Gui,
    localization::Localization,
    on_actions::OnActions,
    scripted_effects::{Effect, Effects},
    scripted_triggers::{Trigger, Triggers},
};
use crate::data::{
    coa::Coas, events::Events, music::Musics, script_values::ScriptValues,
    scripted_lists::ScriptedLists, scripted_modifiers::ScriptedModifiers,
};
use crate::db::{Db, DbKind};
use crate::dds::DdsFiles;
use crate::fileset::{FileEntry, FileKind, FileStage, Fileset};
use crate::helpers::TigerHashSet;
use crate::item::{Item, ItemExt, ItemLoader};
use crate::lowercase::Lowercase;
use crate::macros::MACRO_MAP;
use crate::modfile::ModFile;
use crate::parse::ParserMemory;
use crate::pdxfile::PdxFile;
use crate::report::err;
use crate::report::warn;
use crate::report::{ErrorKey, OutputStyle, Severity, report, set_output_style};
use crate::rivers::Rivers;
use crate::scopes::Scopes;
use crate::token::{Loc, Token};
use crate::variable_scopes::VariableScopes;

#[derive(Debug, Error)]
#[allow(clippy::enum_variant_names)]
pub enum FilesError {
    #[error("Could not read game files at {path}")]
    VanillaUnreadable { path: PathBuf, source: walkdir::Error },
    #[error("Could not read mod files at {path}")]
    ModUnreadable { path: PathBuf, source: walkdir::Error },
    #[error("Could not read config file at {path}")]
    ConfigUnreadable { path: PathBuf },
}

/// A record of everything known about the game and mod being validated.
///
/// References to [`Everything`] are passed down through nearly all of the validation logic, so
/// that individual functions can access all the defined game items.
///
/// The validator has two main phases: parsing and validation.
/// * During parsing, the script files are read, parsed, and loaded into the various databases.
///   `Everything` is mutable during this period.
/// * During validation, `Everything` is immutable and cross-checking between item types can be done safely.
#[derive(Debug)]
pub struct Everything {
    /// Config from file
    config: Block,

    /// The global parser state, carrying information between files.
    /// Currently only used by the pdxfile parser, to handle the `reader_export` directory,
    /// which is specially processed before all other files.
    pub parser: ParserMemory,

    /// A cache of define values (from common/defines) that are missing and that have already been
    /// warned about as missing. This is to avoid duplicate warnings.
    warned_defines: RwLock<TigerHashSet<String>>,

    /// Tracks all the files (vanilla and mods) that are relevant to the current validation.
    pub(crate) fileset: Fileset,

    /// Tracks specifically the .dds files, and their formats and sizes.
    pub(crate) dds: DdsFiles,

    /// A general database of item types. Most items go here. The ones that need special handling
    /// go in the separate databases listed below.
    pub(crate) database: Db,

    pub(crate) localization: Localization,

    pub(crate) scripted_lists: ScriptedLists,

    pub(crate) defines: Defines,

    pub(crate) events: Events,

    pub(crate) scripted_modifiers: ScriptedModifiers,
    pub(crate) on_actions: OnActions,

    pub(crate) interaction_cats: CharacterInteractionCategories,

    pub(crate) provinces_ck3: Ck3Provinces,

    pub(crate) province_histories: ProvinceHistories,
    pub(crate) province_properties: ProvinceProperties,
    pub(crate) province_terrains: ProvinceTerrains,

    pub(crate) gameconcepts: GameConcepts,

    pub(crate) titles: Titles,

    pub(crate) characters: Characters,

    pub(crate) script_values: ScriptValues,

    pub(crate) triggers: Triggers,
    pub(crate) effects: Effects,

    pub(crate) traits: Traits,

    pub(crate) title_history: TitleHistories,

    pub(crate) menatarmstypes: MenAtArmsTypes,

    pub(crate) gui: Gui,
    pub(crate) data_bindings: DataBindings,

    pub(crate) assets: Assets,
    pub(crate) music: Musics,

    pub(crate) coas: Coas,

    pub(crate) wars: Wars,

    pub(crate) global_scopes: VariableScopes,
    pub(crate) global_list_scopes: VariableScopes,
    pub(crate) variable_scopes: VariableScopes,
    pub(crate) variable_list_scopes: VariableScopes,
}

macro_rules! load_all_generic {
    ($s: ident, $t: ident) => {
        $s.spawn(|_| $t.fileset.handle(&mut $t.dds, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.defines, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.triggers, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.effects, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.assets, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.gui, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.on_actions, &$t.parser));
    };
}

macro_rules! load_all_ck3 {
    ($s: ident, $t: ident) => {
        $s.spawn(|_| $t.fileset.handle(&mut $t.events, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.interaction_cats, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.province_histories, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.province_properties, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.province_terrains, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.gameconcepts, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.titles, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.characters, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.traits, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.title_history, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.menatarmstypes, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.music, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.data_bindings, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.provinces_ck3, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.scripted_lists, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.wars, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.coas, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.scripted_modifiers, &$t.parser));
        $s.spawn(|_| $t.fileset.handle(&mut $t.script_values, &$t.parser));
        $s.spawn(|_| crate::ck3::data::buildings::Building::finalize(&mut $t.database));
    };
}

impl Everything {
    /// Create a new `Everything` instance, ready for validating a mod.
    ///
    /// `vanilla_dir` is the path to the base game files. If it's `None`, then no vanilla files
    /// will be loaded. This will seriously affect validation, but it's ok if you just want to load
    /// and examine the mod files.
    ///
    /// `mod_root` is the path to the mod files. The config file will also be looked for there.
    ///
    /// `replace_paths` is from the similarly named field in the `.mod` file.
    pub fn new(
        config_filepath: Option<&Path>,
        vanilla_dir: Option<&Path>,
        workshop_dir: Option<&Path>,
        paradox_dir: Option<&Path>,
        mod_root: &Path,
        replace_paths: Vec<PathBuf>,
    ) -> Result<Self> {
        let mut fileset = Fileset::new(vanilla_dir, mod_root.to_path_buf(), replace_paths);

        let config_file_name = "ck3-tiger.conf";

        let config_file = match config_filepath {
            Some(path) => path.to_path_buf(),
            None => mod_root.join(config_file_name),
        };

        let config = if config_file.is_file() {
            Self::read_config(config_file_name, &config_file)
                .ok_or(FilesError::ConfigUnreadable { path: config_file })?
        } else {
            Block::new(Loc::for_file(
                config_file.clone(),
                FileStage::NoStage,
                FileKind::Mod,
                config_file.clone(),
            ))
        };

        fileset.config(config.clone(), workshop_dir, paradox_dir)?;

        fileset.scan_all()?;
        fileset.finalize();

        let global_scopes = VariableScopes::new("global_var:");
        let global_list_scopes = VariableScopes::new("global list ");
        let variable_scopes = VariableScopes::new("var:");
        let variable_list_scopes = VariableScopes::new("variable list ");

        variable_list_scopes
            .config_override("lover_object_of_importance", Scopes::Character | Scopes::Flag);
        variable_list_scopes
            .config_override("lover_object_of_importance_2", Scopes::Character | Scopes::Flag);
        variable_scopes.config_override("random_location", Scopes::Province | Scopes::Value);
        variable_scopes
            .config_override("task_contract_object", Scopes::Character | Scopes::Artifact);

        if let Some(block) = config.get_field_block("scope_override") {
            for (key, token) in block.iter_assignments() {
                let mut scopes = Scopes::empty();
                if token.lowercase_is("all") {
                    scopes = Scopes::all();
                } else {
                    for part in token.split('|') {
                        if let Some(scope) = Scopes::from_snake_case(part.as_str()) {
                            scopes |= scope;
                        } else {
                            let msg = format!("unknown scope type `{part}`");
                            warn(ErrorKey::Config).msg(msg).loc(part).push();
                        }
                    }
                }
                if let Some(name) = key.strip_prefix("var:") {
                    variable_scopes.config_override(name.as_str(), scopes);
                } else if let Some(name) = key.strip_prefix("var_list:") {
                    variable_list_scopes.config_override(name.as_str(), scopes);
                } else if let Some(name) = key.strip_prefix("global_var:") {
                    global_scopes.config_override(name.as_str(), scopes);
                } else if let Some(name) = key.strip_prefix("global_list:") {
                    global_list_scopes.config_override(name.as_str(), scopes);
                }
            }
        }

        Ok(Everything {
            parser: ParserMemory::default(),
            fileset,
            dds: DdsFiles::default(),
            config,
            warned_defines: RwLock::new(TigerHashSet::default()),
            database: Db::default(),
            localization: Localization::default(),
            scripted_lists: ScriptedLists::default(),
            defines: Defines::default(),
            events: Events::default(),
            scripted_modifiers: ScriptedModifiers::default(),
            on_actions: OnActions::default(),
            interaction_cats: CharacterInteractionCategories::default(),
            provinces_ck3: Ck3Provinces::default(),
            province_histories: ProvinceHistories::default(),
            province_properties: ProvinceProperties::default(),
            province_terrains: ProvinceTerrains::default(),
            gameconcepts: GameConcepts::default(),
            titles: Titles::default(),
            characters: Characters::default(),
            script_values: ScriptValues::default(),
            triggers: Triggers::default(),
            effects: Effects::default(),
            traits: Traits::default(),
            title_history: TitleHistories::default(),
            menatarmstypes: MenAtArmsTypes::default(),
            gui: Gui::default(),
            data_bindings: DataBindings::default(),
            assets: Assets::default(),
            music: Musics::default(),
            coas: Coas::default(),
            wars: Wars::default(),
            global_scopes,
            global_list_scopes,
            variable_scopes,
            variable_list_scopes,
        })
    }

    fn read_config(name: &str, path: &Path) -> Option<Block> {
        let entry =
            FileEntry::new(PathBuf::from(name), FileStage::NoStage, FileKind::Mod, path.to_owned());
        PdxFile::read_optional_bom(&entry, &ParserMemory::default())
    }

    pub fn load_config_filtering_rules(&self) {
        check_for_legacy_ignore(&self.config);
        load_filter(&self.config);
    }

    /// Load the `OutputStyle` settings from the config.
    /// Note that the settings from the config can still be overridden
    /// by supplying the --no-color flag.
    fn load_output_styles(&self, default_color: bool) -> OutputStyle {
        // Treat a missing output_style block and an empty output_style block exactly the same.
        let block = match self.config.get_field_block("output_style") {
            Some(block) => Cow::Borrowed(block),
            None => Cow::Owned(Block::new(self.config.loc)),
        };
        if !block.get_field_bool("enable").unwrap_or(default_color) {
            return OutputStyle::no_color();
        }
        let mut style = OutputStyle::default();
        for severity in Severity::iter() {
            if let Some(error_block) =
                block.get_field_block(format!("{severity}").to_ascii_lowercase().as_str())
                && let Some(color) = error_block.get_field_value("color")
            {
                style.set(severity, color.as_str());
            }
        }
        style
    }

    /// Report the mods that `modfile` says it depends on but that were not loaded with it.
    pub fn check_dependencies(&self, modfile: &ModFile) {
        modfile.check_dependencies(self.fileset.loaded_mod_names());
    }

    pub fn load_output_settings(&self, default_colors: bool) {
        set_output_style(self.load_output_styles(default_colors));
    }

    fn load_reader_export(&mut self) {
        let path = PathBuf::from("reader_export");
        for entry in self.fileset.get_files_under(&path) {
            if entry.filename().to_string_lossy().ends_with(".txt") {
                PdxFile::reader_export(entry, &mut self.parser.pdxfile);
            }
        }
    }

    fn load_pdx_files(&mut self, loader: &ItemLoader) {
        let path = PathBuf::from(loader.itype().path());
        let recursive = loader.recursive();
        let expect_count = path.components().count() + 1;
        for mut block in self.fileset.filter_map_under(&path, |entry| {
            // It's <= expect_count because some loader paths are files not directories
            if (recursive || entry.path().components().count() <= expect_count)
                && entry.filename().to_string_lossy().ends_with(loader.extension())
            {
                PdxFile::read_encoded(entry, loader.encoding(), &self.parser)
            } else {
                None
            }
        }) {
            if loader.whole_file() {
                let fname = block.loc.filename();
                // unwrap is safe here because of the ends_with check above.
                let key = fname.strip_suffix(loader.extension()).unwrap();
                let key = Token::new(key, block.loc);
                (loader.adder())(&mut self.database, key, block);
            } else {
                for (key, block) in block.drain_definitions_warn() {
                    (loader.adder())(&mut self.database, key, block);
                }
            }
        }
    }

    fn load_all_normal_pdx_files(&mut self) {
        for loader in inventory::iter::<ItemLoader> {
            self.load_pdx_files(loader);
        }
    }

    pub fn load_all(&mut self) {
        self.load_reader_export();
        self.load_all_normal_pdx_files();

        std::thread::scope(|s| {
            s.spawn(|| self.fileset.handle(&mut self.localization, &self.parser));

            scope(|s| {
                load_all_generic!(s, self);
                load_all_ck3!(s, self);
            });

            self.database.add_subitems();
        });
    }

    fn validate_all_generic<'a>(&'a self, s: &Scope<'a>) {
        s.spawn(|_| self.fileset.validate(self));
        s.spawn(|_| self.defines.validate(self));
        s.spawn(|_| self.triggers.validate(self));
        s.spawn(|_| self.effects.validate(self));
        s.spawn(|_| self.assets.validate(self));
        s.spawn(|_| self.gui.validate(self));
        s.spawn(|_| self.on_actions.validate(self));
        s.spawn(|_| self.dds.validate());
    }

    fn validate_all_ck3<'a>(&'a self, s: &Scope<'a>) {
        s.spawn(|_| self.events.validate(self));
        s.spawn(|_| self.interaction_cats.validate(self));
        s.spawn(|_| self.province_histories.validate(self));
        s.spawn(|_| self.province_properties.validate(self));
        s.spawn(|_| self.province_terrains.validate(self));
        s.spawn(|_| self.gameconcepts.validate(self));
        s.spawn(|_| self.titles.validate(self));
        s.spawn(|_| self.characters.validate(self));
        s.spawn(|_| self.traits.validate(self));
        s.spawn(|_| self.title_history.validate(self));
        s.spawn(|_| self.menatarmstypes.validate(self));
        s.spawn(|_| self.data_bindings.validate(self));
        s.spawn(|_| self.provinces_ck3.validate(self));
        s.spawn(|_| self.wars.validate(self));
        s.spawn(|_| self.coas.validate(self));
        s.spawn(|_| self.scripted_lists.validate(self));
        s.spawn(|_| self.scripted_modifiers.validate(self));
        s.spawn(|_| self.script_values.validate(self));
        s.spawn(|_| self.music.validate(self));
        s.spawn(|_| Climate::validate_all(&self.database, self));
    }

    pub fn validate_all(&self) {
        scope(|s| {
            self.validate_all_generic(s);
            self.validate_all_ck3(s);
            s.spawn(|_| self.database.validate(self));
        });
        self.events.validate_deferred(self);
        // Themes are called by events, and backgrounds and transitions by both.
        self.database.validate_deferred(Item::EventTheme, self);
        self.database.validate_deferred(Item::EventBackground, self);
        self.database.validate_deferred(Item::EventTransition, self);
        self.localization.validate_pass2(self);
        self.global_scopes.finalize();
        self.global_list_scopes.finalize();
        self.variable_scopes.finalize();
        self.variable_list_scopes.finalize();
    }

    pub fn check_rivers(&mut self) {
        let mut rivers = Rivers::default();
        self.fileset.handle(&mut rivers, &self.parser);
        rivers.validate(self);
    }

    pub fn check_pod(&mut self) {
        self.province_histories.check_pod_faiths(self, &self.titles);
        self.characters.check_pod_flags(self);
        self.localization.check_pod_loca(self);
    }

    pub fn check_unused(&mut self) {
        self.localization.check_unused(self);
        self.fileset.check_unused_dds(self);
    }

    #[allow(dead_code)]
    pub(crate) fn item_has_property(&self, itype: Item, key: &str, property: &str) -> bool {
        self.database.has_property(itype, key, property, self)
    }

    pub(crate) fn item_lc_has_property(
        &self,
        itype: Item,
        key: &Lowercase,
        property: &str,
    ) -> bool {
        self.database.lc_has_property(itype, key, property, self)
    }

    fn item_exists_ck3(&self, itype: Item, key: &str) -> bool {
        match itype {
            Item::ActivityState => ACTIVITY_STATES.contains(&key),
            Item::ArtifactHistory => ARTIFACT_HISTORY.contains(&key),
            Item::ArtifactRarity => ARTIFACT_RARITIES.contains(&&*key.to_ascii_lowercase()),
            Item::Character => self.characters.exists(key),
            Item::CharacterInteractionCategory => self.interaction_cats.exists(key),
            Item::Coa => self.coas.exists(key),
            Item::CoaTemplate => self.coas.template_exists(key),
            Item::Currency => CURRENCIES_CK3.contains(&key),
            Item::DangerType => DANGER_TYPES.contains(&key),
            Item::DlcFeature => DLC_FEATURES_CK3.contains(&key),
            Item::Event => self.events.exists(key),
            Item::EventNamespace => self.events.namespace_exists(key),
            Item::GameConcept => self.gameconcepts.exists(key),
            Item::GeneAttribute => self.assets.attribute_exists(key),
            Item::GeneticConstraint => self.traits.constraint_exists(key),
            Item::MenAtArms => self.menatarmstypes.exists(key),
            Item::MenAtArmsBase => self.menatarmstypes.base_exists(key),
            Item::Music => self.music.exists(key),
            Item::PrisonType => PRISON_TYPES.contains(&key),
            Item::Province => self.provinces_ck3.exists(key),
            Item::RewardItem => REWARD_ITEMS.contains(&key),
            Item::ScriptedList => self.scripted_lists.exists(key),
            Item::ScriptedModifier => self.scripted_modifiers.exists(key),
            Item::ScriptValue => self.script_values.exists(key),
            Item::Sexuality => SEXUALITIES.contains(&key),
            Item::Skill => SKILLS.contains(&key),
            Item::Sound => self.valid_sound(key),
            Item::Title => self.titles.exists(key),
            Item::TitleHistory => self.title_history.exists(key),
            Item::Trait => self.traits.exists(key),
            Item::TraitFlag => self.traits.flag_exists(key),
            Item::TraitTrack => self.traits.track_exists(key),
            Item::TraitCategory => TRAIT_CATEGORIES.contains(&key),
            _ => self.database.exists(itype, key),
        }
    }

    pub(crate) fn item_exists(&self, itype: Item, key: &str) -> bool {
        match itype {
            Item::Asset => self.assets.asset_exists(key),
            Item::BlendShape => self.assets.blend_shape_exists(key),
            Item::Define => self.defines.exists(key),
            Item::Entity => self.assets.entity_exists(key),
            Item::Entry => self.fileset.entry_exists(key),
            Item::File => self.fileset.exists(key),
            Item::GuiLayer => self.gui.layer_exists(key),
            Item::GuiTemplate => self.gui.template_exists(key),
            Item::GuiType => self.gui.type_exists(&Lowercase::new(key)),
            Item::Localization => self.localization.exists(key),
            Item::OnAction => self.on_actions.exists(key),
            Item::Pdxmesh => self.assets.mesh_exists(key),
            Item::ScriptedEffect => self.effects.exists(key),
            Item::ScriptedTrigger => self.triggers.exists(key),
            Item::TextFormat => self.gui.textformat_exists(key),
            Item::TextIcon => self.gui.texticon_exists(key),
            Item::TextureFile => self.assets.texture_exists(key),
            Item::WidgetName => self.gui.name_exists(key),
            Item::Directory | Item::Shortcut => true, // TODO
            // The game takes a faith icon from gfx/interface/icons/faith/<name>.dds, so a name
            // with such a texture is an icon even when no data file lists it.
            Item::FaithIcon => {
                self.item_exists_ck3(itype, key)
                    || self.fileset.exists(&format!("gfx/interface/icons/faith/{key}.dds"))
            }
            _ => self.item_exists_ck3(itype, key),
        }
    }

    /// Return true iff the item `key` is found with a case insensitive match.
    /// This function is **incomplete**. It only contains the item types for which case insensitive
    /// matches are needed; this is currently the ones used in `src/ck3/tables/modif.rs`.
    fn item_exists_lc_ck3(&self, itype: Item, key: &Lowercase) -> bool {
        match itype {
            Item::MenAtArmsBase => self.menatarmstypes.base_exists_lc(key),
            Item::Trait => self.traits.exists_lc(key),
            Item::TraitTrack => self.traits.track_exists_lc(key),
            _ => self.database.exists_lc(itype, key),
        }
    }

    /// Return true iff the item `key` is found with a case insensitive match.
    /// This function is **incomplete**. It only contains the item types for which case insensitive
    /// matches are needed; this is currently the ones used in modif lookups.
    pub(crate) fn item_exists_lc(&self, itype: Item, key: &Lowercase) -> bool {
        #[allow(clippy::match_single_binding)]
        match itype {
            _ => self.item_exists_lc_ck3(itype, key),
        }
    }

    pub(crate) fn mark_used(&self, itype: Item, key: &str) {
        match itype {
            Item::File => self.fileset.mark_used(key),
            Item::Localization => {
                self.localization.mark_used_return_exists(key);
            }
            _ => (),
        }
    }

    pub(crate) fn verify_exists(&self, itype: Item, token: &Token) {
        self.verify_exists_implied(itype, token.as_str(), token);
    }

    pub(crate) fn verify_exists_max_sev(&self, itype: Item, token: &Token, max_sev: Severity) {
        self.verify_exists_implied_max_sev(itype, token.as_str(), token, max_sev);
    }

    pub(crate) fn verify_exists_implied_max_sev(
        &self,
        itype: Item,
        key: &str,
        token: &Token,
        max_sev: Severity,
    ) {
        match itype {
            Item::Entry => self.fileset.verify_entry_exists(key, token, max_sev),
            Item::File => self.fileset.verify_exists_implied(key, token, max_sev),
            Item::Localization => self.localization.verify_exists_implied(key, token, max_sev),
            Item::Music => self.music.verify_exists_implied(key, token, max_sev),
            Item::Province => self.provinces_ck3.verify_exists_implied(key, token, max_sev),
            Item::TextureFile => {
                if let Some(entry) = self.assets.get_texture(key) {
                    // TODO: avoid allocating a string here
                    self.fileset.mark_used(&entry.path().to_string_lossy());
                } else {
                    let msg = format!("no texture file {key} anywhere under {}", itype.path());
                    report(ErrorKey::MissingFile, itype.severity().at_most(max_sev))
                        .conf(itype.confidence())
                        .msg(msg)
                        .loc(token)
                        .push();
                }
            }
            _ => {
                if !self.item_exists(itype, key) {
                    let path = itype.path();
                    let msg = if path.is_empty() {
                        format!("unknown {itype} {key}")
                    } else {
                        format!("{itype} {key} not defined in {path}")
                    };
                    report(ErrorKey::MissingItem, itype.severity().at_most(max_sev))
                        .conf(itype.confidence())
                        .msg(msg)
                        .opt_info(self.older_version_hint(itype, key))
                        .loc(token)
                        .push();
                }
            }
        }
    }

    /// A hint for a missing item that an older game version had. Mods written before 1.20 name as
    /// faiths what are now rites of one faith, ask for a tenet with `has_doctrine`, or use a
    /// trait that was renamed.
    fn older_version_hint(&self, itype: Item, key: &str) -> Option<String> {
        let (found, expected) = match itype {
            Item::Faith => (Item::Rite, "faith"),
            Item::Rite => (Item::Faith, "rite"),
            Item::Doctrine => {
                return self.item_exists(Item::Tenet, key).then(|| {
                    format!("`{key}` is a tenet, not a doctrine; use `has_tenet` or `add_tenet`")
                });
            }
            _ => {
                let hint = crate::ck3::tables::removed::removed_item_hint(itype, key);
                return hint.map(String::from);
            }
        };
        self.item_exists(found, key)
            .then(|| format!("`{key}` is defined as a {found}, but a {expected} is wanted here"))
    }

    #[allow(dead_code)]
    pub(crate) fn verify_exists_implied_max_sev_lc(
        &self,
        itype: Item,
        key: &Lowercase,
        token: &Token,
        max_sev: Severity,
    ) {
        if !self.item_exists_lc(itype, key) {
            let path = itype.path();
            let msg = if path.is_empty() {
                format!("unknown {itype} {key}")
            } else {
                format!("{itype} {key} not defined in {path}")
            };
            report(ErrorKey::MissingItem, itype.severity().at_most(max_sev))
                .conf(itype.confidence())
                .msg(msg)
                .loc(token)
                .push();
        }
    }

    pub(crate) fn verify_exists_implied(&self, itype: Item, key: &str, token: &Token) {
        self.verify_exists_implied_max_sev(itype, key, token, Severity::Error);
    }

    pub(crate) fn verify_icon(&self, define: &str, token: &Token, suffix: &str) {
        if let Some(icon_path) = self.get_defined_string_warn(token, define) {
            let pathname = format!("{icon_path}/{token}{suffix}");
            // It's `Severity::Warning` because a missing icon is only a UI issue.
            self.verify_exists_implied_max_sev(Item::File, &pathname, token, Severity::Warning);
        }
    }

    pub(crate) fn mark_used_icon(&self, define: &str, token: &Token, suffix: &str) {
        if let Some(icon_path) = self.get_defined_string_warn(token, define) {
            let pathname = format!("{icon_path}/{token}{suffix}");
            self.fileset.mark_used(&pathname);
        }
    }

    #[allow(dead_code)]
    pub(crate) fn validate_use(&self, itype: Item, key: &Token, block: &Block) {
        self.database.validate_use(itype, key, block, self);
    }

    #[allow(dead_code)]
    pub(crate) fn validate_call(
        &self,
        itype: Item,
        key: &Token,
        block: &Block,
        sc: &mut ScopeContext,
    ) {
        self.database.validate_call(itype, key, block, self, sc);
    }

    /// Validate the use of a localization within a specific `ScopeContext`.
    /// This allows validation of the named scopes used within the localization's datafunctions.
    pub(crate) fn validate_localization_sc(&self, key: &str, sc: &mut ScopeContext) {
        self.localization.validate_use(key, self, sc);
    }

    #[allow(dead_code)]
    pub(crate) fn get_item<T: DbKind>(
        &self,
        itype: Item,
        key: &str,
    ) -> Option<(&Token, &Block, &T)> {
        self.database.get_item(itype, key)
    }

    pub(crate) fn get_key_block(&self, itype: Item, key: &str) -> Option<(&Token, &Block)> {
        self.database.get_key_block(itype, key)
    }

    pub(crate) fn get_trigger(&self, key: &Token) -> Option<&Trigger> {
        if let Some(trigger) = self.triggers.get(key.as_str()) {
            return Some(trigger);
        }
        if let Some(trigger) = self.events.get_trigger(key) {
            return Some(trigger);
        }
        None
    }

    pub(crate) fn get_effect(&self, key: &Token) -> Option<&Effect> {
        if let Some(effect) = self.effects.get(key.as_str()) {
            return Some(effect);
        }
        if let Some(effect) = self.events.get_effect(key) {
            return Some(effect);
        }
        None
    }

    pub(crate) fn get_defined_string(&self, key: &str) -> Option<&Token> {
        self.defines.get_bv(key).and_then(BV::get_value)
    }

    pub(crate) fn get_defined_array(&self, key: &str) -> Option<&Block> {
        self.defines.get_bv(key).and_then(BV::get_block)
    }

    #[allow(clippy::missing_panics_doc)] // only panics on poisoned mutex
    pub(crate) fn get_defined_string_warn(&self, token: &Token, key: &str) -> Option<&Token> {
        let result = self.get_defined_string(key);
        if result.is_none() {
            let mut cache = self.warned_defines.write().unwrap();
            if !cache.contains(key) {
                let msg = format!("{key} not defined in common/defines/");
                err(ErrorKey::MissingItem).msg(msg).loc(token).push();
                cache.insert(key.to_string());
            }
        }
        result
    }

    #[allow(clippy::missing_panics_doc)] // only panics on poisoned mutex
    pub(crate) fn get_defined_array_warn(&self, token: &Token, key: &str) -> Option<&Block> {
        let result = self.get_defined_array(key);
        if result.is_none() {
            let mut cache = self.warned_defines.write().unwrap();
            if !cache.contains(key) {
                let msg = format!("{key} not defined in common/defines/");
                err(ErrorKey::MissingItem).msg(msg).loc(token).push();
                cache.insert(key.to_string());
            }
        }
        result
    }

    pub fn iter_keys_ck3<'a>(&'a self, itype: Item) -> Box<dyn Iterator<Item = &'a Token> + 'a> {
        match itype {
            Item::Coa => Box::new(self.coas.iter_keys()),
            Item::CoaTemplate => Box::new(self.coas.iter_template_keys()),
            Item::Character => Box::new(self.characters.iter_keys()),
            Item::CharacterInteractionCategory => Box::new(self.interaction_cats.iter_keys()),
            Item::Event => Box::new(self.events.iter_keys()),
            Item::EventNamespace => Box::new(self.events.iter_namespace_keys()),
            Item::GameConcept => Box::new(self.gameconcepts.iter_keys()),
            Item::GeneAttribute => Box::new(self.assets.iter_attribute_keys()),
            Item::GeneticConstraint => Box::new(self.traits.iter_constraint_keys()),
            Item::MenAtArms => Box::new(self.menatarmstypes.iter_keys()),
            Item::MenAtArmsBase => Box::new(self.menatarmstypes.iter_base_keys()),
            Item::Music => Box::new(self.music.iter_keys()),
            Item::Province => Box::new(self.provinces_ck3.iter_keys()),
            Item::ScriptedList => Box::new(self.scripted_lists.iter_keys()),
            Item::ScriptedModifier => Box::new(self.scripted_modifiers.iter_keys()),
            Item::ScriptValue => Box::new(self.script_values.iter_keys()),
            Item::Title => Box::new(self.titles.iter_keys()),
            Item::TitleHistory => Box::new(self.title_history.iter_keys()),
            Item::Trait => Box::new(self.traits.iter_keys()),
            Item::TraitFlag => Box::new(self.traits.iter_flag_keys()),
            Item::TraitTrack => Box::new(self.traits.iter_track_keys()),
            _ => Box::new(self.database.iter_keys(itype)),
        }
    }

    pub fn iter_keys<'a>(&'a self, itype: Item) -> Box<dyn Iterator<Item = &'a Token> + 'a> {
        match itype {
            Item::Asset => Box::new(self.assets.iter_asset_keys()),
            Item::BlendShape => Box::new(self.assets.iter_blend_shape_keys()),
            Item::Define => Box::new(self.defines.iter_keys()),
            Item::Entity => Box::new(self.assets.iter_entity_keys()),
            Item::File => Box::new(self.fileset.iter_keys()),
            Item::GuiLayer => Box::new(self.gui.iter_layer_keys()),
            Item::GuiTemplate => Box::new(self.gui.iter_template_keys()),
            Item::GuiType => Box::new(self.gui.iter_type_keys()),
            Item::Localization => Box::new(self.localization.iter_keys()),
            Item::OnAction => Box::new(self.on_actions.iter_keys()),
            Item::Pdxmesh => Box::new(self.assets.iter_mesh_keys()),
            Item::ScriptedEffect => Box::new(self.effects.iter_keys()),
            Item::ScriptedTrigger => Box::new(self.triggers.iter_keys()),
            Item::TextFormat => Box::new(self.gui.iter_textformat_keys()),
            Item::TextIcon => Box::new(self.gui.iter_texticon_keys()),
            Item::TextureFile => Box::new(self.assets.iter_texture_keys()),
            Item::WidgetName => Box::new(self.gui.iter_names()),
            _ => self.iter_keys_ck3(itype),
        }
    }

    fn valid_sound(&self, name: &str) -> bool {
        // TODO: verify that file:/ values work
        if let Some(filename) = name.strip_prefix("file:/") {
            self.fileset.exists(filename)
        } else {
            let sounds_set = &crate::ck3::tables::sounds::SOUNDS_SET;
            sounds_set.contains(&Lowercase::new(name))
        }
    }

    /// Return true iff a script value of the given name is defined.
    #[allow(clippy::unused_self)]
    pub(crate) fn script_value_exists(&self, name: &str) -> bool {
        self.script_values.exists(name)
    }

    pub(crate) fn event_check_scope(&self, id: &Token, sc: &mut ScopeContext) {
        self.events.check_scope(id, sc, self);
    }

    pub(crate) fn event_validate_call(&self, id: &Token, sc: &mut ScopeContext) {
        self.events.validate_call(id, self, sc);
    }
}

impl Drop for Everything {
    fn drop(&mut self) {
        // For the sake of the benchmark code, restore MACRO_MAP to a clean slate
        MACRO_MAP.clear();
    }
}

#[cfg(feature = "internal_benches")]
#[divan::bench_group(sample_count = 10)]
mod benchmark {
    use super::*;
    use crate::benches;
    use divan::{self, Bencher};

    #[divan::bench(args = benches::ck3::bench_mods())]
    fn load_provinces_ck3(bencher: Bencher, (vanilla_dir, modpath): (&str, &PathBuf)) {
        bencher
            .with_inputs(|| {
                Everything::new(None, Some(Path::new(vanilla_dir)), None, None, modpath, vec![])
                    .unwrap()
            })
            .bench_local_refs(|everything| {
                everything.fileset.handle(&mut everything.provinces_ck3, &everything.parser);
            });
    }

    #[divan::bench(args = benches::bench_mods())]
    fn load_localization(bencher: Bencher, (vanilla_dir, modpath): (&str, &PathBuf)) {
        bencher
            .with_inputs(|| {
                Everything::new(None, Some(Path::new(vanilla_dir)), None, None, modpath, vec![])
                    .unwrap()
            })
            .bench_local_refs(|everything| {
                everything.fileset.handle(&mut everything.localization, &everything.parser);
            });
    }
}
