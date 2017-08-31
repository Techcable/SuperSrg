///! Available mappings targets supersrg can generate
///! supersrg supports any combination of the following:
///! - `srg` - MCP's unique srg mappings, which are the same for each minecraft version.
///! - `mcp` - MCP's crowd sourced deobfuscated mappings, fetched from `MCPBot`
///!   - These have a independent version based on the date, which must be specified as an option.
///! - `spigot` - Spigot's deobfuscation mappings, held in the `BuildData` git repo
///!   - These are significantly lower quality than the MCP mappings, and most member names are still obfuscated
///!   - These mappings don't change very often, since plugins use them and would break if the change
///!   - Therefore the latest available mappings for the minecraft version are fetched by default, though this can be configured
///! - `obf` - The obfuscated mojang names, which are used to unify the mappings systems
///!
///! Mapping targets take the form `{original}2{renamed}` with an optional modifier at the end.
///! For example, `spigot2mcp` specifies mappings from the spigot names into the MCP names.
///! Three modifiers are supported:
///! - `classes` - Restricts the mappings to just class names.
///! - `members` - Restricts the mappings to just member names.
///! - `onlyobf` - Restricts the mappings to just names that are still obfuscated.
///!   - This allows you to take advantage of other mappings, without changing names that are already deobfuscated.
///!   - The motivating example is `spigot2mcp-onlyobf`, which would take advantage of the MCP mappings
///!     without changing names spigot already deobfuscated.
///!   - This is helpful since people often become familiar with and prefer a particular naming scheme (like spigot),
///!     but still want to take advantage of the additional naming information.
use std::str::FromStr;
use std::fmt::{self, Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::mem;

use parking_lot::{RwLock, Mutex, Condvar};
use regex::Regex;
use crossbeam::sync::MsQueue;
use ordermap::Entry;
use chashmap::CHashMap;

use mappings::{MappingsBuilder, MappingsSnapshot, MappingsIterator, Mappings};
use super::{MinecraftMappingsCache, MinecraftMappingError};
use utils::SeaHashOrderMap;

pub struct MappingsTargetComputerBuilder {
    cache: MinecraftMappingsCache,
    minecraft_version: String,
    mcp_version: Option<String>,
    refresh_spigot: bool,
    builddata_commit: Option<String>,
    initial_targets: Vec<MappingsTarget>,
}
impl MappingsTargetComputerBuilder {
    #[inline]
    pub fn new(cache: MinecraftMappingsCache, minecraft_version: String) -> Self {
        MappingsTargetComputerBuilder {
            cache,
            minecraft_version,
            mcp_version: None,
            refresh_spigot: false,
            builddata_commit: None,
            initial_targets: Vec::new(),
        }
    }
    #[inline]
    pub fn mcp_version(&mut self, mcp_version: String) -> &mut Self {
        self.mcp_version = Some(mcp_version);
        self
    }
    #[inline]
    pub fn builddata_commit(&mut self, builddata_commit: String) -> &mut Self {
        self.builddata_commit = Some(builddata_commit);
        self
    }
    #[inline]
    pub fn refresh_spigot(&mut self) -> &mut Self {
        self.refresh_spigot = true;
        self
    }
    #[inline]
    pub fn targets(&mut self, targets: &[MappingsTarget]) -> &mut Self {
        self.initial_targets.extend_from_slice(targets);
        self
    }
    pub fn build(&self) -> MappingsTargetComputer {
        let result = MappingsTargetComputer {
            cache: &self.cache,
            minecraft_version: self.minecraft_version.clone(),
            mcp_version: self.mcp_version.clone(),
            results: Default::default(),
            remaining_targets: MsQueue::new(),
            waiting_targets: Default::default(),
            waiters: Default::default(),
            done: AtomicBool::new(false),
            failed: AtomicBool::new(false),
            running_workers: Mutex::new(0),
            work_cond: Condvar::new(),
        };
        {
            let mut waiters = result.waiters.write();
            for target in &self.initial_targets {
                result.remaining_targets.push(*target);
                // Insert an empty vec so they don't get computed again
                waiters.insert(*target, vec![]);
            }
        }
        result
    }
}
pub struct MappingsTargetComputer<'a> {
    pub cache: &'a MinecraftMappingsCache,
    minecraft_version: String,
    mcp_version: Option<String>,
    results: RwLock<SeaHashOrderMap<MappingsTarget, MappingsSnapshot>>,
    remaining_targets: MsQueue<MappingsTarget>,
    waiters: RwLock<SeaHashOrderMap<MappingsTarget, Vec<MappingsTarget>>>,
    waiting_targets: CHashMap<MappingsTarget, WaitingTarget>,
    done: AtomicBool,
    failed: AtomicBool,
    running_workers: Mutex<usize>,
    work_cond: Condvar,
}
#[derive(Debug, Clone)]
struct WaitingTarget {
    target: MappingsTarget,
    dependencies: SeaHashOrderMap<MappingsTarget, ()>,
}
impl<'a> MappingsTargetComputer<'a> {
    pub fn compute_target_work(&self) -> Result<(), MinecraftMappingError> {
        {
            let mut lock = self.running_workers.lock();
            assert!(!self.done.load(Ordering::SeqCst), "Already done!");
            *lock += 1;
        }
        loop {
            let target: MappingsTarget;
            match self.remaining_targets.try_pop() {
                Some(t) => target = t,
                None => {
                    loop {

                        let mut lock = self.running_workers.lock();
                        if self.done.load(Ordering::SeqCst) {
                            assert!(
                                self.remaining_targets.try_pop().is_none(),
                                "Marked as done with remaining work!"
                            );
                            return Ok(());
                        }

                        // Now that we have the lock, check again if we have more work
                        if let Some(t) = self.remaining_targets.try_pop() {
                            target = t;
                            mem::drop(lock);
                            break;
                        }
                        *lock = lock.checked_sub(1).unwrap();
                        if *lock == 0 {
                            /*
                             * When the last thread finishes its work, we are done,
                             * and we need to notify all other threads that to wake them up.
                             * We also need to set the result
                             */
                            self.done.store(true, Ordering::SeqCst);
                            self.work_cond.notify_all();
                            return Ok(());
                        }
                        // Now sleep until we receive a notification that something has changed
                        self.work_cond.wait(&mut lock);
                        // Now increment the worker count since we woke up
                        *lock += 1;
                    }
                }
            }
            match self.try_compute_target(&target) {
                Ok(result) => {
                    let mut lock = self.waiters.write();
                    let mut results = self.results.write();
                    results.insert(target, result);
                    mem::drop(results);
                    if let Some(waiters) = lock.remove(&target) {
                        mem::drop(lock);
                        // Someone was waiting on our result, so add them to the queue if we're their final dependency
                        for waiting_target in &waiters {
                            let mut waiter = self.waiting_targets.get_mut(waiting_target).unwrap();
                            assert!(
                                waiter.dependencies.remove(&target).is_some(),
                                "{} wasn't a dependency of {}",
                                target,
                                waiting_target
                            );
                            if waiter.dependencies.is_empty() {
                                self.remaining_targets.push(*waiting_target);
                                trace!("Queued {} since {} was finished", waiting_target, target);
                            }
                        }
                        if !waiters.is_empty() {
                            // Notify any waiting threads that we have more work
                            self.work_cond.notify_all();
                        }
                    }
                }
                Err(e) => {
                    match e {
                        TargetComputeError::WaitingFor(dependencies) => {
                            let mut lock = self.waiters.write();
                            #[cfg(debug_assertions)]
                            let results = self.results.read();
                            trace!(
                                "{} waiting for [{}]",
                                target,
                                dependencies
                                    .iter()
                                    .map(MappingsTarget::to_string)
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                            let mut waiting_target = match self.waiting_targets.get_mut(&target) {
                                Some(waiting) => waiting,
                                None => {
                                    self.waiting_targets.alter(target, |old| {
                                        if old.is_some() {
                                            // Somone else got there first
                                            return old;
                                        }
                                        Some(WaitingTarget {
                                            target,
                                            dependencies: Default::default(),
                                        })
                                    });
                                    self.waiting_targets.get_mut(&target).unwrap()
                                }
                            };
                            for dependency in dependencies {
                                if waiting_target.dependencies.insert(dependency, ()).is_some() {
                                    panic!("{} already waited for {}", target, dependency)
                                }
                                let mut waiters = match lock.entry(dependency) {
                                    Entry::Occupied(occupied) => {
                                        // NOTE: Even if the vec is empty, the fact that it's present indicates we queued it before
                                        occupied.into_mut()
                                    }
                                    Entry::Vacant(vacant) => {
                                        #[cfg(debug_assertions)]
                                        debug_assert!(
                                            !results.contains_key(&dependency),
                                            "Already computed: {}",
                                            dependency
                                        );
                                        self.remaining_targets.push(dependency);
                                        trace!("Queued {} for {}", dependency, target);
                                        vacant.insert(Vec::new())
                                    }
                                };
                                waiters.push(target);
                            }
                        }
                        TargetComputeError::MappingError(cause) => {
                            /// When one thread fails, all other threads must stop working and exit cleanly
                            let _ = self.running_workers.lock();
                            if self.failed.compare_and_swap(false, true, Ordering::SeqCst) {
                                // Someone else failed first, so prefer their Error
                                return Ok(())
                            }
                            let was_done = self.done.swap(true, Ordering::SeqCst);
                            assert!(!was_done);
                            /// Notify any sleeping threads that we're done
                            self.work_cond.notify_all();
                            return Err(cause);
                        }
                    }
                }
            }
        }
    }
    fn try_compute_target(&self, target: &MappingsTarget) -> Result<MappingsSnapshot, TargetComputeError> {
        use self::MappingsFormat::*;
        if let Some(modifier) = target.modifier {
            let mut dependency_targets = Vec::with_capacity(2);
            if modifier == TargetModifier::Onlyobf {
                dependency_targets.push(MappingsTarget::new(target.original, Obf));
            }
            dependency_targets.push(MappingsTarget::new(target.original, target.renamed));
            let mut results = self.try_take(&dependency_targets)?;
            let unmodified = results.pop().unwrap();
            return Ok(match modifier {
                TargetModifier::Onlyobf => {
                    if target.original == Obf {
                        // If the original is obfuscated, the modifier is redundant and we should just use the unmodified version
                        unmodified
                    } else {
                        let original2obf = results.pop().unwrap();
                        self.cache.debug_dump(&original2obf.rebuild(), &format!("{}2obf", target.original));
                        let mut builder = unmodified.rebuild();
                        builder.classes.retain(|original, _| {
                            if let Some(obf) = original2obf.try_get_class(original) {
                                // We only want the new mapping if the original is still obfuscated
                                original == obf
                            } else {
                                // If there is no change from the obfuscated mapping, we still want the new mapping
                                true
                            }
                        });
                        builder.method_names.retain(|original, _| {
                            if let Some(obf) = original2obf.try_get_method(original) {
                                /*
                                 * We only want the new method name if the original is still obfuscated
                                 * Note that this still correctly retains deobfuscated classes, since those are handled seperately above.
                                 */
                                original.name == *obf.name
                            } else {
                                true // Unchanged
                            }
                        });
                        builder.field_names.retain(|original, _| if let Some(obf) =
                            original2obf.try_get_field(original)
                        {
                            original.name == *obf.name
                        } else {
                            true
                        });
                        builder.snapshot()
                    }
                }
                TargetModifier::Classes => {
                    let mut builder = unmodified.rebuild();
                    // We don't want the members
                    builder.method_names.clear();
                    builder.field_names.clear();
                    builder.snapshot()
                }
                TargetModifier::Members => {
                    let mut builder = unmodified.rebuild();
                    // We don't want the classes
                    builder.classes.clear();
                    builder.snapshot()
                }
            });
        }
        info!("Computing {}", target);
        // NOTE: Mostly hardcoded for now
        let builder = match target.original {
            Srg => {
                match target.renamed {
                    Srg => panic!("Redundant: {}", target),
                    Mcp => {
                        let obf2srg = self.try_take1(OBF2SRG)?;
                        let mcp_version = self.mcp_version.as_ref().expect("Unspecified MCP version");
                        let mcp_mappings = self.cache.fetch_mcp_mappings(
                            mcp_version,
                            &self.minecraft_version,
                        )?;
                        let mut builder = MappingsBuilder::with_capacities(
                            // NOTE: MCP classes are used
                            0,
                            mcp_mappings.fields.len(),
                            mcp_mappings.methods.len(),
                        );
                        for (_, serage) in obf2srg.fields() {
                            if let Some(mcp) = mcp_mappings.fields.get(&serage.name) {
                                builder.insert_field(serage.into_owned(), mcp.0.clone());
                            }
                        }
                        for (_, serage) in obf2srg.methods() {
                            if let Some(mcp) = mcp_mappings.methods.get(&serage.name) {
                                builder.insert_method(serage.into_owned(), mcp.0.clone());
                            }
                        }
                        builder
                    }
                    Spigot => {
                        let (srg2obf, obf2spigot) = self.try_take2(MappingsTarget::new(Srg, Obf), OBF2SPIGOT)?;
                        let mut builder = srg2obf.rebuild();
                        builder.chain(&obf2spigot);
                        builder
                    }
                    Obf => {
                        let mut obf2srg = self.try_take1(OBF2SRG)?.rebuild();
                        obf2srg.reverse();
                        obf2srg
                    }
                }
            }
            Mcp => {
                match target.renamed {
                    Srg => {
                        let mut srg2mcp = self.try_take1(SRG2MCP)?.rebuild();
                        srg2mcp.reverse();
                        srg2mcp
                    }
                    Mcp => panic!("Redundant: {}", target),
                    Spigot => {
                        let (mcp2obf, obf2spigot) = self.try_take2(MappingsTarget::new(Mcp, Obf), OBF2SPIGOT)?;
                        let mut builder = mcp2obf.rebuild();
                        builder.chain(&obf2spigot);
                        builder
                    }
                    Obf => {
                        let mut obf2mcp = self.try_take1(MappingsTarget::new(Obf, Mcp))?.rebuild();
                        obf2mcp.reverse();
                        obf2mcp
                    }
                }
            }
            Spigot => {
                match target.renamed {
                    Srg => {
                        let (spigot2obf, obf2srg) = self.try_take2(MappingsTarget::new(Spigot, Obf), OBF2SRG)?;
                        let mut builder = spigot2obf.rebuild();
                        builder.chain(&obf2srg);
                        builder
                    }
                    Mcp => {
                        let (spigot2obf, obf2mcp) = self.try_take2(
                            MappingsTarget::new(Spigot, Obf),
                            MappingsTarget::new(Obf, Mcp),
                        )?;
                        let mut builder = spigot2obf.rebuild();
                        builder.chain(&obf2mcp);
                        builder
                    }
                    Spigot => unimplemented!("Redundnant: {}", target),
                    Obf => {
                        let mut obf2spigot = self.try_take1(OBF2SPIGOT)?.rebuild();
                        obf2spigot.reverse();
                        obf2spigot
                    }
                }
            }
            Obf => {
                match target.renamed {
                    Srg => self.cache.load_srg_mappings(&self.minecraft_version)?,
                    Mcp => {
                        let (obf2srg, srg2mcp) = self.try_take2(OBF2SRG, SRG2MCP)?;
                        let mut builder = obf2srg.rebuild();
                        builder.chain(&srg2mcp);
                        builder
                    }
                    Spigot => self.cache.compute_spigot(&self.minecraft_version)?,
                    Obf => panic!("Redundant: {}", target),
                }
            }
        };
        Ok(builder.snapshot())
    }
    #[inline]
    fn try_take1(&self, item: MappingsTarget) -> Result<MappingsSnapshot, TargetComputeError> {
        let mut result = self.try_take(&[item])?;
        let item = result.pop().unwrap();
        assert!(result.is_empty());
        Ok(item)
    }
    #[inline]
    fn try_take2(&self, first: MappingsTarget, second: MappingsTarget) -> Result<(MappingsSnapshot, MappingsSnapshot), TargetComputeError> {
        let mut result = self.try_take(&[first, second])?;
        let second_result = result.pop().unwrap();
        let first_result = result.pop().unwrap();
        assert!(result.is_empty());
        Ok((first_result, second_result))
    }
    /// Take the specified results, waiting for them if they haven't been computed yet
    #[inline]
    fn try_take(&self, targets: &[MappingsTarget]) -> Result<Vec<MappingsSnapshot>, TargetComputeError> {
        let lock = self.results.read();
        let mut results = Vec::with_capacity(targets.len());
        let mut missing = Vec::new();
        for target in targets {
            if let Some(mappings) = lock.get(target) {
                results.push(mappings.clone())
            } else {
                missing.push(*target);
            }
        }
        if !missing.is_empty() {
            Err(TargetComputeError::WaitingFor(missing))
        } else {
            assert_eq!(results.len(), targets.len());
            Ok(results)
        }
    }
    #[inline]
    pub fn results(&self) -> SeaHashOrderMap<MappingsTarget, MappingsSnapshot> {
        assert!(self.done.load(Ordering::SeqCst), "Not finished!");
        assert!(!self.failed.load(Ordering::SeqCst), "Encountered error!");
        let results = self.results.read();
        results.clone()
    }
    /*
    // NOTE: Circular dependency checking is broken
    #[cfg(debug_assertions)]
    fn check_circular_dependencies(&self) {
        let targets = self.waiters.read();
        let mut effective_dependencies = SeaHashOrderMap::default();
        for target in targets.keys() {
            effective_dependencies.clear();
            self.check_circular_dependencies_for(vec![*target], &mut effective_dependencies);
        }
    }
    #[cfg(debug_assertions)]
    fn check_circular_dependencies_for(
        &self,
        targets: Vec<MappingsTarget>,
        effective_dependencies: &mut SeaHashOrderMap<MappingsTarget, ()>
    ) {
        if let Some(waiter) = self.waiting_targets.get(targets.last().unwrap()) {

            for waiter in waiter.dependencies.keys() {
                let mut next_targets = targets.clone();
                next_targets.push(*waiter);
                if effective_dependencies.insert(*waiter, ()).is_some() {
                    if log_enabled!(::log::LogLevel::Debug) {
                        // NOTE: Copy to as HashMap to get pretty-printed debug output, since CHashMap doesn't use debug_map
                        let mut waiting_targets = SeaHashOrderMap::with_capacity_and_hasher(self.waiting_targets.len(), Default::default());
                        // NOTE: Must clone in order to iterate :(
                        for (key, value) in self.waiting_targets.clone() {
                            waiting_targets.insert(key, value);
                        }
                        debug!("Waiting targets: {:#?}", waiting_targets);
                    }
                    panic!("Circular dependency {}: {:?}", waiter, next_targets)
                }
                self.check_circular_dependencies_for(next_targets, effective_dependencies);
            }
        }
    } 
    */
}

pub enum TargetComputeError {
    WaitingFor(Vec<MappingsTarget>),
    MappingError(MinecraftMappingError),
}
impl From<MinecraftMappingError> for TargetComputeError {
    #[inline]
    fn from(cause: MinecraftMappingError) -> TargetComputeError {
        TargetComputeError::MappingError(cause)
    }
}
const OBF2SRG: MappingsTarget = MappingsTarget::new(MappingsFormat::Obf, MappingsFormat::Srg);
const SRG2MCP: MappingsTarget = MappingsTarget::new(MappingsFormat::Srg, MappingsFormat::Mcp);
const OBF2SPIGOT: MappingsTarget = MappingsTarget::new(MappingsFormat::Obf, MappingsFormat::Spigot);

#[derive(Copy, Clone, PartialEq, Debug, Eq, Hash, Ord, PartialOrd)]
pub struct MappingsTarget {
    original: MappingsFormat,
    renamed: MappingsFormat,
    modifier: Option<TargetModifier>,
}
impl MappingsTarget {
    #[inline]
    const fn new(original: MappingsFormat, renamed: MappingsFormat) -> Self {
        MappingsTarget {
            original,
            renamed,
            modifier: None,
        }
    }
    /// The original and renamed formats of this target, as an array
    #[inline]
    pub fn formats(&self) -> [MappingsFormat; 2] {
        [self.original, self.renamed]
    }
}


lazy_static! {
    static ref TARGET_PATTERN: Regex = Regex::new(r#"(\w+)2(\w+)(?:-(classes|members|onlyobf))?"#).unwrap();
}
impl FromStr for MappingsTarget {
    type Err = MinecraftMappingError;
    fn from_str(value: &str) -> Result<Self, MinecraftMappingError> {
        let captures = TARGET_PATTERN.captures(value).ok_or_else(|| {
            MinecraftMappingError::InvalidTarget(value.to_owned())
        })?;
        let original = MappingsFormat::from_str(&captures[1])?;
        let renamed = MappingsFormat::from_str(&captures[2])?;
        if original == renamed {
            // Redundant mappings are forbidden
            return Err(MinecraftMappingError::InvalidTarget(value.to_owned()));
        }
        let modifier = captures.get(3).map(|name| match name.as_str() {
            "classes" => TargetModifier::Classes,
            "members" => TargetModifier::Members,
            "onlyobf" => TargetModifier::Onlyobf,
            _ => panic!("Regex shouldn't have allowed: {}", name.as_str()),
        });
        Ok(MappingsTarget {
            original,
            renamed,
            modifier,
        })
    }
}
impl Display for MappingsTarget {
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}2{}", self.original, self.renamed)?;
        if let Some(modifier) = self.modifier {
            write!(f, "-{}", modifier)?;
        }
        Ok(())
    }
}
#[derive(Copy, Clone, PartialEq, Debug, Eq, Hash, Ord, PartialOrd)]
pub enum MappingsFormat {
    Srg,
    Mcp,
    Spigot,
    Obf,
}
impl FromStr for MappingsFormat {
    type Err = MinecraftMappingError;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "srg" => Ok(MappingsFormat::Srg),
            "mcp" => Ok(MappingsFormat::Mcp),
            "spigot" => Ok(MappingsFormat::Spigot),
            "obf" => Ok(MappingsFormat::Obf),
            _ => Err(MinecraftMappingError::InvalidTarget(value.to_owned())),
        }
    }
}
impl Display for MappingsFormat {
    #[inline]
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match *self {
            MappingsFormat::Srg => write!(fmt, "srg"),
            MappingsFormat::Mcp => write!(fmt, "mcp"),
            MappingsFormat::Spigot => write!(fmt, "spigot"),
            MappingsFormat::Obf => write!(fmt, "obf"),
        }
    }
}
#[derive(Copy, Clone, PartialEq, Debug, Eq, Hash, Ord, PartialOrd)]
pub enum TargetModifier {
    Classes,
    Members,
    Onlyobf,
}
impl Display for TargetModifier {
    #[inline]
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match *self {
            TargetModifier::Classes => write!(fmt, "classes"),
            TargetModifier::Members => write!(fmt, "members"),
            TargetModifier::Onlyobf => write!(fmt, "onlyobf"),
        }
    }
}
