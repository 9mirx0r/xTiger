use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Mutex;

use rayon::prelude::*;

use crate::block::{Block, BlockItem, Field};
use crate::context::{Reason, ScopeContext, Signature};
use crate::data::scripted_effects::Effect;
use crate::data::scripted_triggers::Trigger;
use crate::deferred::DeferredCalls;
use crate::everything::Everything;
use crate::fileset::{FileEntry, FileHandler};
use crate::helpers::{TigerHashMap, TigerHashSet, dup_error};
use crate::item::Item;
use crate::parse::ParserMemory;
use crate::pathtable::PathTableIndex;
use crate::pdxfile::PdxFile;
use crate::report::{ErrorKey, err, warn};
use crate::scopes::Scopes;
use crate::token::Token;

#[derive(Debug, Default)]
#[allow(clippy::struct_field_names)]
pub struct Events {
    events: TigerHashMap<(&'static str, u16), Event>,
    namespaces: TigerHashSet<Token>,
    triggers: TigerHashMap<(PathTableIndex, &'static str), Trigger>,
    effects: TigerHashMap<(PathTableIndex, &'static str), Effect>,
    /// Calls to events from other script, validated after everything else.
    calls: DeferredCalls,
}

impl Events {
    fn load_event(&mut self, key: Token, block: Block) {
        if let Some((key_a, key_b)) = key.as_str().split_once('.')
            && let Ok(id) = u16::from_str(key_b)
        {
            if let Some(other) = self.get_event(key.as_str()) {
                #[allow(clippy::redundant_else)]
                {
                    // In the other games, overriding events is always an error.
                    dup_error(&key, &other.key, "event");
                }
            }
            self.events.insert((key_a, id), Event::new(key, block));
            return;
        }
        let msg = "Event names should be in the form NAMESPACE.NUMBER";
        let info = "where NAMESPACE is the namespace declared at the top of the file, and NUMBER is a series of up to 4 digits.";
        warn(ErrorKey::EventNamespace).msg(msg).info(info).loc(key).push();
    }

    fn load_scripted_trigger(&mut self, key: Token, block: Block) {
        let index = (key.loc.idx, key.as_str());
        if let Some(other) = self.triggers.get(&index) {
            dup_error(&key, &other.key, "scripted trigger");
        }
        self.triggers.insert(index, Trigger::new(key, block, None));
    }

    fn load_scripted_effect(&mut self, key: Token, block: Block) {
        let index = (key.loc.idx, key.as_str());
        if let Some(other) = self.effects.get(&index) {
            dup_error(&key, &other.key, "scripted effect");
        }
        self.effects.insert(index, Effect::new(key, block, None));
    }

    pub fn get_trigger(&self, key: &Token) -> Option<&Trigger> {
        let index = (key.loc.idx, key.as_str());
        self.triggers.get(&index)
    }

    pub fn get_effect(&self, key: &Token) -> Option<&Effect> {
        let index = (key.loc.idx, key.as_str());
        self.effects.get(&index)
    }

    fn get_event<'a>(&'a self, key: &'a str) -> Option<&'a Event> {
        if let Some((namespace, id)) = key.split_once('.')
            && let Ok(id) = u16::from_str(id)
        {
            return self.events.get(&(namespace, id));
        }
        None
    }

    pub fn check_scope(&self, token: &Token, sc: &mut ScopeContext, data: &Everything) {
        if let Some(event) = self.get_event(token.as_str()) {
            sc.expect(event.expects_scope, &Reason::Token(token.clone()), data);
        }
    }

    pub fn namespace_exists(&self, key: &str) -> bool {
        self.namespaces.contains(key)
    }

    pub fn iter_namespace_keys(&self) -> impl Iterator<Item = &Token> {
        self.namespaces.iter()
    }

    pub fn exists(&self, key: &str) -> bool {
        if let Some((namespace, id)) = key.split_once('.')
            && let Ok(id) = u16::from_str(id)
            && self.events.contains_key(&(namespace, id))
        {
            return true;
        }
        false
    }

    pub fn iter_keys(&self) -> impl Iterator<Item = &Token> {
        self.events.values().map(|item| &item.key)
    }

    pub fn validate(&self, data: &Everything) {
        for item in self.effects.values() {
            item.validate(data);
        }

        for item in self.triggers.values() {
            item.validate(data);
        }

        self.events.par_iter().for_each(|(_, item)| {
            item.validate(data);
        });
    }

    pub fn validate_call(&self, key: &Token, _data: &Everything, sc: &mut ScopeContext) {
        if self.get_event(key.as_str()).is_some() {
            self.calls.push(key, sc);
        }
    }

    /// Validate the calls recorded by `validate_call`, including the calls those make in turn.
    /// The calls to each event are handled in order of call site, so that the first call with
    /// each kind of scope context is always the same one.
    pub fn validate_deferred(&self, data: &Everything) {
        loop {
            let calls = self.calls.take_sorted(data);
            if calls.is_empty() {
                break;
            }
            let mut by_event: TigerHashMap<&str, Vec<ScopeContext>> = TigerHashMap::default();
            for (key, sc) in calls {
                by_event.entry(key.as_str()).or_default().push(sc);
            }
            by_event.into_par_iter().for_each(|(key, calls)| {
                if let Some(event) = self.get_event(key) {
                    for mut sc in calls {
                        event.validate_call(data, &mut sc);
                    }
                }
            });
        }
    }
}

impl FileHandler<Block> for Events {
    fn subpath(&self) -> PathBuf {
        PathBuf::from("events")
    }

    fn load_file(&self, entry: &FileEntry, parser: &ParserMemory) -> Option<Block> {
        if !entry.filename().to_string_lossy().ends_with(".txt") {
            return None;
        }

        PdxFile::read(entry, parser)
    }

    fn handle_file(&mut self, _entry: &FileEntry, mut block: Block) {
        #[derive(Copy, Clone)]
        enum Expecting {
            Event,
            ScriptedTrigger,
            ScriptedEffect,
        }

        let mut expecting = Expecting::Event;

        for item in block.drain() {
            if let BlockItem::Field(Field(key, _, bv)) = item {
                if key.is("namespace") {
                    if let Some(value) = bv.expect_into_value() {
                        self.namespaces.insert(value);
                    }
                } else if key.is("scripted_trigger") || key.is("scripted_effect") {
                    let msg = format!("`{key}` should be used without `=`");
                    err(ErrorKey::ParseError).msg(msg).loc(key).push();
                } else if let Some(block) = bv.into_block() {
                    match expecting {
                        Expecting::ScriptedTrigger => {
                            self.load_scripted_trigger(key, block);
                            expecting = Expecting::Event;
                        }
                        Expecting::ScriptedEffect => {
                            self.load_scripted_effect(key, block);
                            expecting = Expecting::Event;
                        }
                        Expecting::Event => {
                            self.load_event(key, block);
                        }
                    }
                } else {
                    let msg = "unknown setting in event file";
                    err(ErrorKey::UnknownField).msg(msg).loc(key).push();
                }
            } else if let Some(key) = item.expect_value() {
                if matches!(expecting, Expecting::Event) && key.is("scripted_trigger") {
                    expecting = Expecting::ScriptedTrigger;
                } else if matches!(expecting, Expecting::Event) && key.is("scripted_effect") {
                    expecting = Expecting::ScriptedEffect;
                } else {
                    err(ErrorKey::Validation)
                        .msg("unexpected token")
                        .info("Did you forget an = ?")
                        .loc(key)
                        .push();
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct Event {
    pub key: Token,
    pub block: Block,
    expects_scope: Scopes,
    expects_from_token: Token,
    visited: Mutex<TigerHashSet<Signature>>,
}

impl Event {
    pub fn new(key: Token, block: Block) -> Self {
        let (expects_scope, expects_from_token) = crate::ck3::events::get_event_scope(&key, &block);
        let visited = Mutex::new(TigerHashSet::default());
        Self { key, block, expects_scope, expects_from_token, visited }
    }

    pub fn validate(&self, data: &Everything) {
        if let Some((namespace, _)) = self.key.as_str().split_once('.')
            && !data.item_exists(Item::EventNamespace, namespace)
        {
            let msg = format!("event file should start with `namespace = {namespace}`");
            let info = "otherwise the event won't be found in-game";
            err(ErrorKey::EventNamespace).msg(msg).info(info).loc(&self.key).push();
        }

        let mut sc = ScopeContext::new(self.expects_scope, &self.expects_from_token);
        sc.set_strict_scopes(false);
        sc.set_source(&self.key);

        crate::ck3::events::validate_event(self, data, &mut sc);
    }

    pub fn validate_call(&self, data: &Everything, sc: &mut ScopeContext) {
        if !self.visited.lock().unwrap().insert(sc.signature(data)) {
            // The event was already visited with an equivalent sc
            return;
        }
        crate::ck3::events::validate_event(self, data, sc);
    }
}
