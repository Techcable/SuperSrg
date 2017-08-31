use std::io::{self, BufReader, BufWriter, BufRead, Write};
use std::path::{Path, PathBuf};
use std::fs::{File, create_dir_all};
use std::process::exit;
use std::mem;
use std::str;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::rangemap::{RangeMap, FileRanges, MemberReference};
use mappings::{MappingsSnapshot, Mappings};

use walkdir::{DirEntry, WalkDir};
use chan::{self, Receiver};

pub struct ParallelRangeApplier<'a> {
    pub num_workers: u32,
    pub error_action: ErrorAction,
    pub log_level: LogLevel,
    num_files: AtomicUsize,
    num_references: AtomicUsize,
    mappings: &'a MappingsSnapshot,
    rangemap: &'a RangeMap,
}
impl<'a> ParallelRangeApplier<'a> {
    pub fn new(mappings: &'a MappingsSnapshot, rangemap: &'a RangeMap) -> Self {
        ParallelRangeApplier {
            num_workers: 2, // Default to not very parallel
            error_action: ErrorAction::Exit(1), // Fail fast is always a good default
            log_level: LogLevel::Normal,
            mappings, rangemap,
            num_files: AtomicUsize::new(0),
            num_references: AtomicUsize::new(0)
        }
    }
    #[inline]
    pub fn num_files(&self) -> usize {
        self.num_files.load(Ordering::SeqCst)
    }
    #[inline]
    pub fn num_references(&self) -> usize {
        self.num_references.load(Ordering::SeqCst)
    }
    pub fn parallel_apply<'b>(&self, source: &'b Path, output: &'b Path) {
        assert!(
            source.is_dir(),
            "Source isn't a directory: {}",
            source.display()
        );
        assert!(
            !output.is_file(),
            "Output is an existing file: {}",
            output.display()
        );
        assert!(self.num_workers > 0, "Zero workers!");
        if cfg!(debug_assertions) {
            self.rangemap.debug_dump();
        }
        ::crossbeam::scope(|scope| {
            let (sender, reciever) = chan::sync(1000);
            for _ in 0..self.num_workers {
                let reciever = reciever.clone();
                let error_action = self.error_action;
                let log_level = self.log_level;
                let source = source.to_owned();
                let output = output.to_owned();
                scope.spawn(move || {
                    let applier = RangeMapApplier::new(&self.mappings);
                    let worker = ParallelRangeApplierWorker {
                        source,
                        output,
                        applier,
                        reciever,
                        rangemap: self.rangemap,
                        error_action,
                        log_level,
                        num_files: &self.num_files,
                        num_references: &self.num_references
                    };
                    worker.run();
                });
            }
            for result in WalkDir::new(source) {
                match result {
                    Ok(entry) => sender.send(entry),
                    Err(e) => {
                        eprintln!("ERROR walking directory: {}", e);
                        match self.error_action {
                            ErrorAction::Warn => {}
                            ErrorAction::Exit(code) => exit(code), 
                        }
                    }
                }
            }
            mem::drop(sender);
        });
    }
}

#[derive(Copy, Clone)]
pub enum ErrorAction {
    Exit(i32),
    Warn,
}
#[derive(Copy, Clone, PartialEq)]
pub enum LogLevel {
    Verbose,
    Quiet,
    Normal,
}

struct ParallelRangeApplierWorker<'a> {
    source: PathBuf,
    output: PathBuf,
    applier: RangeMapApplier<'a>,
    rangemap: &'a RangeMap,
    reciever: Receiver<DirEntry>,
    error_action: ErrorAction,
    log_level: LogLevel,
    num_files: &'a AtomicUsize,
    num_references: &'a AtomicUsize,
}
impl<'a> ParallelRangeApplierWorker<'a> {
    fn run(&self) {
        for entry in self.reciever.iter() {
            let relative = entry.path().strip_prefix(&self.source).unwrap();
            let relative_name = relative.to_str().unwrap().to_owned();
            if let Some(ranges) = self.rangemap.files.get(&relative_name) {
                match self.apply_file(ranges, relative) {
                    Ok(changes) => {
                        if changes > 0 {
                            debug!("Remapped {} references: {}", changes, relative.display());
                            self.num_references.fetch_add(changes, Ordering::SeqCst);
                            if self.log_level == LogLevel::Verbose {
                                println!("Remapped {} references: {}", changes, relative.display())
                            }
                        } else {
                            debug!("Unchanged: {}", relative.display());
                            if self.log_level == LogLevel::Verbose {
                                println!("Unchanged: {}", relative.display())
                            }
                        }
                        self.num_files.fetch_add(1, Ordering::SeqCst);
                    }
                    Err(e) => {
                        eprintln!("ERROR in {}: {}", relative.display(), e);
                        match self.error_action {
                            ErrorAction::Warn => {}
                            ErrorAction::Exit(code) => exit(code),
                        };
                    }
                }
            } else {
                debug!("No mappings for {}", relative.display());
                if self.log_level == LogLevel::Verbose {
                    println!("No mappings for {}", relative.display());
                }
            }
        }
    }
    fn apply_file(&self, ranges: &FileRanges, relative_path: &Path) -> io::Result<usize> {
        assert!(
            relative_path.is_relative(),
            "Path isn't relative: {}",
            relative_path.display()
        );
        let mut source_file = PathBuf::from(&self.source);
        source_file.push(relative_path);
        let mut output_file = PathBuf::from(&self.output);
        output_file.push(relative_path);
        if let Some(output_parent) = output_file.parent() {
            create_dir_all(output_parent)?;
        }
        let mut input = BufReader::new(File::open(source_file)?);
        let mut output = BufWriter::new(File::create(output_file)?);
        Ok(self.applier.apply(ranges, &mut input, &mut output)?)
    }
}

#[derive(Clone)]
pub struct RangeMapApplier<'a> {
    mappings: &'a MappingsSnapshot,
}

impl<'a> RangeMapApplier<'a> {
    #[inline]
    pub fn new(mappings: &'a MappingsSnapshot) -> Self {
        RangeMapApplier { mappings }
    }
    fn remap_reference(&self, reference: MemberReference, out: &mut String) -> bool {
        let (changed, new_name) = match reference {
            MemberReference::Field(fieldref) => {
                match self.mappings.try_get_field_name(&fieldref.referenced_field) {
                    Some(renamed_name) => (
                        *renamed_name != fieldref.referenced_field.name,
                        renamed_name.as_ref(),
                    ),
                    None => (false, fieldref.referenced_field.name.as_ref()),
                }
            }
            MemberReference::Method(methodref) => {
                match self.mappings.try_get_method_name(
                    &methodref.referenced_method,
                ) {
                    Some(renamed_name) => (
                        *renamed_name == methodref.referenced_method.name,
                        renamed_name.as_ref(),
                    ),
                    None => (false, methodref.referenced_method.name.as_ref()),
                }
            }
        };
        out.push_str(new_name);
        changed
    }
    pub fn apply<R: BufRead, W: Write>(&self, ranges: &FileRanges, mut input: R, output: &mut W) -> io::Result<usize> {
        let references = ranges.sorted().into_iter();
        let mut index: u64 = 0;
        let mut original_name_buffer = Vec::new();
        let mut output_name_buffer = String::new();
        let mut num_changes = 0;
        for next_reference in references {
            let location = next_reference.location();
            assert!(location.start as u64 >= index);
            let tocopy = location.start as u64 - index;
            if tocopy > 0 {
                let mut taker = input.take(tocopy as u64);
                let numread = io::copy(&mut taker, output)?;
                input = taker.into_inner();
                index += numread;
                if index < location.start as u64 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        format!(
                            "Expected to reach location {:?}, but only got {}",
                            location,
                            index
                        ),
                    ));
                }
            }
            assert_eq!(location.start as u64, index);
            while original_name_buffer.len() < location.size() as usize {
                original_name_buffer.push(0);
            }
            let original_name_bytes = &mut original_name_buffer[..location.size() as usize];
            input.read_exact(original_name_bytes)?;
            index += location.size() as u64;
            let original_name = str::from_utf8(original_name_bytes).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, e)
            })?;
            {
                let expected_name = next_reference.name();
                assert_eq!(
                    original_name,
                    expected_name,
                    "Unexpected reference at {:?}",
                    location
                );
            }
            output_name_buffer.clear();
            if self.remap_reference(next_reference, &mut output_name_buffer) {
                num_changes += 1;
            }
            output.write_all(output_name_buffer.as_bytes())?;
        }
        // Directly copy remaining
        io::copy(&mut input, output)?;
        Ok(num_changes)
    }
}
