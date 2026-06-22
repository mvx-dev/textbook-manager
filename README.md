![image](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)
![image](https://img.shields.io/badge/GPL--3.0-red?style=for-the-badge)

# Textbook Manager
This is a simple CLI tool to manage my textbooks PDFs, or any other file that I need.

# Usage
There are two modes to this: `add` and `open`. If not specified, `open` mode will be used by default.

In `add` mode, pass a path to a (usually) PDF. This will then copy that PDF to the textbook directory (in this case it defaults to `~/Documents/textbooks/`), track it with `git-lfs`, and commit it to the repo configured there.

In `open` mode, a textbook can be opened in one of two ways. If no name is passed, then it will open a basic fuzzy-finding interface to select a book. If a name is passed then it will attempt to open the book with the closest match to that name. Eventually this will also be fuzzy find, but at this point is only all lowercase and so if more than one match is made it will just default to the interactive interface with all matches.

Note that currently the program assumes the textbook directory is already setup. To setup a new directory, just create a git repo with a remote, then ensure lfs is installed with `git lfs install`. There isn't currently a way to change the default directory to this new directory, you have to use the `--dir` flag each time you invoke the program.

# Future Features
Eventually I want to add the following features

- Proper fuzzy-finding (both interface and passed names)
- Directory initialisation (setup the textbook directory for you, currently it assumes this directory is already initialised)
- Configuration
    - Default commit message
    - Default textbook directory
- Directory structure. I would like to have subdirectories so I can break them up by subject (i.e. electromagnetism, quantum mechanics, calculus, etc.)

---

This project is released under the GPL V3 license. See LICENSE for details.
