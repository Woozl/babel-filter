mod config;
mod file;

use ahash::AHashMap;
pub use config::{Config, OutputFormat};
use file::{reader::Reader, writer::Writer};
use serde::{Deserialize, Serialize};
use serde_json;
use std::{ffi::OsStr, fs, path::Path, process::ExitCode, time::Instant};

const BUF_CAPACITY: usize = 32_000;

#[derive(Serialize, Deserialize)]
struct BabelJson {
    curie: String,
    names: Vec<String>,
    types: Vec<String>,
    preferred_name: Option<String>,
    shortest_name_length: Option<usize>,
    taxa: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct NodeListJson {
    id: String,
    name: String,
    category: Vec<String>,
    equivalent_identifiers: Option<Vec<String>>,
}

pub fn run(args: Config) -> ExitCode {
    let start = Instant::now();

    let babel_directory = args.babel_directory;
    let filter_file = args.filter_file;
    let output_directory = args.output_directory;

    if !babel_directory.is_dir() {
        eprintln!("The path provided to the Babel directory isn't a directory or doesn't exist");
        return ExitCode::FAILURE;
    }
    if !filter_file.is_file() {
        eprintln!("The path provided to the filter file isn't a file or doesn't exist");
        return ExitCode::FAILURE;
    }
    if !output_directory.is_dir() {
        eprintln!("The path provided to the output directory isn't a directory or doesn't exist");
        return ExitCode::FAILURE;
    }

    let mut filter_set: AHashMap<String, NodeListJson> = AHashMap::new();
    {
        let mut num_removed: usize = 0;
        let t0 = Instant::now();
        let lines = Reader::new(filter_file, BUF_CAPACITY)
            .expect("Error opening filter file")
            .lines();
        for (line_index, line) in lines.enumerate() {
            if let Ok(node_json) = line {
                match serde_json::from_str::<NodeListJson>(&node_json) {
                    Ok(node) => {
                        if let Some(ref exclude_cats) = args.exclude_category {
                            if !has_excluded_category(node.category.iter(), &exclude_cats) {
                                filter_set.insert(String::from(&node.id), node);
                            } else {
                                num_removed += 1;
                            }
                        } else {
                            filter_set.insert(String::from(&node.id), node);
                        }
                    }
                    Err(e) => eprintln!("Parse error in filter file line {}: {e}", line_index + 1),
                }
            } else {
                eprintln!("Read error in filter file line {}", line_index + 1)
            }
        }
        println!("Creating filter set took {:.2?}", t0.elapsed());
        println!("{} nodes excluded", num_removed);
    }

    for babel_file in fs::read_dir(babel_directory).unwrap() {
        match babel_file {
            Ok(f) => {
                if f.path().is_file() {
                    let t0 = Instant::now();
                    let mut num_nodes: usize = 0;
                    let mut num_kept: usize = 0;

                    let mut output_file_path = Path::join(
                        output_directory.as_std_path(),
                        f.path().file_name().unwrap(), // should be safe to unwrap as we're checking is_file() above
                    );

                    // force compressed/not compressed output if output_format arg is set
                    match args.output_format {
                        Some(OutputFormat::Plaintext) => {
                            if output_file_path.extension() == Some(OsStr::new("gz")) {
                                output_file_path = output_file_path.with_extension("")
                            }
                        }
                        Some(OutputFormat::Gzipped) => {
                            if output_file_path.extension() != Some(OsStr::new("gz")) {
                                output_file_path = output_file_path.with_extension("gz")
                            }
                        }
                        None => (),
                    }

                    let reader: Reader = Reader::new(f.path(), BUF_CAPACITY)
                        .expect("Error opening file for reading");
                    let mut writer: Writer = Writer::new(output_file_path.clone(), BUF_CAPACITY)
                        .expect("Error creating file");

                    for (line_index, line) in reader.lines().enumerate() {
                        num_nodes += 1;
                        if let Ok(node_json) = line {
                            match serde_json::from_str::<BabelJson>(&node_json) {
                                Ok(node) => {
                                    if filter_set.remove(&node.curie).is_some() {
                                        num_kept += 1;
                                        writer.write_line(&node_json).expect("Error writing line");
                                    }
                                }
                                Err(e) => eprint!("{e}"),
                            }
                        } else {
                            eprintln!(
                                "Something went wrong reading line {} of {:?}",
                                line_index + 1,
                                f.path()
                            )
                        }
                    }

                    println!(
                        "Writing {:?} took {:.2?}, kept {}/{} nodes ({:.2}%)",
                        output_file_path.file_name().unwrap_or_default(),
                        t0.elapsed(),
                        num_kept,
                        num_nodes,
                        (num_kept as f64 / num_nodes as f64) * 100.0
                    );
                }
            }
            Err(error) => eprintln!("Error opening file in babel directory: {error}"),
        }
    }

    // create a new file (NonBabelNodes.txt.gz) for all the extra nodes in the filter_set
    let non_babel_nodes_path = Path::join(output_directory.as_std_path(), "./NonBabelNodes.txt.gz");
    let mut nbn_writer =
        Writer::new(non_babel_nodes_path, BUF_CAPACITY).expect("Error creating NonBabelNodes file");
    let filter_set_size = filter_set.len();
    for (curie, node_json) in filter_set {
        let NodeListJson { name, category, .. } = node_json;

        let types = category
            .iter()
            .map(|s| s.replace("biolink:", ""))
            .collect::<Vec<String>>();

        let converted_node = BabelJson {
            curie,
            names: vec![name.clone()],
            types,
            preferred_name: Some(name.clone()),
            shortest_name_length: Some(name.len()),
            taxa: vec![]
        };

        match serde_json::to_string(&converted_node) {
            Ok(json_string) => { nbn_writer.write_line(&json_string).expect("Error writing line"); },
            Err(e) => { eprintln!("Error converting a non babel node to a json line: {e}"); }
        }
    }

    println!("Wrote an extra {filter_set_size} nodes to NonBabelNodes.txt.gz");

    let duration = start.elapsed();
    println!("Program took {:.2?}", duration);

    ExitCode::SUCCESS
}

fn has_excluded_category<'a, I>(set: I, exclude_set: &Vec<String>) -> bool
where
    I: IntoIterator<Item = &'a String>,
{
    if exclude_set.is_empty() {
        return false;
    }
    for cat in set {
        for ex_cat in exclude_set.into_iter() {
            if cat == ex_cat {
                return true;
            }
        }
    }
    false
}
