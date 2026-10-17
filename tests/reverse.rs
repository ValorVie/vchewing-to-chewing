// SPDX-License-Identifier: LGPL-2.1-or-later
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{TempDir, tempdir};

struct Case {
    _root: TempDir,
    source: PathBuf,
    target: PathBuf,
}

impl Case {
    fn new(csv: &[u8]) -> Self {
        let root = tempdir().unwrap();
        let source = root.path().join("chewing/v4/user_dict.csv");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, csv).unwrap();
        let target = root.path().join("vchewing");
        Self {
            _root: root,
            source,
            target,
        }
    }
    fn file(&self, name: &str) -> PathBuf {
        self.target.join(name)
    }
    fn seed(&self, words: &[u8], bans: &[u8]) {
        fs::create_dir_all(&self.target).unwrap();
        fs::write(self.file("userdata-cht.txt"), words).unwrap();
        fs::write(self.file("exclude-phrases-cht.txt"), bans).unwrap();
    }
    fn run(&self, extra: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_chewing-to-vchewing"))
            .arg("--source")
            .arg(&self.source)
            .arg("--target-dir")
            .arg(&self.target)
            .args(extra)
            .output()
            .unwrap()
    }
    fn forward(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_vchewing-to-chewing"))
            .arg("--source-dir")
            .arg(&self.target)
            .arg("--target")
            .arg(&self.source)
            .arg("--apply")
            .output()
            .unwrap()
    }
    fn backups(&self) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(&self.target) else {
            return vec![];
        };
        entries
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains(".backup-")
            })
            .collect()
    }
}

fn output(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).unwrap()
}
fn success(out: &Output) {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
fn failure(out: &Output) {
    assert!(!out.status.success(), "unexpected success: {}", output(out));
}
fn rows(path: &Path) -> Vec<String> {
    let mut rows: Vec<_> = fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(str::to_owned)
        .collect();
    rows.sort();
    rows
}

#[test]
fn reverse_default_dry_run_never_creates_target_or_discloses_words() {
    let csv = "你好,ㄋㄧˇ ㄏㄠˇ,10\n免,ㄇㄧㄢˇ,-100\n".as_bytes();
    let c = Case::new(csv);
    let out = c.run(&[]);
    success(&out);
    assert!(output(&out).contains("mode=dry-run variant=cht add_words=1 add_bans=1"));
    assert!(!c.target.exists());
    assert_eq!(fs::read(&c.source).unwrap(), csv);
    assert!(!output(&out).contains("你好"));
    assert!(out.stderr.is_empty());
}

#[test]
fn only_minimum_boost_is_a_ban_other_negative_preferences_are_words() {
    let c = Case::new(
        "免,ㄇㄧㄢˇ,-100\n低,ㄉㄧ,-99\n減,ㄐㄧㄢˇ,-10\n零,ㄌㄧㄥˊ,0\n高,ㄍㄠ,100\n".as_bytes(),
    );
    let out = c.run(&["--apply"]);
    success(&out);
    assert!(output(&out).contains("add_words=4 add_bans=1"));
    assert_eq!(
        rows(&c.file("userdata-cht.txt")),
        ["低 ㄉㄧ", "減 ㄐㄧㄢˇ", "零 ㄌㄧㄥˊ", "高 ㄍㄠ"]
    );
    assert_eq!(rows(&c.file("exclude-phrases-cht.txt")), ["免 ㄇㄧㄢˇ"]);
}

#[test]
fn existing_pairs_in_either_file_win_bytes_and_backups_are_preserved() {
    let c = Case::new("留,ㄌㄧㄡˊ,-100\n禁,ㄐㄧㄣˋ,10\n好,ㄏㄠˇ,10\n免,ㄇㄧㄢˇ,-100\n".as_bytes());
    let words = "\u{feff}# synthetic header\r留 ㄌㄧㄡˊ -2 # original".as_bytes();
    let bans = "# synthetic filter\x0c禁 ㄐㄧㄣˋ\x0c".as_bytes();
    c.seed(words, bans);
    let preview = c.run(&[]);
    success(&preview);
    assert_eq!(fs::read(c.file("userdata-cht.txt")).unwrap(), words);
    assert_eq!(fs::read(c.file("exclude-phrases-cht.txt")).unwrap(), bans);
    assert!(c.backups().is_empty());
    let out = c.run(&["--apply"]);
    success(&out);
    assert!(output(&out).contains("add_words=1 add_bans=1 skipped=2"));
    assert!(
        fs::read(c.file("userdata-cht.txt"))
            .unwrap()
            .starts_with(words)
    );
    assert!(
        fs::read(c.file("exclude-phrases-cht.txt"))
            .unwrap()
            .starts_with(bans)
    );
    assert_eq!(c.backups().len(), 2);
    for (name, original) in [
        ("userdata-cht.txt", words),
        ("exclude-phrases-cht.txt", bans),
    ] {
        let backup = c
            .backups()
            .into_iter()
            .find(|p| p.file_name().unwrap().to_string_lossy().starts_with(name))
            .unwrap();
        assert_eq!(fs::read(backup).unwrap(), original);
    }
    let before = (
        fs::read(c.file("userdata-cht.txt")).unwrap(),
        fs::read(c.file("exclude-phrases-cht.txt")).unwrap(),
    );
    let rerun = c.run(&["--apply"]);
    success(&rerun);
    assert!(output(&rerun).contains("add_words=0 add_bans=0 skipped=4"));
    assert_eq!(fs::read(c.file("userdata-cht.txt")).unwrap(), before.0);
    assert_eq!(
        fs::read(c.file("exclude-phrases-cht.txt")).unwrap(),
        before.1
    );
    assert_eq!(c.backups().len(), 2);
}

#[test]
fn chs_writes_selected_files_without_converting_or_reading_cht() {
    let c = Case::new("电脑,ㄉㄧㄢˋ ㄋㄠˇ,10\n免,ㄇㄧㄢˇ,-100\n".as_bytes());
    c.seed(&[0xff], &[0xff]);
    let out = c.run(&["--variant", "chs", "--apply"]);
    success(&out);
    assert_eq!(rows(&c.file("userdata-chs.txt")), ["电脑 ㄉㄧㄢˋ-ㄋㄠˇ"]);
    assert_eq!(rows(&c.file("exclude-phrases-chs.txt")), ["免 ㄇㄧㄢˇ"]);
    assert_eq!(fs::read(c.file("userdata-cht.txt")).unwrap(), [0xff]);
    assert!(c.backups().is_empty());
}

#[test]
fn malformed_chewing_source_stops_before_any_target_write() {
    for csv in [
        &[0xff][..],
        "秘密,abc,10\n".as_bytes(),
        "秘密,ㄇㄧˋ ㄇㄧˋ,101\n".as_bytes(),
        "秘密,ㄇㄧˋ ㄇㄧˋ\n".as_bytes(),
        "好,ㄏㄠˇ,10\n好,ㄏㄠˇ,10\n".as_bytes(),
    ] {
        let c = Case::new(csv);
        for args in [&[][..], &["--apply"][..]] {
            let out = c.run(args);
            failure(&out);
            assert!(!c.target.exists());
            assert!(!String::from_utf8_lossy(&out.stderr).contains("秘密"));
        }
        assert_eq!(fs::read(&c.source).unwrap(), csv);
    }
}

#[test]
fn a_word_that_would_become_a_vchewing_comment_stops_in_both_modes() {
    let c = Case::new("#,ㄐㄧㄥˇ,10\n".as_bytes());
    for args in [&[][..], &["--apply"][..]] {
        let out = c.run(args);
        failure(&out);
        assert!(String::from_utf8_lossy(&out.stderr).contains("source_not_representable"));
        assert!(!c.target.exists());
    }
}

#[test]
fn both_vchewing_targets_are_validated_before_any_append_or_backup() {
    let c = Case::new("好,ㄏㄠˇ,10\n免,ㄇㄧㄢˇ,-100\n".as_bytes());
    let words = b"# keep this header\n";
    for bad in [
        "壞行\n".as_bytes(),
        &[0xff][..],
        "符號 _punctuation\n".as_bytes(),
    ] {
        c.seed(words, bad);
        let out = c.run(&["--apply"]);
        failure(&out);
        assert_eq!(fs::read(c.file("userdata-cht.txt")).unwrap(), words);
        assert_eq!(fs::read(c.file("exclude-phrases-cht.txt")).unwrap(), bad);
        assert!(c.backups().is_empty());
    }
}

#[test]
fn vchewing_to_chewing_and_back_adds_nothing_and_preserves_tones() {
    let c = Case::new(b"");
    let words = "# header\r\n你好 ㄋㄧˇ-ㄏㄠˇ -1 # comment\r\n的 ㄉㄜ˙\r\n新 ㄒㄧㄣ\r\n行 ㄒㄧㄥˊ\r\n行 ㄏㄤˊ\r\n".as_bytes();
    let bans = "免 ㄇㄧㄢˇ\n".as_bytes();
    c.seed(words, bans);
    success(&c.forward());
    let out = c.run(&["--apply"]);
    success(&out);
    assert!(output(&out).contains("add_words=0 add_bans=0 skipped=6"));
    assert_eq!(fs::read(c.file("userdata-cht.txt")).unwrap(), words);
    assert_eq!(fs::read(c.file("exclude-phrases-cht.txt")).unwrap(), bans);
    assert!(c.backups().is_empty());
}

#[test]
fn chewing_to_vchewing_and_back_adds_nothing_and_preserves_boosts() {
    let csv = "的,ㄉㄜ˙,-10\r\n新,ㄒㄧㄣ,10\r\n行,ㄒㄧㄥˊ,0\r\n行,ㄏㄤˊ,100\r\n免,ㄇㄧㄢˇ,-100"
        .as_bytes();
    let c = Case::new(csv);
    success(&c.run(&["--apply"]));
    let out = c.forward();
    success(&out);
    assert!(output(&out).contains("add_words=0 add_bans=0"));
    assert_eq!(fs::read(&c.source).unwrap(), csv);
    assert_eq!(
        rows(&c.file("userdata-cht.txt")),
        ["新 ㄒㄧㄣ", "的 ㄉㄜ˙", "行 ㄏㄤˊ", "行 ㄒㄧㄥˊ"]
    );
}

#[test]
fn empty_source_and_unmodified_file_are_noops() {
    let c = Case::new(b"");
    let out = c.run(&["--apply"]);
    success(&out);
    assert!(output(&out).contains("add_words=0 add_bans=0"));
    assert!(!c.target.exists());
    fs::write(&c.source, "好,ㄏㄠˇ,10\n").unwrap();
    success(&c.run(&["--apply"]));
    assert_eq!(rows(&c.file("userdata-cht.txt")), ["好 ㄏㄠˇ"]);
    assert!(rows(&c.file("exclude-phrases-cht.txt")).is_empty());
}

#[test]
fn ban_only_source_creates_a_readable_pair_of_files_for_roundtrip() {
    let csv = "免,ㄇㄧㄢˇ,-100\n".as_bytes();
    let c = Case::new(csv);
    success(&c.run(&["--apply"]));
    let out = c.forward();
    success(&out);
    assert!(output(&out).contains("add_words=0 add_bans=0"));
    assert_eq!(fs::read(&c.source).unwrap(), csv);
    assert!(rows(&c.file("userdata-cht.txt")).is_empty());
}

#[test]
fn misplaced_source_reports_the_required_path_format_without_writes() {
    let csv = "秘密,ㄇㄧˋ ㄇㄧˋ,10\n".as_bytes();
    let mut c = Case::new(csv);
    let misplaced = c._root.path().join("not-v4/user_dict.csv");
    fs::create_dir_all(misplaced.parent().unwrap()).unwrap();
    fs::rename(&c.source, &misplaced).unwrap();
    c.source = misplaced;
    for args in [&[][..], &["--apply"][..]] {
        let out = c.run(args);
        failure(&out);
        let error = String::from_utf8_lossy(&out.stderr);
        assert!(error.contains("target_path: expected BASE/v4/user_dict.csv"));
        assert!(!error.contains("秘密"));
        assert!(out.stdout.is_empty());
        assert!(!c.target.exists());
        assert_eq!(fs::read(&c.source).unwrap(), csv);
    }
}

#[test]
fn missing_source_and_invalid_arguments_stop_without_writes() {
    let c = Case::new(b"");
    fs::remove_file(&c.source).unwrap();
    failure(&c.run(&["--apply"]));
    failure(&c.run(&["--variant", "both"]));
    assert!(!c.target.exists());
    let help = Command::new(env!("CARGO_BIN_EXE_chewing-to-vchewing"))
        .arg("--help")
        .output()
        .unwrap();
    success(&help);
    assert!(output(&help).contains("chewing-to-vchewing --source"));
    let missing = Command::new(env!("CARGO_BIN_EXE_chewing-to-vchewing"))
        .output()
        .unwrap();
    failure(&missing);
}

#[cfg(unix)]
#[test]
fn symlink_destination_is_rejected_before_other_file_changes() {
    use std::os::unix::fs::symlink;
    let c = Case::new("好,ㄏㄠˇ,10\n免,ㄇㄧㄢˇ,-100\n".as_bytes());
    c.seed(b"# keep\n", b"");
    fs::remove_file(c.file("exclude-phrases-cht.txt")).unwrap();
    let referent = c._root.path().join("other.txt");
    fs::write(&referent, b"").unwrap();
    symlink(&referent, c.file("exclude-phrases-cht.txt")).unwrap();
    failure(&c.run(&["--apply"]));
    assert_eq!(fs::read(c.file("userdata-cht.txt")).unwrap(), b"# keep\n");
    assert_eq!(fs::read(referent).unwrap(), b"");
    assert!(c.backups().is_empty());
}

#[cfg(unix)]
#[test]
fn backup_failure_keeps_both_original_files() {
    use std::os::unix::fs::PermissionsExt;
    let c = Case::new("好,ㄏㄠˇ,10\n免,ㄇㄧㄢˇ,-100\n".as_bytes());
    c.seed(b"# word header\n", b"# ban header\n");
    fs::set_permissions(&c.target, fs::Permissions::from_mode(0o500)).unwrap();
    let out = c.run(&["--apply"]);
    fs::set_permissions(&c.target, fs::Permissions::from_mode(0o700)).unwrap();
    failure(&out);
    assert_eq!(
        fs::read(c.file("userdata-cht.txt")).unwrap(),
        b"# word header\n"
    );
    assert_eq!(
        fs::read(c.file("exclude-phrases-cht.txt")).unwrap(),
        b"# ban header\n"
    );
    assert!(c.backups().is_empty());
}
