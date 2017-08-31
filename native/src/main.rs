#[macro_use]
extern crate clap;
extern crate supersrg;
extern crate num_cpus;
extern crate crossbeam;
extern crate env_logger;

use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::exit;
use std::cmp::min;
use std::io::{BufReader, Write, BufWriter};
use std::fs::{File, create_dir_all};
use std::collections::HashSet;
use std::str::FromStr;

use supersrg::mappings::{MappingsBuilder, Mappings, MappingsSnapshot};
use supersrg::mappings::binary::{MappingsEncoder, MappingsDecoder};
use supersrg::mappings::encoder::{MappingsEncoder as TextMappingsEncoder, SrgEncoder};
use supersrg::mappings::parser::{MappingsParser, SrgMappingsParser, CompactSrgParser};
use supersrg::ranges::rangemap::RangeMapDeserializer;
use supersrg::ranges::applier::{ParallelRangeApplier, LogLevel};
use supersrg::minecraft::MinecraftMappingsCache;
use supersrg::minecraft::targets::{MappingsTarget, MappingsTargetComputerBuilder, MappingsFormat};
use supersrg::utils::full_extension;

fn main() {
    env_logger::init().unwrap();
    let mut app =
        clap_app!(supersrg =>
        (version: crate_version!())
        (author: crate_authors!())
        (about: crate_description!())
        (@subcommand apply_range =>
            (about: "Applies the specified range map to the source directory")
            (@arg force: --force -f "Delete the output directory if it already exists")
            (@arg verbose: -v --verbose "Show verbose output")
            (@arg rangemap: +required "The SuperSrg binary rangemap to apply")
            (@arg mappings: +required "The mappings file to apply")
            (@arg source: +required "The source directory containing the files to remap")
            (@arg output: +required "Where to place the remapped files")
        )
        (@subcommand generate_minecraft =>
            (about: "Generates minecraft mappings based on the MCP and Spigot deobfuscation info")
            (@arg builddata_commit: +takes_value --("builddata-commit") "The spigot BuildData commit to generate the mappings for, infered by default")
            (@arg refresh_spigot: --("refresh-spigot") "Refresh spigot BuildData information, checking if it's changed")
            (@arg mcp_version: --mcp +takes_value "The MCP version to generate the mappings for")
            (@arg cache: --cache "Specify an alternate cache location, defaulting to the output directory")
            (@arg format: --format default_value[binary] +takes_value "The mapping format to emit the resulting mappings in")
            (@arg minecraft_version: +required "The minecraft version to generate the mappings for")
            (@arg output_dir: +required "The output directory to place generated mappings")
            (@arg targets: +required +multiple "The target mappings to generate")
        )
        (@subcommand convert =>
            (about: "Converts from one mapping format into another")
            (@arg format: --format default_value[binary] +takes_value "The mapping format to emit the resulting mappings in")
            (@arg input: +required "The input mappings file to convert")
            (@arg output: +required "The output file to place the resulting mappings")
        )
    );
    let primary_args = app.clone().get_matches();
    match primary_args.subcommand() {
        ("apply_range", Some(args)) => {
            let rangemap_path = Path::new(args.value_of("rangemap").unwrap());
            let mappings_path = Path::new(args.value_of("mappings").unwrap());
            let source = Path::new(args.value_of("source").unwrap());
            let output = Path::new(args.value_of("output").unwrap());
            let rangemap = {
                if !rangemap_path.exists() {
                    eprintln!("Range map doesn't exist: {}", rangemap_path.display());
                    exit(1);
                }
                let mut rangemap_reader = match File::open(rangemap_path) {
                    Ok(result) => BufReader::new(result),
                    Err(e) => {
                        eprintln!(
                            "Unable to open range map {}: {}",
                            rangemap_path.display(),
                            e
                        );
                        exit(1);
                    }
                };
                println!("Reading rangemap from {}", rangemap_path.display());
                match RangeMapDeserializer::read(&mut rangemap_reader) {
                    Ok(result) => result.build(),
                    Err(e) => {
                        eprintln!("Error loading range map: {}", e);
                        exit(1);
                    }
                }
            };
            println!("Reading mappings from {}", mappings_path.display());
            let mappings = parse_mappings(mappings_path).snapshot();
            let mut applier = ParallelRangeApplier::new(&mappings, &rangemap);
            if args.is_present("verbose") {
                applier.log_level = LogLevel::Verbose;
            }
            // NOTE: Consider using more than just the number of CPUS since this is likely IO-bound
            applier.num_workers = min(num_cpus::get() as u32, 2);
            applier.parallel_apply(source, output);
            println!("Remapped {} references in {} files", applier.num_references(), applier.num_files());
        }
        ("generate_minecraft", Some(args)) => {
            let output_dir = Path::new(args.value_of("output_dir").unwrap());
            let cache_dir = args.value_of("cache").map(PathBuf::from).unwrap_or_else(
                || {
                    output_dir.to_owned()
                },
            );
            if let Err(e) = create_dir_all(&output_dir) {
                eprintln!("Unable to create output dir: {}", e);
                exit(1)
            }
            if let Err(e) = create_dir_all(&cache_dir) {
                eprintln!("Unable to create cache dir: {}", e);
                exit(1)
            }
            let output_format = value_t!(args, "format", OutputFormat).unwrap_or_else(|e| e.exit());
            let minecraft_version = args.value_of("minecraft_version").unwrap();
            let targets = values_t!(args, "targets", MappingsTarget).unwrap_or_else(|e| e.exit());
            let mut target_set: HashSet<MappingsTarget> = HashSet::with_capacity(targets.len());
            for target in &targets {
                if !target_set.insert(*target) {
                    eprintln!("Duplicate target: {}", target);
                    exit(1);
                }
            }
            assert_eq!(targets.len(), target_set.len());
            let mut computer_builder = MappingsTargetComputerBuilder::new(
                MinecraftMappingsCache::new(cache_dir.to_owned()),
                minecraft_version.to_owned(),
            );
            computer_builder.targets(&targets);

            if let Some(mcp_version) = args.value_of("mcp_version") {
                computer_builder.mcp_version(mcp_version.to_owned());
            } else {
                for target in &targets {
                    if target.formats().contains(&MappingsFormat::Mcp) {
                        eprintln!("MCP version must be specified to compute {}", target);
                        exit(1)
                    }
                }
            }
            if let Some(builddata_commit) = args.value_of("builddata_commit") {
                computer_builder.builddata_commit(builddata_commit.to_owned());
            }
            if args.is_present("refresh_spigot") {
                computer_builder.refresh_spigot();
            }
            let computer = computer_builder.build();
            // NOTE: Don't use more than just the number of CPUS for now since this isn't nessicarrily IO-bound
            let target_threads = min(num_cpus::get() as u32, 2);
            crossbeam::scope(|s| {
                debug_assert!(target_threads > 0);
                for _ in 0..target_threads {
                    s.spawn(|| if let Err(e) = computer.compute_target_work() {
                        eprintln!("Error computing targets: {:?}", e);
                        exit(1)
                    });
                }
            });
            let mut results = computer.results();
            // Now, create a brand new scope and spawn a thread to write each result
            // TODO: Consider somehow reusing the above threads for this job
            crossbeam::scope(|s| for target in &targets {
                let result = results.remove(target).unwrap_or_else(
                    || panic!("Missing {}", target),
                );
                s.spawn(move || {
                    let mappings = result.snapshot();
                    let output_file = output_dir.join(format!("{}.{}", target, output_format.extension()));
                    output_format
                        .write_path(&mappings, &output_file)
                        .unwrap_or_else(|e| {
                            eprintln!("Error writing mappings: {}", e);
                            exit(1)
                        })
                });
            });
        },
        ("convert", Some(args)) => {
            let output_format = value_t!(args, "format", OutputFormat).unwrap_or_else(|e| e.exit());
            let input = Path::new(args.value_of("input").unwrap());
            let output = Path::new(args.value_of("output").unwrap());
            let mappings = parse_mappings(input);
            output_format.write_path(&mappings.snapshot(), output).unwrap_or_else(|e| {
                eprintln!("Error writing mappings: {}", e);
                exit(1)
            })
        }
        _ => {
            // Run help if no subcommand specified
            app.print_help().unwrap_or_else(|e| e.exit());
        }
    }
}
#[derive(Copy, Clone, Debug, PartialEq)]
enum OutputFormat {
    Binary,
    Srg,
}
impl FromStr for OutputFormat {
    type Err = ();
    #[inline]
    fn from_str(s: &str) -> Result<OutputFormat, ()> {
        match s {
            "binary" => Ok(OutputFormat::Binary),
            "srg" => Ok(OutputFormat::Srg),
            _ => Err(()),
        }
    }
}
impl OutputFormat {
    #[inline]
    fn extension(&self) -> &'static str {
        match *self {
            OutputFormat::Binary => "srg.dat",
            OutputFormat::Srg => "srg",
        }
    }
    #[inline]
    fn write_path(&self, mappings: &MappingsSnapshot, path: &Path) -> Result<(), Box<Error>> {
        self.write(mappings, BufWriter::new(File::create(path)?))
    }
    fn write<W: Write>(&self, mappings: &MappingsSnapshot, mut writer: W) -> Result<(), Box<Error>> {
        match *self {
            OutputFormat::Srg => {
                let encoder = SrgEncoder::new(mappings);
                if let Err(e) = encoder.write(&mut writer) {
                    return Err(Box::new(e));
                }
            }
            OutputFormat::Binary => {
                let encoder = MappingsEncoder::new(writer);
                if let Err(e) = encoder.encode(mappings) {
                    return Err(Box::new(e));
                }
            }
        }
        Ok(())
    }
}
fn parse_mappings(mappings_path: &Path) -> MappingsBuilder {
    let format: &'static str;
    if let Some(extension) = full_extension(mappings_path) {
        if extension == "csrg" {
            format = "csrg";
        } else if extension == "srg" {
            format = "srg";
        } else if extension == "srg.dat" {
            format = "binary";
        } else {
            eprintln!("Unknown mapping file extension: '{}'", extension);
            exit(1)
        }
    } else {
        eprintln!("WARN: Misisng mappping file extension, assuming srg format.");
        format = "srg";
    }
    let mut mappings_reader = match File::open(mappings_path) {
        Ok(result) => BufReader::new(result),
        Err(e) => {
            eprintln!("Unable to open mappings {}: {}", mappings_path.display(), e);
            exit(1)
        }
    };
    let result: Result<MappingsBuilder, Box<Error>> = match format {
        "srg" => {
            let mut parser = SrgMappingsParser::default();
            match parser.read(&mut mappings_reader) {
                Err(e) => Err(Box::new(e)),
                Ok(_) => Ok(parser.finish()),
            }
        }
        "csrg" => {
            let mut parser = CompactSrgParser::default();
            match parser.read(&mut mappings_reader) {
                Err(e) => Err(Box::new(e)),
                Ok(_) => Ok(parser.finish()),
            }
        }
        "binary" => {
            let decoder = MappingsDecoder::new(mappings_reader);
            let mut builder = MappingsBuilder::new();
            match decoder.decode(&mut builder) {
                Err(e) => Err(Box::new(e)),
                Ok(_) => Ok(builder),
            }
        }
        _ => unimplemented!("Unkown format: {}", format),
    };
    match result {
        Ok(mappings) => mappings,
        Err(e) => {
            eprintln!("Error parsing mappings: {}", e);
            exit(1)
        }
    }
}
