use super::*;
use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard};
use tempfile::TempDir;

// ---------- helpers ----------

static ENV_LOCK: Mutex<()> = Mutex::new(());

/// Sets $HOME for the duration of a test and restores it afterwards.
/// Holds a lock so env-mutating tests don't race each other.
struct HomeGuard {
    old: Option<OsString>,
    _lock: MutexGuard<'static, ()>,
}

impl HomeGuard {
    fn set(home: &Path) -> Self {
        let lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var_os("HOME");
        #[allow(unused_unsafe)]
        unsafe {
            std::env::set_var("HOME", home)
        };
        Self { old, _lock: lock }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        #[allow(unused_unsafe)]
        unsafe {
            match &self.old {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }
}

fn touch(root: &Path, rel: &str) -> PathBuf {
    let p = root.join(rel);
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&p, b"x").unwrap();
    p
}

fn rel_sorted(root: &Path, paths: Vec<PathBuf>) -> Vec<String> {
    let mut v: Vec<String> = paths
        .iter()
        .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

fn config_arg(tmp: &TempDir) -> Option<String> {
    Some(tmp.path().to_str().unwrap().to_owned())
}

// ---------- expand_tilde ----------

#[test]
fn expand_tilde_replaces_leading_tilde_slash() {
    let _g = HomeGuard::set(Path::new("/home/tester"));
    assert_eq!(
        expand_tilde("~/Documents/textbooks"),
        PathBuf::from("/home/tester/Documents/textbooks")
    );
}

#[test]
fn expand_tilde_expands_bare_tilde() {
    let _g = HomeGuard::set(Path::new("/home/tester"));
    assert_eq!(expand_tilde("~"), PathBuf::from("/home/tester"));
}

#[test]
fn expand_tilde_leaves_absolute_and_relative_paths_alone() {
    let _g = HomeGuard::set(Path::new("/home/tester"));
    assert_eq!(expand_tilde("/abs/path"), PathBuf::from("/abs/path"));
    assert_eq!(expand_tilde("rel/path"), PathBuf::from("rel/path"));
    assert_eq!(expand_tilde(""), PathBuf::from(""));
}

#[test]
fn expand_tilde_only_expands_a_leading_tilde() {
    let _g = HomeGuard::set(Path::new("/home/tester"));
    assert_eq!(expand_tilde("/a/~/b"), PathBuf::from("/a/~/b"));
    assert_eq!(expand_tilde("a~/b"), PathBuf::from("a~/b"));
}

#[test]
fn expand_tilde_does_not_expand_other_users() {
    let _g = HomeGuard::set(Path::new("/home/tester"));
    assert_eq!(expand_tilde("~root/x"), PathBuf::from("~root/x"));
}

#[test]
fn expand_tilde_ext_trait_works_on_str_and_string() {
    let _g = HomeGuard::set(Path::new("/home/tester"));
    assert_eq!("~/x".expand_tilde(), PathBuf::from("/home/tester/x"));
    assert_eq!(
        String::from("~/x").expand_tilde(),
        PathBuf::from("/home/tester/x")
    );
}

// ---------- dir_to_str ----------

#[test]
fn dir_to_str_returns_str_for_valid_utf8() {
    assert_eq!(dir_to_str(Path::new("/tmp/books")).unwrap(), "/tmp/books");
}

#[cfg(unix)]
#[test]
fn dir_to_str_rejects_non_utf8_paths() {
    use std::os::unix::ffi::OsStrExt;
    let p = Path::new(OsStr::from_bytes(b"/tmp/\xff\xfe"));
    assert!(dir_to_str(p).is_err());
}

// ---------- run_cmd ----------

#[cfg(unix)]
#[test]
fn run_cmd_ok_on_zero_exit() {
    assert!(run_cmd("true", &[]).is_ok());
}

#[cfg(unix)]
#[test]
fn run_cmd_errors_on_nonzero_exit_and_names_the_command() {
    let err = run_cmd("false", &[]).unwrap_err().to_string();
    assert!(err.contains("false"), "error was: {err}");
}

#[test]
fn run_cmd_errors_when_binary_does_not_exist() {
    assert!(run_cmd("definitely-not-a-real-binary-xyz", &[]).is_err());
}

// ---------- list_pdfs ----------

#[test]
fn list_pdfs_finds_nested_pdfs_only() {
    let tmp = TempDir::new().unwrap();
    let r = tmp.path();
    touch(r, "a.pdf");
    touch(r, "sub/deeper/b.pdf");
    touch(r, "notes.txt");
    touch(r, "notes.pdf.txt");
    touch(r, "pdf"); // no extension
    touch(r, "archive.pdf.bak");

    let got = rel_sorted(r, list_pdfs(r).unwrap());
    assert_eq!(got, vec!["a.pdf", "sub/deeper/b.pdf"]);
}

#[test]
fn list_pdfs_ignores_directories_named_like_pdfs() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir_all(tmp.path().join("weird.pdf")).unwrap();
    touch(tmp.path(), "real.pdf");
    let got = rel_sorted(tmp.path(), list_pdfs(tmp.path()).unwrap());
    assert_eq!(got, vec!["real.pdf"]);
}

#[test]
fn list_pdfs_matches_extension_case_insensitively() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "UPPER.PDF");
    touch(tmp.path(), "Mixed.Pdf");
    let got = rel_sorted(tmp.path(), list_pdfs(tmp.path()).unwrap());
    assert_eq!(got, vec!["Mixed.Pdf", "UPPER.PDF"]);
}

#[test]
fn list_pdfs_skips_the_git_directory() {
    let tmp = TempDir::new().unwrap();
    touch(tmp.path(), "book.pdf");
    touch(tmp.path(), ".git/stray.pdf");
    let got = rel_sorted(tmp.path(), list_pdfs(tmp.path()).unwrap());
    assert_eq!(got, vec!["book.pdf"]);
}

#[test]
fn list_pdfs_empty_dir_gives_empty_vec() {
    let tmp = TempDir::new().unwrap();
    assert!(list_pdfs(tmp.path()).unwrap().is_empty());
}

/// Assumption: output should be deterministic. WalkDir's default order is
/// filesystem-dependent, so `tbm list` output can change between machines.
#[test]
fn list_pdfs_returns_sorted_order() {
    let tmp = TempDir::new().unwrap();
    for f in ["z.pdf", "b.pdf", "a/q.pdf", "a/c.pdf", "m.pdf"] {
        touch(tmp.path(), f);
    }
    let got: Vec<String> = list_pdfs(tmp.path())
        .unwrap()
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let mut want = got.clone();
    want.sort();
    assert_eq!(got, want);
}

// ---------- fuzzy_find (non-interactive paths only) ----------
// The zero-match and many-match paths launch skim, so they can't run here
// without refactoring the picker behind a trait/closure.

#[test]
fn fuzzy_find_single_match_is_case_insensitive() {
    let files = vec![
        PathBuf::from("/b/Griffiths_QM.pdf"),
        PathBuf::from("/b/feynman_vol1.pdf"),
    ];
    assert_eq!(fuzzy_find(&files, "GRIFF").unwrap(), Some(files[0].clone()));
    assert_eq!(fuzzy_find(&files, "vol1").unwrap(), Some(files[1].clone()));
}

#[test]
fn fuzzy_find_matches_file_name_not_directory() {
    let files = vec![
        PathBuf::from("/b/quantum/intro.pdf"),
        PathBuf::from("/b/other/griffiths_quantum.pdf"),
    ];
    // only the second file *name* contains "quantum"
    assert_eq!(
        fuzzy_find(&files, "quantum").unwrap(),
        Some(files[1].clone())
    );
}

// ---------- check_init ----------

#[test]
fn check_init_not_exists() {
    let tmp = TempDir::new().unwrap();
    assert!(matches!(
        check_init(&tmp.path().join("nope")),
        DirectoryState::NotExists
    ));
}

#[test]
fn check_init_existing_dir_without_git_is_not_initialised() {
    let tmp = TempDir::new().unwrap();
    assert!(matches!(
        check_init(tmp.path()),
        DirectoryState::NotInitialised
    ));
}

#[test]
fn check_init_dir_with_git_is_initialised() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join(".git")).unwrap();
    assert!(matches!(
        check_init(tmp.path()),
        DirectoryState::Initialised
    ));
}

// ---------- ensure_initialised ----------

#[test]
fn ensure_initialised_errors_when_no_dir_given() {
    let err = ensure_initialised(&None).unwrap_err().to_string();
    assert!(err.contains("not specified"), "{err}");
    assert!(err.contains("tbm init"), "{err}");
}

#[test]
fn ensure_initialised_errors_when_dir_missing() {
    let tmp = TempDir::new().unwrap();
    let dir = Some(tmp.path().join("missing"));
    let err = ensure_initialised(&dir).unwrap_err().to_string();
    assert!(err.contains("does not exist"), "{err}");
}

#[test]
fn ensure_initialised_errors_when_dir_not_a_repo() {
    let tmp = TempDir::new().unwrap();
    let dir = Some(tmp.path().to_path_buf());
    let err = ensure_initialised(&dir).unwrap_err().to_string();
    assert!(err.contains("not correctly initialised"), "{err}");
}

#[test]
fn ensure_initialised_returns_the_dir_when_ready() {
    let tmp = TempDir::new().unwrap();
    fs::create_dir(tmp.path().join(".git")).unwrap();
    let dir = Some(tmp.path().to_path_buf());
    assert_eq!(ensure_initialised(&dir).unwrap().as_path(), tmp.path());
}

// ---------- load_config ----------

#[test]
fn load_config_none_gives_default_config() {
    assert!(load_config(&None).unwrap().tbm.default_dir.is_none());
}

#[test]
fn load_config_reads_default_dir() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("config.toml"),
        "[tbm]\ndefault_dir = \"/srv/books\"\n",
    )
    .unwrap();
    let cfg = load_config(&config_arg(&tmp)).unwrap();
    assert_eq!(cfg.tbm.default_dir, Some(PathBuf::from("/srv/books")));
}

#[test]
fn load_config_section_without_keys_means_no_default_dir() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("config.toml"), "[tbm]\n").unwrap();
    assert!(
        load_config(&config_arg(&tmp))
            .unwrap()
            .tbm
            .default_dir
            .is_none()
    );
}

#[test]
fn load_config_rejects_malformed_toml() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("config.toml"), "[tbm\ndefault_dir = ").unwrap();
    assert!(load_config(&config_arg(&tmp)).is_err());
}

/// A missing config file must not be fatal: otherwise `tbm init` can never
/// run on a fresh machine (the default --config path always exists as Some).
#[test]
fn load_config_missing_file_falls_back_to_defaults() {
    let tmp = TempDir::new().unwrap(); // no config.toml inside
    let cfg = load_config(&config_arg(&tmp)).expect("missing config should not be an error");
    assert!(cfg.tbm.default_dir.is_none());
}

/// `--dir` gets tilde-expanded in main(); `default_dir` from the file should too.
#[test]
fn load_config_expands_tilde_in_default_dir() {
    let _g = HomeGuard::set(Path::new("/home/tester"));
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("config.toml"),
        "[tbm]\ndefault_dir = \"~/books\"\n",
    )
    .unwrap();
    let cfg = load_config(&config_arg(&tmp)).unwrap();
    assert_eq!(
        cfg.tbm.default_dir,
        Some(PathBuf::from("/home/tester/books"))
    );
}

// ---------- mode_open ----------

#[test]
fn mode_open_errors_when_no_pdfs() {
    let tmp = TempDir::new().unwrap();
    let err = mode_open(tmp.path(), None).unwrap_err().to_string();
    assert!(err.contains("no PDFs found"), "{err}");
}
