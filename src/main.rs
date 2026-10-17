// Textbook Manager
// Copyright (C) 2026 mvx-dev
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// any later version
//
// This program is distributed in the hope it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see http://www.gnu.org/licenses/

use anyhow::anyhow;
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::{
    error::Error,
    ffi::OsStr,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
};
use toml;
use walkdir::WalkDir;

trait ExpandTildeExt {
    fn expand_tilde(&self) -> PathBuf;
}

impl ExpandTildeExt for str {
    fn expand_tilde(&self) -> PathBuf {
        expand_tilde(self)
    }
}

impl ExpandTildeExt for String {
    fn expand_tilde(&self) -> PathBuf {
        expand_tilde(self)
    }
}

#[derive(Debug, Deserialize)]
struct Config {
    tbm: TBMConfig,
}

#[derive(Debug, Deserialize)]
struct TBMConfig {
    default_dir: Option<PathBuf>,
}

#[derive(Parser)]
#[command(name = "tbm")]
#[command(about = "Manage textbook PDFs", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Location to look for documents
    #[arg(short, long, global = true)]
    dir: Option<String>,

    /// Location of configuration file
    #[arg(short, long, global = true, default_value = "~/.config/tbm")]
    config: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    Open {
        query: Option<String>,
    },
    Add {
        file: PathBuf,
        message: Option<String>,
    },
    List,
    Init {
        remote: Option<String>,
    },
}

enum DirectoryState {
    NotExists,
    NotInitialised,
    Initialised,
}

fn list_pdfs(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut out = Vec::new();
    for entry in WalkDir::new(dir).into_iter().filter_map(Result::ok) {
        let p = entry.path();
        if p.is_file() && p.extension() == Some(OsStr::new("pdf")) {
            out.push(p.to_path_buf());
        }
    }

    Ok(out)
}

fn fuzzy_find(files: &[PathBuf], query: &str) -> Result<Option<PathBuf>, Box<dyn Error>> {
    let query_lc = query.to_lowercase();
    let mut matches: Vec<&PathBuf> = files
        .iter()
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.to_lowercase().contains(&query_lc))
                .unwrap_or(false)
        })
        .collect();

    // TODO: implement proper fuzzy find
    if matches.is_empty() {
        return inter_pick(files);
    }

    if matches.len() == 1 {
        return Ok(Some(matches.remove(0).clone()));
    }

    // TODO: implement ranking system
    inter_pick_list(matches.into_iter().cloned().collect())
}

fn inter_pick(files: &[PathBuf]) -> Result<Option<PathBuf>, Box<dyn Error>> {
    inter_pick_list(files.to_vec())
}

fn inter_pick_list(files: Vec<PathBuf>) -> Result<Option<PathBuf>, Box<dyn Error>> {
    use skim::prelude::*;

    let input = files
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");

    let options = SkimOptionsBuilder::default()
        .no_height(true)
        .multi(false)
        .prompt("tbm> ")
        .preview("file {}")
        .build()
        .unwrap();

    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(std::io::Cursor::new(input));

    let selected = Skim::run_with(options, Some(items))
        .ok()
        .and_then(|out| out.selected_items.first().cloned());

    Ok(selected.map(|item| PathBuf::from(item.output().to_string())))
}

fn handlr_open(path: &Path) -> Result<(), Box<dyn Error>> {
    run_cmd("handlr", &["open", path.to_str().ok_or("bad path")?])
}

fn run_cmd(cmd: &str, args: &[&str]) -> Result<(), Box<dyn Error>> {
    let status = Command::new(cmd).args(args).status()?;
    if !status.success() {
        return Err(format!("command failed: {} {:?}", cmd, args).into());
    }
    Ok(())
}

fn expand_tilde(s: &str) -> PathBuf {
    if let Some(rest) = s.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    PathBuf::from(s)
}

fn dir_to_str(dir: &Path) -> Result<&str, Box<dyn Error>> {
    dir.to_str().ok_or_else(|| "invalid directory path".into())
}

fn mode_open(dir: &Path, query: Option<&str>) -> Result<(), Box<dyn Error>> {
    let files = list_pdfs(dir)?;
    if files.is_empty() {
        return Err("no PDFs found".into());
    }

    let selected = match query {
        Some(q) if !q.trim().is_empty() => fuzzy_find(&files, q)?,
        _ => inter_pick(&files)?,
    };

    if let Some(path) = selected {
        handlr_open(&path)?;
    }

    Ok(())
}

fn mode_add(dir: &Path, file: &Path, message: Option<&str>) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(dir)?;
    let filename = file.file_name().ok_or("input file has no name")?;

    let dest = dir.join(filename);
    let dest_str = dest.to_str().ok_or("bad path")?;

    fs::copy(file, &dest)?;

    run_cmd("git", &["-C", dir_to_str(dir)?, "lfs", "track", "*.pdf"])?;
    run_cmd("git", &["-C", dir_to_str(dir)?, "add", ".gitattributes"])?;
    run_cmd("git", &["-C", dir_to_str(dir)?, "add", &dest_str])?;

    let default_msg = format!("Added {dest_str}");
    let msg = message.unwrap_or(default_msg.as_str());

    run_cmd("git", &["-C", dir_to_str(dir)?, "commit", "-m", msg])?;
    run_cmd("git", &["-C", dir_to_str(dir)?, "push"])?;

    Ok(())
}

fn mode_list(dir: &Path) -> Result<(), Box<dyn Error>> {
    list_pdfs(dir)?
        .iter()
        .filter_map(|p| p.file_name())
        .for_each(|name| println!("{}", name.to_string_lossy()));

    Ok(())
}

fn mode_init(dir: &Option<PathBuf>, uri: &Option<String>) -> Result<PathBuf, Box<dyn Error>> {
    let dir: PathBuf = match dir {
        Some(dir) => dir.clone(),
        None => loop {
            print!("Enter textbook directory (default ~/Documents/textbooks): ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            let input = match input.trim() {
                "" => "~/Documents/textbooks".to_owned(),
                dir => dir.to_owned(),
            }
            .expand_tilde();

            print!("Confirm directory {}? [Y/n]: ", dir_to_str(&input)?);
            io::stdout().flush()?;

            let mut confirm = String::new();
            io::stdin().read_line(&mut confirm)?;

            if confirm.trim().eq_ignore_ascii_case("n") {
                continue;
            }

            break input;
        },
    };

    match check_init(&dir) {
        DirectoryState::Initialised => {
            println!("Directory already initialised");
            return Ok(dir);
        }
        DirectoryState::NotExists => fs::create_dir_all(&dir)?,
        DirectoryState::NotInitialised => (),
    }

    let uri: String = match uri {
        Some(uri) => uri.clone(),
        None => loop {
            print!("Enter remote URI: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            let input = input.trim();
            if input.is_empty() {
                // TODO allow local-only repos
                eprintln!("URI cannot be empty.");
            }

            print!("Confirm URI [{input}] [Y/n]: ");
            io::stdout().flush()?;

            let mut confirm = String::new();
            io::stdin().read_line(&mut confirm)?;

            if confirm.trim().eq_ignore_ascii_case("n") {
                continue;
            }

            break input.to_owned();
        },
    };

    let dir_str = dir_to_str(&dir)?;

    run_cmd("git", &["-C", dir_str, "init"])?;
    run_cmd("git", &["-C", dir_str, "lfs", "install"])?;
    run_cmd(
        "git",
        &["-C", dir_str, "remote", "add", "origin", uri.as_str()],
    )?;

    if run_cmd(
        "git",
        &["-C", dir_str, "pull", "--set-upstream", "origin", "main"],
    )
    .is_err()
    {
        fs::remove_dir_all(dir.join(".git"))?;
        return Err(
            "could not pull from remote (invalid URI, no access, or no `main` branch)".into(),
        );
    }

    Ok(dir)
}

fn load_config(config_dir: &Option<String>) -> Result<Config, Box<dyn Error>> {
    let config = if let Some(dir) = config_dir {
        let path = expand_tilde(&dir);
        let text = fs::read_to_string(path.join("config.toml"))?;
        toml::from_str(&text)?
    } else {
        Config {
            tbm: TBMConfig { default_dir: None },
        }
    };

    Ok(config)
}

fn check_init(dir: &Path) -> DirectoryState {
    if fs::exists(dir).is_ok_and(|b| b) {
        if fs::exists(Path::join(dir, ".git")).is_ok_and(|b| b) {
            return DirectoryState::Initialised;
        }
        return DirectoryState::NotInitialised;
    }
    DirectoryState::NotExists
}

fn ensure_initialised(dir: &Option<PathBuf>) -> anyhow::Result<&PathBuf> {
    let Some(dir) = dir else {
        return Err(anyhow!(
            "directory not specified; run `tbm init` to configure"
        ));
    };

    match check_init(dir) {
        DirectoryState::Initialised => Ok(dir),
        DirectoryState::NotExists => Err(anyhow!(
            "directory does not exist; run `tbm init` to initialise it"
        )),
        DirectoryState::NotInitialised => Err(anyhow!(
            "directory is not correctly initialised; run `tbm init` to initialise it"
        )),
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let config = load_config(&cli.config)?;

    let dir = match &cli.dir {
        Some(dir) => &Some(expand_tilde(dir)),
        None => &config.tbm.default_dir,
    };

    match cli.command.unwrap_or(Commands::Open { query: None }) {
        Commands::Init { remote } => {
            mode_init(&dir, &remote)?;
        }
        Commands::Open { query } => {
            let dir = ensure_initialised(dir)?;
            mode_open(dir, query.as_deref())?;
        }
        Commands::Add { file, message } => {
            let dir = ensure_initialised(&dir)?;
            mode_add(dir, &file, message.as_deref())?;
        }
        Commands::List => {
            let dir = ensure_initialised(&dir)?;
            mode_list(dir)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests;
