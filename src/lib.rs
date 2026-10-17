// SPDX-License-Identifier: LGPL-2.1-or-later
//! Shared parsing, validation and file replacement for both conversion CLIs.
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{Cursor, ErrorKind, Write},
    path::{Path, PathBuf},
    process::ExitCode,
};

use bstr::ByteSlice;
use chewing::{
    dictionary::StringTable,
    user::UserDict,
    zhuyin::{Syllable, SyllableVec},
};

type Result<T> = std::result::Result<T, String>;

const HELP: &str = "vchewing-to-chewing --source-dir DIR --target BASE/v4/user_dict.csv [--variant cht|chs] [--apply]
預設 variant=cht、dry-run；--apply 才備份並寫入。每次只讀一個 variant。
只輸出筆數與原因，不輸出詞條。請先關閉使用目標詞庫的輸入法宿主程式。";

const REVERSE_HELP: &str = "chewing-to-vchewing --source BASE/v4/user_dict.csv --target-dir DIR [--variant cht|chs] [--apply]
預設 variant=cht、dry-run；--apply 才備份並追加唯音一般詞與排除檔。
不轉 history_dict.bin 或權重。套用前請切換到其他輸入法或結束唯音。";

const VCHEWING_HEADER: &str = "# 𝙵𝙾𝚁𝙼𝙰𝚃 𝚘𝚛𝚐.𝚊𝚝𝚎𝚕𝚒𝚎𝚛𝙸𝚗𝚖𝚞.𝚟𝚌𝚑𝚎𝚠𝚒𝚗𝚐.𝚞𝚜𝚎𝚛𝙻𝚊𝚗𝚐𝚞𝚊𝚐𝚎𝙼𝚘𝚍𝚎𝚕𝙳𝚊𝚝𝚊.𝚏𝚘𝚛𝚖𝚊𝚝𝚝𝚎𝚍\n";

struct Options {
    source: PathBuf,
    target: PathBuf,
    variant: String,
    apply: bool,
}

fn options(reverse: bool) -> Result<Option<Options>> {
    let source_flag = if reverse { "--source" } else { "--source-dir" };
    let target_flag = if reverse { "--target-dir" } else { "--target" };
    let source_error = if reverse {
        "missing_source"
    } else {
        "missing_source_dir"
    };
    let mut args = env::args_os().skip(1);
    let (mut source, mut target, mut variant, mut apply) = (None, None, None, false);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => return Ok(None),
            Some("--apply") if !apply => apply = true,
            Some(flag) if flag == source_flag && source.is_none() => {
                source = Some(PathBuf::from(args.next().ok_or(source_error)?))
            }
            Some(flag) if flag == target_flag && target.is_none() => {
                target = Some(PathBuf::from(args.next().ok_or("missing_target")?))
            }
            Some("--variant") if variant.is_none() => {
                let value = args.next().ok_or("missing_variant")?;
                if value != "cht" && value != "chs" {
                    return Err("invalid_variant: use cht|chs".into());
                }
                variant = Some(value.to_string_lossy().into_owned());
            }
            _ => return Err("invalid_or_duplicate_argument".into()),
        }
    }
    Ok(Some(Options {
        source: source.ok_or(source_error)?,
        target: target.ok_or("missing_target")?,
        variant: variant.unwrap_or_else(|| "cht".into()),
        apply,
    }))
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Pair {
    syllables: SyllableVec,
    word: String,
}

fn pair(word: &str, reading: &str, separator: char) -> std::result::Result<Pair, &'static str> {
    if word.is_empty()
        || word
            .chars()
            .any(|c| c == ',' || c.is_control() || c.is_whitespace())
    {
        return Err("unrepresentable_word");
    }
    if reading.contains('_') {
        return Err("symbol_key");
    }
    let parts: Vec<_> = reading.split(separator).collect();
    if parts.len() > SyllableVec::MAX_LEN {
        return Err("too_long");
    }
    let mut syllables = SyllableVec::new();
    for part in parts {
        if !part.chars().any(|c| ('ㄅ'..='ㄩ').contains(&c))
            || part
                .chars()
                .any(|c| !('ㄅ'..='ㄩ').contains(&c) && !"ˊˇˋ˙".contains(c))
        {
            return Err("invalid_reading");
        }
        let syllable: Syllable = part.parse().map_err(|_| "invalid_reading")?;
        if syllable.to_string() != part {
            return Err("invalid_reading");
        }
        syllables.push(syllable).map_err(|_| "too_long")?;
    }
    if word.as_bytes().graphemes().count() != syllables.len() {
        return Err("length_mismatch");
    }
    Ok(Pair {
        syllables,
        word: word.into(),
    })
}

#[derive(Default)]
struct Counts {
    words: usize,
    bans: usize,
    skipped: BTreeMap<&'static str, usize>,
}

impl Counts {
    fn skip(&mut self, reason: &'static str) {
        *self.skipped.entry(reason).or_default() += 1;
    }
    fn print(&self, opt: &Options) {
        println!(
            "mode={} variant={} add_words={} add_bans={} skipped={}",
            if opt.apply { "apply" } else { "dry-run" },
            opt.variant,
            self.words,
            self.bans,
            self.skipped.values().sum::<usize>()
        );
        for (reason, count) in &self.skipped {
            println!("{reason}={count}");
        }
    }
}

fn source_line(line: &[u8]) -> std::result::Result<Pair, &'static str> {
    let text = std::str::from_utf8(line)
        .map_err(|_| "invalid_utf8")?
        .trim();
    if text.is_empty() {
        return Err("blank");
    }
    if text.starts_with('#') {
        return Err("comment");
    }
    let cells: Vec<_> = text.split_whitespace().collect();
    if cells.len() < 2 {
        return Err("bad_line");
    }
    let value = pair(cells[0], cells[1], '-')?;
    if let Some(third) = cells.get(2)
        && !third.starts_with('#')
    {
        let weight: f64 = third.parse().map_err(|_| "invalid_weight")?;
        if !weight.is_finite() {
            return Err("invalid_weight");
        }
        if cells.get(3).is_some_and(|c| !c.starts_with('#')) {
            return Err("bad_line");
        }
    }
    Ok(value)
}

fn vchewing_lines(bytes: &[u8]) -> Vec<u8> {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    // Accept vChewing's LF/CR/FF separators; count CRLF once and retain blank lines.
    bytes
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(i, byte)| match byte {
            b'\n' if i > 0 && bytes[i - 1] == b'\r' => None,
            b'\r' | 0x0c => Some(b'\n'),
            _ => Some(byte),
        })
        .collect()
}

fn read_source(opt: &Options, counts: &mut Counts) -> Result<BTreeMap<Pair, bool>> {
    let mut pairs = BTreeMap::new();
    for (name, ban, optional) in [("userdata", false, false), ("exclude-phrases", true, true)] {
        let path = opt.source.join(format!("{name}-{}.txt", opt.variant));
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if optional && e.kind() == ErrorKind::NotFound => continue,
            Err(_) => return Err(format!("source_read_failed: {name}-{}.txt", opt.variant)),
        };
        let normalized = vchewing_lines(&bytes);
        for line in normalized.lines() {
            match source_line(line) {
                Err(reason) => counts.skip(reason),
                Ok(pair) => match pairs.entry(pair) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(ban);
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        if ban && !*entry.get() {
                            entry.insert(true);
                            counts.skip("source_exclusion_wins");
                        } else {
                            counts.skip("duplicate_source");
                        }
                    }
                },
            }
        }
    }
    Ok(pairs)
}

fn target_records(bytes: &[u8]) -> Result<BTreeMap<Pair, i8>> {
    let text = std::str::from_utf8(bytes).map_err(|_| "target_not_lossless: invalid_utf8")?;
    let mut records = BTreeMap::new();
    for (i, line) in text.lines().enumerate() {
        let error = || format!("target_not_lossless: line {}", i + 1);
        let cells: Vec<_> = line.split(',').collect();
        if cells.len() != 3 {
            return Err(error());
        }
        let pair = pair(cells[0], cells[1], ' ').map_err(|_| error())?;
        let boost: i8 = cells[2].parse().map_err(|_| error())?;
        if !(UserDict::MIN..=UserDict::MAX).contains(&boost)
            || records.insert(pair, boost).is_some()
        {
            return Err(error());
        }
    }
    Ok(records)
}

fn serialize(dict: &UserDict) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    dict.to_writer(&mut bytes)
        .map_err(|_| "dictionary_write_failed")?;
    Ok(bytes)
}

struct Target {
    original: Option<Vec<u8>>,
    records: BTreeMap<Pair, i8>,
}

fn target_parent(path: &Path) -> Result<&Path> {
    let parent = path
        .parent()
        .ok_or("target_path: expected BASE/v4/user_dict.csv")?;
    if path.file_name().is_none_or(|n| n != "user_dict.csv")
        || parent.file_name().is_none_or(|n| n != "v4")
    {
        return Err("target_path: expected BASE/v4/user_dict.csv".into());
    }
    Ok(parent)
}

fn read_regular(path: &Path) -> Result<Option<Vec<u8>>> {
    let original = match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => {
            Some(fs::read(path).map_err(|_| "target_read_failed")?)
        }
        Ok(_) => return Err("target_not_regular_file".into()),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(_) => return Err("target_read_failed".into()),
    };
    Ok(original)
}

fn read_target(path: &Path) -> Result<Target> {
    let parent = target_parent(path)?;
    let original = read_regular(path)?;
    let base = parent
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    match fs::symlink_metadata(base.join("chewing.dat")) {
        Ok(_) if original.is_none() => {
            return Err("legacy_unmigrated: missing v4/user_dict.csv".into());
        }
        Err(e) if e.kind() != ErrorKind::NotFound => return Err("legacy_check_failed".into()),
        _ => (),
    }
    let bytes = original.as_deref().unwrap_or_default();
    let records = target_records(bytes)?;
    // Validate before calling upstream: it can skip bad readings and log their content.
    let dict = UserDict::from_reader(Cursor::new(bytes), StringTable::new())
        .map_err(|_| "target_not_lossless: libchewing_read")?;
    if target_records(&serialize(&dict)?)? != records {
        return Err("target_not_lossless: roundtrip".into());
    }
    Ok(Target { original, records })
}

#[derive(Clone, Copy)]
enum TargetFormat {
    Chewing,
    Vchewing,
}

struct WritePlan {
    path: PathBuf,
    original: Option<Vec<u8>>,
    image: Vec<u8>,
    format: TargetFormat,
}

fn unchanged(plan: &WritePlan) -> Result<()> {
    let current = match plan.format {
        TargetFormat::Chewing => read_target(&plan.path)?.original,
        TargetFormat::Vchewing => read_regular(&plan.path)?,
    };
    if current != plan.original {
        return Err("target_changed: rerun dry-run".into());
    }
    Ok(())
}

#[cfg(unix)]
fn sync_dir(parent: &Path) -> Result<()> {
    fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|_| "directory_sync_failed".into())
}

#[cfg(not(unix))]
fn sync_dir(_parent: &Path) -> Result<()> {
    Ok(())
}

fn prepare(plan: &WritePlan) -> Result<tempfile::NamedTempFile> {
    unchanged(plan)?;
    let path = &plan.path;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|_| "target_directory_create_failed")?;
    if let Some(bytes) = &plan.original {
        let prefix = format!(
            "{}.backup-",
            path.file_name().ok_or("target_path")?.to_string_lossy()
        );
        let mut backup = tempfile::Builder::new()
            .prefix(&prefix)
            .tempfile_in(parent)
            .map_err(|_| "backup_create_failed")?;
        backup
            .write_all(bytes)
            .and_then(|_| backup.as_file().sync_all())
            .map_err(|_| "backup_write_failed")?;
        let (file, backup_path) = backup.keep().map_err(|_| "backup_keep_failed")?;
        drop(file);
        println!("backup={}", backup_path.display());
        sync_dir(parent)?;
    } else {
        println!("backup=none_new_target");
    }
    let mut temp =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "target_temp_create_failed")?;
    temp.write_all(&plan.image)
        .map_err(|_| "target_temp_write_failed")?;
    if plan.original.is_some() {
        let permissions = fs::metadata(path)
            .map_err(|_| "target_metadata_failed")?
            .permissions();
        temp.as_file()
            .set_permissions(permissions)
            .map_err(|_| "target_permissions_failed")?;
    }
    temp.as_file()
        .sync_all()
        .map_err(|_| "target_temp_sync_failed")?;
    Ok(temp)
}

fn apply(plans: Vec<WritePlan>) -> Result<()> {
    let mut prepared = Vec::new();
    // Complete every backup and staged output before replacing any destination.
    for plan in plans {
        let temp = prepare(&plan)?;
        prepared.push((plan, temp));
    }
    for (plan, _) in &prepared {
        unchanged(plan)?;
    }
    for (plan, temp) in prepared {
        if plan.original.is_some() {
            temp.persist(&plan.path)
                .map_err(|_| "target_replace_failed")?;
        } else {
            temp.persist_noclobber(&plan.path)
                .map_err(|_| "target_create_failed")?;
        }
        let parent = plan
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        sync_dir(parent)?;
        if fs::read(&plan.path).map_err(|_| "target_verify_read_failed")? != plan.image {
            return Err("target_verify_failed".into());
        }
    }
    println!("applied=true");
    Ok(())
}

fn run(opt: &Options) -> Result<()> {
    let target = read_target(&opt.target)?;
    let mut counts = Counts::default();
    let source = read_source(opt, &mut counts)?;
    let additions = UserDict::new(StringTable::new());
    for (pair, ban) in source {
        if target.records.contains_key(&pair) {
            counts.skip("target_exists");
            continue;
        }
        if ban {
            additions.ban(&pair.syllables, &pair.word);
            counts.bans += 1;
        } else {
            additions.insert(&pair.syllables, &pair.word);
            counts.words += 1;
        }
    }
    counts.print(opt);
    if !opt.apply || counts.words + counts.bans == 0 {
        return Ok(());
    }
    let mut image = target.original.clone().unwrap_or_default();
    if !image.is_empty() && !image.ends_with(b"\n") {
        image.push(b'\n');
    }
    image.extend_from_slice(&serialize(&additions)?);
    // Preserve every original byte; only the new records come from UserDict::to_writer.
    let expected = target.records.len() + counts.words + counts.bans;
    if target_records(&image)?.len() != expected {
        return Err("output_validation_failed".into());
    }
    apply(vec![WritePlan {
        path: opt.target.clone(),
        original: target.original,
        image,
        format: TargetFormat::Chewing,
    }])
}

struct VchewingTarget {
    original: Option<Vec<u8>>,
    pairs: BTreeSet<Pair>,
}

fn vchewing_pairs(bytes: &[u8]) -> Result<BTreeSet<Pair>> {
    let normalized = vchewing_lines(bytes);
    let mut pairs = BTreeSet::new();
    for (i, line) in normalized.lines().enumerate() {
        match source_line(line) {
            Ok(pair) => {
                pairs.insert(pair);
            }
            Err("comment" | "blank") => (),
            Err(_) => return Err(format!("target_not_lossless: line {}", i + 1)),
        }
    }
    Ok(pairs)
}

fn read_vchewing_target(path: &Path) -> Result<VchewingTarget> {
    let original = read_regular(path)?;
    let pairs = vchewing_pairs(original.as_deref().unwrap_or_default())?;
    Ok(VchewingTarget { original, pairs })
}

fn run_reverse(opt: &Options) -> Result<()> {
    if opt.target.as_os_str().is_empty() {
        return Err("missing_target_dir".into());
    }
    let source = read_target(&opt.source).map_err(|_| "source_not_lossless")?;
    if source.original.is_none() {
        return Err("source_missing".into());
    }
    let paths = [
        opt.target.join(format!("userdata-{}.txt", opt.variant)),
        opt.target
            .join(format!("exclude-phrases-{}.txt", opt.variant)),
    ];
    let targets = [
        read_vchewing_target(&paths[0])?,
        read_vchewing_target(&paths[1])?,
    ];
    let mut added = [String::new(), String::new()];
    let mut counts = Counts::default();
    for (pair, boost) in source.records {
        if targets.iter().any(|target| target.pairs.contains(&pair)) {
            counts.skip("target_exists");
            continue;
        }
        let ban = boost == UserDict::MIN;
        let reading = pair
            .syllables
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("-");
        let line = format!("{} {}\n", pair.word, reading);
        if !matches!(source_line(line.as_bytes()), Ok(parsed) if parsed == pair) {
            return Err("source_not_representable: vchewing_line".into());
        }
        added[usize::from(ban)].push_str(&line);
        if ban {
            counts.bans += 1;
        } else {
            counts.words += 1;
        }
    }
    counts.print(opt);
    if !opt.apply || counts.words + counts.bans == 0 {
        return Ok(());
    }
    let mut plans = Vec::new();
    for ((path, target), new_lines) in paths.into_iter().zip(targets).zip(added) {
        if new_lines.is_empty() && target.original.is_some() {
            continue;
        }
        // Produce both files on first export, including a header-only empty dictionary.
        let mut image = target
            .original
            .clone()
            .unwrap_or_else(|| VCHEWING_HEADER.as_bytes().to_vec());
        if image
            .last()
            .is_some_and(|b| !matches!(b, b'\n' | b'\r' | 0x0c))
        {
            image.push(b'\n');
        }
        image.extend_from_slice(new_lines.as_bytes());
        vchewing_pairs(&image)?;
        plans.push(WritePlan {
            path,
            original: target.original,
            image,
            format: TargetFormat::Vchewing,
        });
    }
    apply(plans)
}

fn entry(reverse: bool) -> ExitCode {
    let result = match options(reverse) {
        Ok(None) => {
            println!("{}", if reverse { REVERSE_HELP } else { HELP });
            return ExitCode::SUCCESS;
        }
        Ok(Some(opt)) => {
            if reverse {
                run_reverse(&opt)
            } else {
                run(&opt)
            }
        }
        Err(e) => Err(e),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error={e}");
            ExitCode::FAILURE
        }
    }
}

pub fn forward_main() -> ExitCode {
    entry(false)
}
pub fn reverse_main() -> ExitCode {
    entry(true)
}
