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

use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::{
    error::Error,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use toml;
use walkdir::WalkDir;

#[derive(Debug, Deserialize)]
struct Config {
    tbm: TBMConfig,
}

#[derive(Debug, Deserialize)]
struct TBMConfig {
    default_dir: String,
}

#[derive(Parser)]
#[command(name = "tbm")]
#[command(about = "Manage textbook PDFs", long_about = None)]
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

fn load_config(config_dir: &Option<String>) -> Result<Config, Box<dyn Error>> {
    let config = if let Some(dir) = config_dir {
        let path = expand_tilde(&dir);
        let text = fs::read_to_string(path.join("config.toml"))?;
        toml::from_str(&text)?
    } else {
        Config {
            tbm: TBMConfig {
                default_dir: "~/Documents/textbooks".into(),
            },
        }
    };

    Ok(config)
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    let mut config = load_config(&cli.config)?;

    dbg!(&config);

    let dir: PathBuf;
    if let Some(_dir) = &cli.dir {
        dir = expand_tilde(_dir);
    } else {
        dir = expand_tilde(&config.tbm.default_dir);
    }
    dbg!(&dir);

    match cli.command.unwrap_or(Commands::Open { query: None }) {
        Commands::Open { query } => mode_open(&dir, query.as_deref())?,
        Commands::Add { file, message } => mode_add(&dir, &file, message.as_deref())?,
    }

    Ok(())
}
