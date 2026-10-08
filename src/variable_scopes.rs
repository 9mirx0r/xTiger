use std::sync::Mutex;

use scc::HashMap as SccHashMap;

use crate::report::{ErrorKey, warn};
use crate::scopes::Scopes;
use crate::token::Token;

/// A registry of what is known about the scope types of variables.
/// There will be one registry per namespace (globals, global lists, variables, variable lists).
///
/// Validation runs in parallel, so the order in which uses of a variable are seen differs from
/// run to run. To keep the output deterministic, uses are only recorded while validating and
/// are checked against each other afterwards, in file order, by [`VariableScopes::finalize`].
/// While validating, a variable is assumed to hold any scope type unless it was configured.
#[derive(Debug)]
pub struct VariableScopes {
    /// The string to be used when reporting scope conflicts to the user.
    namespace: &'static str,
    /// Scope types set in configuration. These are known before validation starts.
    overrides: SccHashMap<&'static str, Scopes>,
    /// Every scope type expectation seen during validation, in arbitrary order.
    observed: Mutex<Vec<(&'static str, Token, Scopes)>>,
}

impl VariableScopes {
    pub fn new(namespace: &'static str) -> Self {
        Self { namespace, overrides: SccHashMap::default(), observed: Mutex::default() }
    }

    #[allow(dead_code)]
    pub fn config_override(&self, name: &'static str, scopes: Scopes) {
        self.overrides.upsert_sync(name, scopes);
    }

    pub fn scopes(&self, name: &str) -> Scopes {
        self.overrides.read_sync(name, |_, s| *s).unwrap_or(Scopes::all())
    }

    pub fn expect(&self, name: &'static str, token: &Token, scopes: Scopes) {
        if let Some(configured) = self.overrides.read_sync(name, |_, s| *s) {
            if !configured.intersects(scopes) {
                self.conflict(name, token, "configured", configured, scopes);
            }
            return;
        }
        self.observed.lock().unwrap().push((name, token.clone(), scopes));
    }

    /// Check the recorded uses of each variable against each other. The first use in file order
    /// sets the expected scope types, later uses narrow it down, and a use that cannot match is
    /// reported.
    pub fn finalize(&self) {
        let mut observed = std::mem::take(&mut *self.observed.lock().unwrap());
        observed.sort_by(|(n1, t1, s1), (n2, t2, s2)| {
            n1.cmp(n2).then_with(|| t1.loc.stable_cmp(t2.loc)).then(s1.bits().cmp(&s2.bits()))
        });
        let mut current: Option<(&str, Scopes)> = None;
        for (name, token, scopes) in observed {
            match current {
                Some((cur_name, ref mut s)) if cur_name == name => {
                    if s.intersects(scopes) {
                        *s &= scopes;
                    } else {
                        self.conflict(name, &token, "deduced", *s, scopes);
                    }
                }
                _ => current = Some((name, scopes)),
            }
        }
    }

    fn conflict(&self, name: &str, token: &Token, verb: &str, known: Scopes, scopes: Scopes) {
        let msg = format!(
            "{}{name} was {verb} to be {known} but scope seems to be {scopes}",
            self.namespace
        );
        warn(ErrorKey::Scopes).weak().msg(msg).loc(token).push();
    }
}
