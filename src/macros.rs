//! [`MacroCache`] to cache macro expansions, and [`MacroMap`] to track [`Loc`] use across macro expansions.

use std::cell::{Cell, RefCell};
use std::hash::Hash;
use std::num::NonZeroU32;
use std::sync::{LazyLock, RwLock};
use std::thread::{ThreadId, current};

use crate::helpers::{BiTigerHashMap, TigerHashMap};
use crate::token::{Loc, Token};
use crate::tooltipped::Tooltipped;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct MacroKey {
    /// the loc of the call site
    loc: Loc,
    /// lexically sorted macro arguments
    args: Vec<(&'static str, &'static str)>,
    tooltipped: Tooltipped,
    /// only for triggers
    negated: bool,
}

impl MacroKey {
    pub fn new(
        mut loc: Loc,
        args: &[(&'static str, Token)],
        tooltipped: Tooltipped,
        negated: bool,
    ) -> Self {
        loc.link_idx = None;
        let mut args: Vec<_> = args.iter().map(|(parm, arg)| (*parm, arg.as_str())).collect();
        args.sort_unstable();
        Self { loc, args, tooltipped, negated }
    }
}

/// A cache of validation results, keyed by call site.
///
/// While a result is being computed, a placeholder is visible only to the thread computing it, so
/// that recursive calls terminate. Other threads compute the result themselves instead of seeing
/// the placeholder.
///
/// A result that was computed using the placeholder of an enclosing computation depends on where
/// the recursion was entered, so it is not cached. This keeps the cached results, and thus the
/// reports, independent of validation order and thread scheduling.
#[derive(Debug)]
pub struct ValidationCache<K, T> {
    done: RwLock<TigerHashMap<K, T>>,
    /// Placeholders with the nesting depth at which they were created.
    pending: RwLock<TigerHashMap<(K, ThreadId), (T, usize)>>,
}

thread_local! {
    /// How many cached computations are in progress on this thread, across all caches.
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    /// The shallowest depth of a placeholder used by the current computation.
    static MIN_PENDING_USED: Cell<usize> = const { Cell::new(usize::MAX) };
    /// The saved `MIN_PENDING_USED` values of the enclosing computations.
    static PARENT_MIN: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

impl<K: Clone + Eq + Hash, T> ValidationCache<K, T> {
    pub fn perform<F: FnOnce(&T)>(&self, key: &K, f: F) -> bool {
        if let Some(x) = self.done.read().unwrap().get(key) {
            f(x);
            return true;
        }
        if let Some((x, depth)) = self.pending.read().unwrap().get(&(key.clone(), current().id())) {
            MIN_PENDING_USED.set(MIN_PENDING_USED.get().min(*depth));
            f(x);
            return true;
        }
        false
    }

    /// Store a placeholder result that only the current thread will see, and start a computation.
    /// Must be followed by a call to `insert` with the same key.
    pub fn insert_pending(&self, key: K, value: T) {
        let depth = DEPTH.get() + 1;
        DEPTH.set(depth);
        PARENT_MIN.with_borrow_mut(|v| v.push(MIN_PENDING_USED.replace(usize::MAX)));
        self.pending.write().unwrap().insert((key, current().id()), (value, depth));
    }

    /// Finish a computation started with `insert_pending`.
    pub fn insert(&self, key: K, value: T) {
        let depth = DEPTH.get();
        DEPTH.set(depth - 1);
        self.pending.write().unwrap().remove(&(key.clone(), current().id()));
        let used = MIN_PENDING_USED.get();
        let parent = PARENT_MIN.with_borrow_mut(Vec::pop).unwrap_or(usize::MAX);
        MIN_PENDING_USED.set(parent.min(if used < depth { used } else { usize::MAX }));
        if used >= depth {
            self.done.write().unwrap().insert(key, value);
        }
    }
}

impl<K, T> Default for ValidationCache<K, T> {
    fn default() -> Self {
        Self { done: RwLock::default(), pending: RwLock::default() }
    }
}

#[derive(Debug)]
/// A helper for scripted effects, triggers, and modifiers, all of which can
/// accept macro arguments and which need to be expanded for every macro call.
///
/// The cache helps avoid needless re-expansions for arguments that have already been validated.
pub struct MacroCache<T> {
    cache: ValidationCache<MacroKey, T>,
}

impl<T> MacroCache<T> {
    pub fn perform<F: FnOnce(&T)>(
        &self,
        key: &Token,
        args: &[(&'static str, Token)],
        tooltipped: Tooltipped,
        negated: bool,
        f: F,
    ) -> bool {
        self.cache.perform(&MacroKey::new(key.loc, args, tooltipped, negated), f)
    }

    /// Store a placeholder result, to be used by recursive calls while validating.
    pub fn insert_pending(
        &self,
        key: &Token,
        args: &[(&'static str, Token)],
        tooltipped: Tooltipped,
        negated: bool,
        value: T,
    ) {
        self.cache.insert_pending(MacroKey::new(key.loc, args, tooltipped, negated), value);
    }

    pub fn insert(
        &self,
        key: &Token,
        args: &[(&'static str, Token)],
        tooltipped: Tooltipped,
        negated: bool,
        value: T,
    ) {
        self.cache.insert(MacroKey::new(key.loc, args, tooltipped, negated), value);
    }
}

impl<T> Default for MacroCache<T> {
    fn default() -> Self {
        MacroCache { cache: ValidationCache::default() }
    }
}

/// Global macro map
pub(crate) static MACRO_MAP: LazyLock<MacroMap> = LazyLock::new(MacroMap::default);

#[derive(Default)]
pub struct MacroMap(RwLock<MacroMapInner>);

/// A bijective map storing the link index and the associated loc denoting the key
/// to the block containing the macros.
pub struct MacroMapInner {
    counter: NonZeroU32,
    bi_map: BiTigerHashMap<NonZeroU32, Loc>,
}

impl Default for MacroMapInner {
    fn default() -> Self {
        Self { counter: NonZeroU32::new(1).unwrap(), bi_map: BiTigerHashMap::default() }
    }
}

impl MacroMap {
    /// Get the loc associated with the index
    pub fn get_loc(&self, index: MacroMapIndex) -> Option<Loc> {
        self.0.read().unwrap().bi_map.get_by_left(&index.0).copied()
    }
    /// Get the index associated with the loc
    pub fn get_index(&self, loc: Loc) -> Option<MacroMapIndex> {
        self.0.read().unwrap().bi_map.get_by_right(&loc).copied().map(MacroMapIndex)
    }

    /// Insert a loc that is not expected to be in the map yet, and return its index.
    pub fn insert_or_get_loc(&self, loc: Loc) -> MacroMapIndex {
        let mut guard = self.0.write().unwrap();
        let counter = guard.counter;
        if guard.bi_map.insert_no_overwrite(counter, loc).is_err() {
            // The loc was already in the map. (The counter is always unique so that side can't have collided.)
            return guard.bi_map.get_by_right(&loc).copied().map(MacroMapIndex).unwrap();
        }
        guard.counter =
            guard.counter.checked_add(1).expect("internal error: 2^32 macro map entries");
        MacroMapIndex(counter)
    }

    /// Get the index of a loc, inserting it if it was not yet stored
    pub fn get_or_insert_loc(&self, loc: Loc) -> MacroMapIndex {
        // First try with just a read lock, which allows for more parallelism than using a write lock.
        if let Some(index) = self.get_index(loc) {
            index
        } else {
            // We need a write lock.
            self.insert_or_get_loc(loc)
        }
    }

    /// Clear all entries. This will break all existing `MacroMapIndex` values!
    pub(crate) fn clear(&self) {
        let mut guard = self.0.write().unwrap();
        guard.counter = NonZeroU32::new(1).unwrap();
        guard.bi_map.clear();
    }
}

/// Type-safety wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MacroMapIndex(NonZeroU32);
