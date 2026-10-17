// SPDX-License-Identifier: LGPL-2.1-or-later
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use tempfile::{TempDir, tempdir};

struct Case {
    root: TempDir,
    source: PathBuf,
    target: PathBuf,
}

impl Case {
    fn new(phrases: &str, exclusions: &str) -> Self {
        let root = tempdir().unwrap();
        let source = root.path().join("source");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("userdata-cht.txt"), phrases).unwrap();
        fs::write(source.join("exclude-phrases-cht.txt"), exclusions).unwrap();
        let target = root.path().join("destination/v4/user_dict.csv");
        Self {
            root,
            source,
            target,
        }
    }

    fn seed(&self, data: &[u8]) {
        fs::create_dir_all(self.target.parent().unwrap()).unwrap();
        fs::write(&self.target, data).unwrap();
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_vchewing-to-chewing"))
            .arg("--source-dir")
            .arg(&self.source)
            .arg("--target")
            .arg(&self.target)
            .args(args)
            .output()
            .unwrap()
    }

    fn backups(&self) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(self.target.parent().unwrap()) else {
            return vec![];
        };
        entries
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("user_dict.csv.backup-")
            })
            .collect()
    }
}

fn stdout(out: &Output) -> String {
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
    assert!(!out.status.success(), "unexpected success: {}", stdout(out));
}
fn rows(path: &Path) -> Vec<String> {
    let mut lines: Vec<_> = fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    lines.sort();
    lines
}

// Break caught: dry-run creates a target, or quietly reads the other variant.
#[test]
fn default_is_cht_dry_run_without_creating_directories() {
    let c = Case::new("你好 ㄋㄧˇ-ㄏㄠˇ -1.2 #合成註解\n", "");
    fs::write(c.source.join("userdata-chs.txt"), [0xff]).unwrap();
    let out = c.run(&[]);
    success(&out);
    assert!(stdout(&out).contains("mode=dry-run"));
    assert!(stdout(&out).contains("add_words=1"));
    assert!(!c.target.parent().unwrap().exists());
    assert!(!stdout(&out).contains("你好"));
    assert!(out.stderr.is_empty());
}

// Break caught: variant selection merges both files or converts Chinese glyphs.
#[test]
fn chs_keeps_original_glyphs_and_ignores_cht() {
    let c = Case::new("電腦 ㄉㄧㄢˋ-ㄋㄠˇ\n", "");
    fs::write(c.source.join("userdata-chs.txt"), "电脑 ㄉㄧㄢˋ-ㄋㄠˇ\n").unwrap();
    let out = c.run(&["--variant", "chs", "--apply"]);
    success(&out);
    assert_eq!(rows(&c.target), ["电脑,ㄉㄧㄢˋ ㄋㄠˇ,10"]);
}

// Break caught: target wins is lost, bans get resurrected, or polyphonic pairs collapse.
#[test]
fn merge_preserves_target_and_exclusion_precedence_and_is_idempotent() {
    let c = Case::new(
        "你好 ㄋㄧˇ-ㄏㄠˇ -2 #示例\n你好 ㄋㄧˇ-ㄏㄠˇ\n行 ㄒㄧㄥˊ\n行 ㄏㄤˊ\n秘 ㄇㄧˋ\n禁 ㄐㄧㄣˋ\n留 ㄌㄧㄡˊ\n",
        "你好 ㄋㄧˇ-ㄏㄠˇ\n秘 ㄇㄧˋ\n留 ㄌㄧㄡˊ\n",
    );
    let original = "留,ㄌㄧㄡˊ,37\r\n禁,ㄐㄧㄣˋ,-100".as_bytes();
    c.seed(original);
    let preview = c.run(&[]);
    success(&preview);
    assert_eq!(fs::read(&c.target).unwrap(), original);
    assert!(c.backups().is_empty());
    let out = c.run(&["--apply"]);
    success(&out);
    assert!(stdout(&out).contains("add_words=2"));
    assert!(stdout(&out).contains("add_bans=2"));
    let data = fs::read(&c.target).unwrap();
    assert!(data.starts_with(original));
    assert_eq!(
        rows(&c.target),
        [
            "你好,ㄋㄧˇ ㄏㄠˇ,-100",
            "留,ㄌㄧㄡˊ,37",
            "禁,ㄐㄧㄣˋ,-100",
            "秘,ㄇㄧˋ,-100",
            "行,ㄏㄤˊ,10",
            "行,ㄒㄧㄥˊ,10"
        ]
    );
    assert_eq!(c.backups().len(), 1);
    assert_eq!(fs::read(&c.backups()[0]).unwrap(), original);
    let rerun = c.run(&["--apply"]);
    success(&rerun);
    assert!(stdout(&rerun).contains("add_words=0"));
    assert!(stdout(&rerun).contains("add_bans=0"));
    assert_eq!(fs::read(&c.target).unwrap(), data);
    assert_eq!(c.backups().len(), 1);
    assert!(!stdout(&out).contains("秘"));
}

// Break caught: bad input gets silently inserted or reasons aren't counted.
#[test]
fn malformed_source_lines_are_counted_without_disclosing_content() {
    let c = Case::new(
        "#註解\n\n缺欄\n符號 _punctuation\n拼音 ni3-hao3\n壞音 ㄋㄋ\n錯長 ㄘㄨㄛˋ\n壞分 ㄏㄨㄞˋ-ㄈㄣ NaN\n逗,號 ㄉㄡˋ-ㄏㄠˋ\n好 ㄏㄠˇ\n",
        "",
    );
    let out = c.run(&["--apply"]);
    success(&out);
    let report = stdout(&out);
    assert!(report.contains("add_words=1"));
    assert!(report.contains("skipped=9"));
    for reason in [
        "comment=1",
        "blank=1",
        "bad_line=1",
        "symbol_key=1",
        "invalid_reading=2",
        "length_mismatch=1",
        "invalid_weight=1",
        "unrepresentable_word=1",
    ] {
        assert!(report.contains(reason), "missing {reason}: {report}");
    }
    assert_eq!(rows(&c.target), ["好,ㄏㄠˇ,10"]);
    assert!(!report.contains("缺欄"));
    assert!(out.stderr.is_empty());
}

// Break caught: a hard-coded length 10 or byte count rejects valid words.
#[test]
fn eleven_syllables_and_graphemes_are_supported_twelve_are_skipped() {
    let phrases = format!(
        "{} {}\n{} {}\n禰\u{e0100} ㄋㄧˇ\n",
        "哈".repeat(11),
        ["ㄏㄚ"; 11].join("-"),
        "哈".repeat(12),
        ["ㄏㄚ"; 12].join("-")
    );
    let c = Case::new(&phrases, "");
    let out = c.run(&["--apply"]);
    success(&out);
    assert!(stdout(&out).contains("add_words=2"));
    assert!(stdout(&out).contains("too_long=1"));
    assert_eq!(rows(&c.target).len(), 2);
}

// Break caught: whitespace/BOM handling treats genuine source entries as garbage.
#[test]
fn source_whitespace_bom_and_trailing_comments_do_not_change_words() {
    let c = Case::new(
        "\u{feff}#格式\r\n　你好\tㄋㄧˇ-ㄏㄠˇ\u{a0}-0.5 #注\r\n的 ㄉㄜ˙ #輕聲\n",
        "",
    );
    let out = c.run(&["--apply"]);
    success(&out);
    assert_eq!(rows(&c.target), ["你好,ㄋㄧˇ ㄏㄠˇ,10", "的,ㄉㄜ˙,10"]);
}

// Break caught: staging first suppresses the upstream legacy migration trigger.
#[test]
fn unmigrated_legacy_stops_before_making_v4_or_backup() {
    for v4_exists in [false, true] {
        let c = Case::new("好 ㄏㄠˇ\n", "");
        let base = c.target.parent().unwrap().parent().unwrap();
        fs::create_dir_all(base).unwrap();
        fs::write(base.join("chewing.dat"), b"synthetic legacy").unwrap();
        if v4_exists {
            fs::create_dir(c.target.parent().unwrap()).unwrap();
        }
        let out = c.run(&["--apply"]);
        failure(&out);
        assert!(String::from_utf8_lossy(&out.stderr).contains("legacy_unmigrated"));
        assert!(!c.target.exists());
        assert_eq!(c.target.parent().unwrap().exists(), v4_exists);
        assert!(c.backups().is_empty());
    }
}

// Break caught: an old file retained after successful migration blocks valid v4.
#[test]
fn migrated_target_with_legacy_file_can_be_merged() {
    let c = Case::new("好 ㄏㄠˇ\n", "");
    c.seed(b"");
    fs::write(
        c.target
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("chewing.dat"),
        b"old",
    )
    .unwrap();
    success(&c.run(&["--apply"]));
    assert_eq!(rows(&c.target), ["好,ㄏㄠˇ,10"]);
}

// Break caught: upstream parser drops, clamps, normalizes, or ignores target data.
#[test]
fn lossy_targets_are_rejected_unchanged_in_both_modes() {
    let mut cases = vec![vec![0xff]];
    for text in [
        "秘密\n",
        "秘密,abc,10\n",
        "秘密,ㄇㄧˋ ㄇㄧˋ,101\n",
        "秘密,ㄇㄧˋ ㄇㄧˋ,no\n",
        "秘密,ㄇㄧˋ ㄇㄧˋ,10,extra\n",
        "秘密,ㄇㄧˋ ㄇㄧˋ\n",
        "\n",
        "#秘密\n",
        "好,ㄏㄠˇ,10\n好,ㄏㄠˇ,10\n",
        "好,ㄏㄠˇ ㄏㄠˇ,10\n",
        "好,ㄏㄠˇ  ㄏㄠˇ,10\n",
        "好,˙,10\n",
        "好,ㄏㄠˉ,10\n",
    ] {
        cases.push(text.as_bytes().to_vec());
    }
    for original in cases {
        let c = Case::new("新 ㄒㄧㄣ\n", "");
        c.seed(&original);
        for args in [&[][..], &["--apply"][..]] {
            let out = c.run(args);
            failure(&out);
            assert!(String::from_utf8_lossy(&out.stderr).contains("target_not_lossless"));
            assert!(!String::from_utf8_lossy(&out.stderr).contains("秘密"));
            assert_eq!(fs::read(&c.target).unwrap(), original);
            assert!(c.backups().is_empty());
        }
    }
}

#[test]
fn missing_target_argument_and_invalid_variant_are_errors() {
    let out = Command::new(env!("CARGO_BIN_EXE_vchewing-to-chewing"))
        .output()
        .unwrap();
    failure(&out);
    let c = Case::new("好 ㄏㄠˇ\n", "");
    failure(&c.run(&["--variant", "both"]));
    failure(&c.run(&["--unknown"]));
    assert!(!c.target.exists());
}

#[test]
fn target_path_must_name_v4_user_dictionary() {
    let mut c = Case::new("好 ㄏㄠˇ\n", "");
    c.target = c.root.path().join("chewing.dat");
    failure(&c.run(&["--apply"]));
    assert!(!c.target.exists());
}

#[test]
fn unreadable_source_is_fatal_but_missing_exclusion_is_empty() {
    let c = Case::new("好 ㄏㄠˇ\n", "");
    fs::remove_file(c.source.join("exclude-phrases-cht.txt")).unwrap();
    success(&c.run(&[]));
    fs::remove_file(c.source.join("userdata-cht.txt")).unwrap();
    failure(&c.run(&["--apply"]));
    assert!(!c.target.exists());
}

#[test]
fn invalid_utf8_source_line_is_counted_and_other_lines_survive() {
    let c = Case::new("", "");
    let mut bytes = vec![0xff, b'\n'];
    bytes.extend_from_slice("好 ㄏㄠˇ\n".as_bytes());
    fs::write(c.source.join("userdata-cht.txt"), bytes).unwrap();
    let out = c.run(&["--apply"]);
    success(&out);
    assert!(stdout(&out).contains("invalid_utf8=1"));
    assert_eq!(rows(&c.target), ["好,ㄏㄠˇ,10"]);
}

#[test]
fn zero_additions_never_write_or_create_backups() {
    let c = Case::new("#只有註解\n", "");
    let out = c.run(&["--apply"]);
    success(&out);
    assert!(stdout(&out).contains("add_words=0"));
    assert!(!c.target.parent().unwrap().exists());
    c.seed(b"");
    success(&c.run(&["--apply"]));
    assert!(c.backups().is_empty());
    assert_eq!(fs::read(&c.target).unwrap(), b"");
}

#[test]
fn two_runs_share_one_target_without_automatic_variant_merging() {
    let c = Case::new("電腦 ㄉㄧㄢˋ-ㄋㄠˇ\n", "");
    fs::write(c.source.join("userdata-chs.txt"), "电脑 ㄉㄧㄢˋ-ㄋㄠˇ\n").unwrap();
    success(&c.run(&["--apply"]));
    assert_eq!(rows(&c.target).len(), 1);
    success(&c.run(&["--variant", "chs", "--apply"]));
    assert_eq!(
        rows(&c.target),
        ["电脑,ㄉㄧㄢˋ ㄋㄠˇ,10", "電腦,ㄉㄧㄢˋ ㄋㄠˇ,10"]
    );
    assert_eq!(c.backups().len(), 1);
}

#[cfg(unix)]
#[test]
fn symlink_target_is_rejected_without_touching_its_referent() {
    use std::os::unix::fs::symlink;
    let c = Case::new("好 ㄏㄠˇ\n", "");
    let other = c.root.path().join("other.csv");
    fs::write(&other, b"").unwrap();
    fs::create_dir_all(c.target.parent().unwrap()).unwrap();
    symlink(&other, &c.target).unwrap();
    failure(&c.run(&["--apply"]));
    assert_eq!(fs::read(other).unwrap(), b"");
    assert!(c.backups().is_empty());
}

#[cfg(unix)]
#[test]
fn backup_creation_failure_leaves_target_untouched() {
    use std::os::unix::fs::PermissionsExt;
    let c = Case::new("好 ㄏㄠˇ\n", "");
    c.seed(b"");
    let parent = c.target.parent().unwrap();
    fs::set_permissions(parent, fs::Permissions::from_mode(0o500)).unwrap();
    let out = c.run(&["--apply"]);
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700)).unwrap();
    failure(&out);
    assert_eq!(fs::read(&c.target).unwrap(), b"");
    assert!(c.backups().is_empty());
}
