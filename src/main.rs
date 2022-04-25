#![allow(unused_imports)]
#![allow(unused_variables)]
#![allow(dead_code)]
use clap::Parser;
use itertools::Itertools;
use log::LevelFilter;
use log::{error, info, trace};
use rand::seq::SliceRandom;
use rand::Rng;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use simple_logger::SimpleLogger;
use std::cell::RefCell;
use std::cmp;
use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::fs::File;
use std::io::prelude::*;
use std::io::SeekFrom;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::time::Duration;
use std::time::Instant;
use walkdir::WalkDir;

#[macro_use]
extern crate error_chain;

mod errors {
	error_chain! {
		foreign_links {
			Io(std::io::Error);
			Serde(serde_json::Error);
			TryFromIntError(std::num::TryFromIntError);
		}
	}
}

use errors::*;

const KB: usize = 1024;

#[derive(Debug, Parser)]
#[clap(about = "Compress folders sensibly")]
struct Opt {
	/// Increase verbosity
	#[clap(short, long)]
	verbose: bool,

	/// Action
	#[clap(subcommand)]
	action: Action,

	/// Test Folder
	#[clap(long, parse(from_os_str))]
	test_folder: PathBuf,
}

#[derive(clap::Subcommand, Debug)]
enum Action {
	Setup {
		/// Number of files to generate
		#[clap(long, default_value_t = 10)]
		num_files_to_generate: usize,

		/// Minium size of file to generate (in kb)
		#[clap(long, default_value_t = 1)]
		min_file_size: usize,

		/// Maximum size of file to generate (in kb)
		#[clap(long, default_value_t = 10)]
		max_file_size: usize,
	},
	Test {
		/// Number of iterations
		#[clap(long, default_value_t = 1)]
		num_iterations: usize,

		/// How many files to test
		#[clap(long, default_value_t = 1)]
		num_files_to_test: usize,
	},
}

#[derive(Serialize, Deserialize)]
struct Entries {
	entries: Vec<FileEntry>,
}

#[derive(Debug, Serialize, Deserialize)]
struct FileEntry {
	path: String,
	size: usize,
	start: usize,
}

fn main() -> Result<()> {
	/* Setup:
		- Given a directory, an equivalent tar file, and a manifest of each, randomly choose n paths to go down.
		  Copy the data from each, compute some basic hash, and make sure they all match up.
		- Play around with ordering so paging doesn't come into play
		- Play around with filesize, too
	*/

	let options = Opt::parse();

	let logger = SimpleLogger::new().with_colors(true).without_timestamps();
	if options.verbose {
		logger.with_level(LevelFilter::Trace).init().unwrap();
	} else {
		logger.with_level(LevelFilter::Info).init().unwrap();
	}

	match options.action {
		Action::Setup {
			num_files_to_generate,
			max_file_size,
			min_file_size,
		} => setup(
			&options,
			num_files_to_generate,
			min_file_size,
			max_file_size,
		),
		Action::Test {
			num_iterations,
			num_files_to_test,
		} => test(&options, num_iterations, num_files_to_test),
	}
}

fn test(options: &Opt, num_iterations: usize, num_files_to_test: usize) -> Result<()> {
	let folder_path = get_folder_path(&options.test_folder);
	let archive_path = get_archive_path(&options.test_folder);
	let manifest_path = get_manifest_path(&options.test_folder);

	let manifest = get_manifest(&manifest_path)?;

	let mut archive_file = File::open(archive_path)?;

	let entry = &manifest[0];
	info!("Testing {}", &entry.path);

	if num_files_to_test > manifest.len() {
		return Err(format!(
			"Can't test more files than exist. Max files: {}",
			manifest.len()
		))?;
	}

	let mut rng = rand::thread_rng(); // TODO: probably we shouldn't make a new rng per file, but w/e

	let mut indexes = (0..manifest.len()).collect::<Vec<usize>>();
	indexes.shuffle(&mut rng);

	let mut disk_hash: u8;
	let mut archive_hash: u8;

	let mut total_disk_time: Duration = Duration::new(0, 0);
	let mut total_archive_time: Duration = Duration::new(0, 0);
	let mut count = 0;
	for i in 0..num_iterations {
		for index in &indexes[..num_files_to_test] {
			let index = rng.gen_range(0..manifest.len());

			{
				let before = Instant::now();
				archive_hash = hash_file_in_archive(entry, &mut archive_file)?;
				let archive_time = before.elapsed();
				total_archive_time += archive_time;
			}

			{
				let before = Instant::now();
				disk_hash = hash_file_on_disk(entry)?;
				let disk_time = before.elapsed();
				total_disk_time += disk_time;
			}

			assert_eq!(disk_hash, archive_hash);
			count += 1;
		}
	}

	info!("Total iterations: {}", count);

	info!("Total disk time: {:.2?}", total_disk_time);
	info!("Total archive time: {:.2?}", total_archive_time);

	info!("Averate disk time: {:.2?}", total_disk_time / count);
	info!("Averate archive time: {:.2?}", total_archive_time / count);

	Ok(())
}

fn hash_file_on_disk(entry: &FileEntry) -> Result<u8> {
	return Ok(super_fast_hash(&fs::read(&entry.path)?));
}

fn hash_file_in_archive(entry: &FileEntry, archive_file: &mut File) -> Result<u8> {
	return Ok(super_fast_hash(&get_bytes_from_archive(
		archive_file,
		&entry,
	)?));
}

fn get_bytes_from_archive(archive_file: &mut File, entry: &FileEntry) -> Result<Vec<u8>> {
	archive_file.seek(SeekFrom::Start(entry.start.try_into()?))?;
	let mut rv = vec![0u8; entry.size];
	archive_file.read_exact(&mut rv)?;
	return Ok(rv);
}

fn super_fast_hash(bytes: &[u8]) -> u8 {
	let mut rv: u8 = 0;
	for b in bytes {
		rv ^= b;
	}
	return rv;
}

fn setup(
	options: &Opt,
	num_files_to_generate: usize,
	min_file_size: usize,
	max_file_size: usize,
) -> Result<()> {
	let folder_path = get_folder_path(&options.test_folder);
	let archive_path = get_archive_path(&options.test_folder);
	let manifest_path = get_manifest_path(&options.test_folder);

	if folder_path.exists() {
		info!("Removing old folder {}...", &folder_path.to_string_lossy());
		fs::remove_dir_all(&folder_path)?;
	}

	if archive_path.exists() {
		info!("Removing old archive...");
		fs::remove_file(&archive_path)?;
	}

	if manifest_path.exists() {
		info!("Removing old manifest...");
		fs::remove_file(&manifest_path)?;
	}

	fs::create_dir_all(&folder_path)?;
	let mut archive = File::create(&archive_path)?;

	let mut file_entries: Vec<FileEntry> = Vec::new();
	let mut start: usize = 0;
	for i in 0..num_files_to_generate {
		// generate filename
		let filename = Path::new(&folder_path).join(format!("{}.txt", i));

		// generate file
		let filename_string = filename.to_string_lossy().to_string();
		info!("Generating {}...", &filename_string);
		let size = generate_random_file(filename, min_file_size * KB, max_file_size * KB)?;
		info!("Size: {}", size);

		// write to archive
		archive.write_all(&fs::read(&filename_string)?)?;

		// update manifests
		file_entries.push(FileEntry {
			path: filename_string,
			start,
			size,
		});
		start += size;
	}

	let mut manifest = File::create(&manifest_path)?;
	manifest.write_all(
		serde_json::to_string(&Entries {
			entries: file_entries,
		})?
		.to_string()
		.as_bytes(),
	)?;

	Ok(())
}

fn get_folder_path(test_folder: &PathBuf) -> PathBuf {
	return Path::new(test_folder).join("test");
}

fn get_manifest_path(test_folder: &PathBuf) -> PathBuf {
	return Path::new(test_folder).join("test.manifest");
}

fn get_archive_path(test_folder: &PathBuf) -> PathBuf {
	return Path::new(test_folder).join("test.archive");
}

fn get_manifest(manifest_path: &PathBuf) -> Result<Vec<FileEntry>> {
	let contents = fs::read_to_string(manifest_path)?;
	let entries: Entries = serde_json::from_str(&contents)?;
	return Ok(entries.entries);
}

fn generate_random_file(filename: PathBuf, min_size: usize, max_size: usize) -> Result<usize> {
	let mut rng = rand::thread_rng(); // TODO: probably we shouldn't make a new rng per file, but w/e
	let size = rng.gen_range(min_size..max_size);
	let f = File::create(filename)?;
	let mut writer = BufWriter::new(f);

	let mut buffer = [0; 1024];
	let mut remaining_size = size;

	while remaining_size > 0 {
		let to_write = cmp::min(remaining_size, buffer.len());
		let buffer = &mut buffer[..to_write];
		rng.fill(buffer);
		writer.write(buffer)?;

		remaining_size -= to_write;
	}

	Ok(size)
}
