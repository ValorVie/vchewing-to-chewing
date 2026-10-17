// SPDX-License-Identifier: LGPL-2.1-or-later
use std::{fs, process::Command};

const HEADER: &str = "# 𝙵𝙾𝚁𝙼𝙰𝚃 𝚘𝚛𝚐.𝚊𝚝𝚎𝚕𝚒𝚎𝚛𝙸𝚗𝚖𝚞.𝚟𝚌𝚑𝚎𝚠𝚒𝚗𝚐.𝚞𝚜𝚎𝚛𝙻𝚊𝚗𝚐𝚞𝚊𝚐𝚎𝙼𝚘𝚍𝚎𝚕𝙳𝚊𝚝𝚊.𝚏𝚘𝚛𝚖𝚊𝚝𝚝𝚎𝚍";

// Break caught: CR/FF hide records after a header, or CRLF/blank lines inflate skip counts.
fn check(separator: &str) {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(
        source.join("userdata-cht.txt"),
        [HEADER, "好 ㄏㄠˇ", "", "新 ㄒㄧㄣ", "壞行", ""].join(separator),
    )
    .unwrap();
    fs::write(
        source.join("exclude-phrases-cht.txt"),
        [HEADER, "免 ㄇㄧㄢˇ", ""].join(separator),
    )
    .unwrap();
    let target = root.path().join("destination/v4/user_dict.csv");
    for apply in [false, true] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_vchewing-to-chewing"));
        cmd.arg("--source-dir")
            .arg(&source)
            .arg("--target")
            .arg(&target);
        if apply {
            cmd.arg("--apply");
        }
        let out = cmd.output().unwrap();
        assert!(out.status.success());
        assert!(out.stderr.is_empty());
        let report = String::from_utf8(out.stdout).unwrap();
        assert!(
            report.contains("add_words=2 add_bans=1 skipped=4"),
            "separator={separator:?}: {report}"
        );
        for reason in ["comment=2", "blank=1", "bad_line=1"] {
            assert!(report.contains(reason), "missing {reason}: {report}");
        }
        if !apply {
            assert!(!target.parent().unwrap().exists());
        }
    }
    let mut rows: Vec<_> = fs::read_to_string(target)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    rows.sort();
    assert_eq!(rows, ["免,ㄇㄧㄢˇ,-100", "好,ㄏㄠˇ,10", "新,ㄒㄧㄣ,10"]);
}

#[test]
fn lf_records_after_format_header_keep_counts() {
    check("\n");
}

#[test]
fn crlf_records_after_format_header_match_lf() {
    check("\r\n");
}

#[test]
fn cr_records_after_format_header_match_lf() {
    check("\r");
}

#[test]
fn ff_records_after_format_header_match_lf() {
    check("\x0c");
}
