//! [`DeferredCalls`] to replay calls made during parallel validation in a fixed order.

use std::sync::Mutex;

use crate::context::ScopeContext;
use crate::everything::Everything;
use crate::token::Token;

/// Some items are validated once for each kind of [`ScopeContext`] they are called with, and the
/// first call with a given kind decides which locations the reports point to. To keep that
/// independent of thread scheduling, the calls are recorded during validation and replayed
/// afterwards, sorted by call site.
#[derive(Debug, Default)]
pub struct DeferredCalls {
    calls: Mutex<Vec<(Token, ScopeContext)>>,
}

impl DeferredCalls {
    pub fn push(&self, caller: &Token, sc: &ScopeContext) {
        self.calls.lock().unwrap().push((caller.clone(), sc.clone()));
    }

    /// Take the recorded calls, sorted by call site and then by scope context.
    pub fn take_sorted(&self, data: &Everything) -> Vec<(Token, ScopeContext)> {
        let calls = std::mem::take(&mut *self.calls.lock().unwrap());
        let mut calls: Vec<_> =
            calls.into_iter().map(|(t, sc)| (format!("{:?}", sc.signature(data)), t, sc)).collect();
        calls.sort_by(|(s1, t1, sc1), (s2, t2, sc2)| {
            t1.loc
                .stable_cmp(t2.loc)
                .then_with(|| s1.cmp(s2))
                .then_with(|| sc1.stable_cmp_reasons(sc2))
        });
        calls.into_iter().map(|(_, t, sc)| (t, sc)).collect()
    }
}
