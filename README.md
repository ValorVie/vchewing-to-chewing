---
title: 唯音與新酷音使用者詞庫轉換
type: guide
date: 2026-10-18
author: Codex
status: ready-for-review
---

<!-- SPDX-License-Identifier: LGPL-2.1-or-later -->

# vchewing-to-chewing

手動將唯音使用者詞庫轉成 libchewing 0.14 的 `v4/user_dict.csv`。預設只預覽筆數；加 `--apply` 才寫入。只輸出筆數、原因與備份路徑，不輸出詞條內容。

反方向使用新增的 `chewing-to-vchewing`，見[新酷音轉回唯音](#新酷音轉回唯音)。原有 CLI 參數與 `cargo run -- ...` 用法保持相同，兩個 binary 共用解析、驗證與備份替換程式。

## 建置與預覽

需要 Rust 1.95 以上。Cargo 從 crates.io 取得固定版本 `chewing 0.14.0-alpha.4`，不需要相鄰的原始碼目錄、D512 模型、ORT 或輸入法安裝。第一次建置需連線下載相依；快取齊全後可加 `--offline`：

下方跨機移轉範例另需 Mac 上可用的 `python3` 與 PC 上的 PowerShell；所有操作都使用一般使用者權限。

```sh
# Mac；一般使用者權限，在本專案目錄執行
cargo build --release --locked
./target/release/vchewing-to-chewing --help
./target/release/chewing-to-vchewing --help
```

```sh
# Mac；一般使用者權限，僅預覽指定副本
./target/release/vchewing-to-chewing \
  --source-dir /path/to/vchewing-export \
  --target /path/to/chewing-copy/v4/user_dict.csv
```

| 參數 | 行為 |
| --- | --- |
| `--source-dir DIR` | 必填。只讀 `userdata-<variant>.txt` 與 `exclude-phrases-<variant>.txt`。一般詞檔必須存在；排除檔不存在視為空清單，其他讀取錯誤停止。 |
| `--target BASE/v4/user_dict.csv` | 必填。指定完整目標檔路徑，不猜目前輸入法目錄；檔名與末層目錄必須符合此形式。 |
| `--variant cht\|chs` | 預設 `cht`。每次只處理一份繁體或簡體來源，不讀另一份。 |
| `--apply` | 寫入前備份既有目標。沒有新增配對時不寫檔、不建立備份。 |

**跑兩次會讓兩份共用同一詞庫。** 工具不自動繁簡轉換，也不在一次執行中合併繁簡來源。目標格式沒有繁簡模式欄位。

預覽不建立目錄、目標檔、備份或 lock 檔。成功 exit 0，停止／錯誤 exit 1。範例輸出：

```text
mode=dry-run variant=cht add_words=2 add_bans=1 skipped=3
comment=1
duplicate_source=1
target_exists=1
```

`add_words`／`add_bans` 是本次規劃新增的配對數；`skipped` 是略過的來源行數。LF、CR、FF 均分行，CRLF 算一次換行；保留空白行計數，檔尾單一換行不另算空行。新增與略過筆數加總等於讀到的來源行數。來源一般詞被同配對排除詞取代時，原一般詞行計入 `source_exclusion_wins`。

## 移轉規則與停止條件

- 同一詞面不同讀音是不同配對。目標已有一般或排除記錄時保留目標，來源配對略過；不調整目標 boost、不取消目標排除。
- 來源同配對兼有一般與排除時，以排除為準。新增一般詞透過 `UserDict::insert`，boost 為 10；新增排除用 `UserDict::ban`，boost 為 -100。
- 來源格式是 `詞面 注音-注音 [權重] [#註解]`。不轉權重、不留註解；第三欄若不是註解，仍須是有限數值。接受檔首 UTF-8 BOM、LF／CRLF／CR／FF 換行，以及 Tab、NBSP 與全形空白等欄位空白。
- 讀音限國語注音 `ㄅ..ㄩ` 與音節尾端的 `ˊˇˋ˙`，一聲不加標記；每音節交由 libchewing parser 檢查字元次序，且序列化須相同。這是拼寫結構檢查，不推斷詞語的正確發音。拼音、數字聲調、符號 key 不自動猜轉。
- 字數以 Unicode 字素群計算，與 libchewing 一致；須等於音節數。上限依相依套件的 `SyllableVec::MAX_LEN`（目前 11）。含逗號、空白或控制字元的詞面無法安全表示，略過並計數。
- 發現 `BASE/chewing.dat` 但缺少 `BASE/v4/user_dict.csv` 時，報 `legacy_unmigrated` 並停止，即使已有空 `v4/` 也一樣。先由相容版本的新酷音完成遷移；本工具不遷移舊詞庫，也不自行刪除舊檔。
- 目標必須是 UTF-8、無表頭的三欄 `詞面,注音 空白分隔,boost`；boost 須是 -100 到 100 的整數。拒絕壞行、空行、註解、額外欄、重複配對、不合規讀音／字數與無法通過 libchewing 重讀核對的資料，報 `target_not_lossless` 並停止。這比上游寬鬆 parser 更保守，避免其忽略壞行或夾限 boost 後造成資料損失。
- 拒絕目標檔本身為 symlink 或非一般檔案。預覽與寫入前會重讀目標，發現內容變動就停止。這不能阻止其他程式在最後檢查後寫入；**實際套用、複製或回復前，必須先停止使用目標詞庫的程式**。

| 略過原因 | 說明 |
| --- | --- |
| `comment`／`blank` | 註解或空白行 |
| `bad_line`／`invalid_utf8` | 欄位不完整、非預期欄位或該行不是 UTF-8 |
| `invalid_weight` | 可選權重不是有限數值 |
| `symbol_key`／`invalid_reading` | 特殊 key 或不符合上述注音格式 |
| `too_long`／`length_mismatch` | 超出音節上限或字數不符 |
| `unrepresentable_word` | 詞面不能安全表示於目標格式 |
| `duplicate_source`／`source_exclusion_wins` | 來源重複或由排除配對取代 |
| `target_exists` | 目標已有該配對，保留目標 |

套用時保留原始目標的每個位元組，只在尾端追加新記錄。既有檔案先寫一份同目錄、隨機名稱的 `user_dict.csv.backup-*`，同步備份內容後才替換目標；新目標沒有舊檔可備份，會輸出 `backup=none_new_target`。新內容先寫同目錄暫存檔、同步，再用 tempfile 的持久化操作替換。備份不覆寫、不自動刪除；若備份失敗，目標保持原樣。

不要以 `chewing-cli dump` 的結果冒充這個 CSV；該命令的 Trie 輸出欄序是「詞面、freq、讀音」。自動學習的 `history_dict.bin`、唯音個人選字偏好、聯想詞、替換規則與符號選單均不在本次移轉內。

## 在 Mac 準備匯出

以下命令由使用者自行執行。先從唯音設定確認自己的詞庫資料夾；不要把資料放進這個 Git repo，也不要貼到終端紀錄、issue 或報告。只複製本次選用的兩個文字檔，不必複製整個應用程式容器。

```sh
# Mac；一般使用者權限，只複製所選來源至私人暫存目錄
VCH_SOURCE='/path/to/your-vchewing-userdata'
EXPORT_DIR=$(mktemp -d -t chewing-export)
mkdir -p "$EXPORT_DIR/source"
cp "$VCH_SOURCE/userdata-cht.txt" "$EXPORT_DIR/source/"
if [ -f "$VCH_SOURCE/exclude-phrases-cht.txt" ]; then
  cp "$VCH_SOURCE/exclude-phrases-cht.txt" "$EXPORT_DIR/source/"
fi
```

### PC 現有詞庫的唯讀取回

若 PC 已有詞庫，先停止使用新酷音的程式，再執行以下 PowerShell。`$Target` 範例對應 `%APPDATA%\Chewing\v4\user_dict.csv`；請先確認這是目前有效的詞庫路徑。自訂 `CHEWING_USER_PATH` 或舊安裝可能使用其他目錄，工具不會自動尋找。

```powershell
# PC；一般使用者權限，只讀來源，將副本與原始 SHA 寫入新的暫存目錄
$ErrorActionPreference = 'Stop'
$Target = Join-Path $env:APPDATA 'Chewing\v4\user_dict.csv'
$Bytes = [IO.File]::ReadAllBytes($Target)
$Hasher = [Security.Cryptography.SHA256]::Create()
try {
    $Hash = ([BitConverter]::ToString($Hasher.ComputeHash($Bytes))).Replace('-', '')
} finally {
    $Hasher.Dispose()
}
$ExportDir = Join-Path $env:TEMP ('chewing-export-' + [guid]::NewGuid().ToString())
$OriginalDir = Join-Path $ExportDir 'original'
New-Item -ItemType Directory -Path $OriginalDir | Out-Null
[IO.File]::WriteAllBytes((Join-Path $OriginalDir 'user_dict.csv'), $Bytes)
[IO.File]::WriteAllText((Join-Path $OriginalDir 'user_dict.csv.sha256'), $Hash + "`n", [Text.Encoding]::ASCII)
'export_dir=' + $ExportDir
```

PC 從同一份位元組快照產生原始副本與 SHA-256；內容只落檔，終端只印匯出目錄。用你慣用的方式複製檔案，將匯出目錄中的 `original/` 放到 Mac 的 `$EXPORT_DIR/original/`，再執行以下驗證。不要把詞庫內容貼到終端紀錄、issue 或報告。

```sh
# Mac；一般使用者權限，驗證取回的原件並建立獨立工作副本
python3 - "$EXPORT_DIR" <<'PYTHON'
import hashlib
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
data = (root / "original/user_dict.csv").read_bytes()
expected = (root / "original/user_dict.csv.sha256").read_text(encoding="ascii").strip()
if not re.fullmatch(r"[A-Fa-f0-9]{64}", expected):
    raise SystemExit("原始 SHA-256 格式錯誤，停止。")
if hashlib.sha256(data).hexdigest().upper() != expected.upper():
    raise SystemExit("原始 SHA-256 不符，停止。")
working = root / "target/v4/user_dict.csv"
working.parent.mkdir(parents=True, exist_ok=True)
with working.open("xb") as output:
    output.write(data)
print("原始 SHA-256 已核對，工作副本已建立。")
PYTHON
```

`original/user_dict.csv` 與 `original/user_dict.csv.sha256` 保持不動，CLI 只合併 `target/v4/user_dict.csv`。後續 PC `$OriginalSha` 要填原始 SHA 檔的值，不能填合併後工作副本的 SHA。複製、驗證或獨占建立失敗就停止；工作副本已存在時另建私人匯出目錄，不覆寫舊副本。

如果 PC 還沒有 v4 詞庫，先確認有效根目錄沒有未遷移的 `chewing.dat`，再使用空的 Mac 目標；不要只因暫存目錄是空的就略過確認。PC 根目錄可能由 `CHEWING_USER_PATH` 指定，舊安裝也可能使用 `%USERPROFILE%\ChewingTextService`。請以實際輸入法版本的設定為準。

```sh
# Mac；一般使用者權限，先預覽再合併私人工作副本
./target/release/vchewing-to-chewing \
  --source-dir "$EXPORT_DIR/source" \
  --target "$EXPORT_DIR/target/v4/user_dict.csv" \
  --variant cht

./target/release/vchewing-to-chewing \
  --source-dir "$EXPORT_DIR/source" \
  --target "$EXPORT_DIR/target/v4/user_dict.csv" \
  --variant cht --apply

shasum -a 256 "$EXPORT_DIR/target/v4/user_dict.csv"
```

要移轉簡體時，只複製 `*-chs.txt`，並在兩次命令都加 `--variant chs`。先核對預覽與略過原因，再執行 apply；不要直接用空目標的匯出結果覆寫 PC 既有詞庫。

## 把結果送到 PC 並套用

用你慣用的方式複製檔案，將 Mac 的 `target/v4/user_dict.csv` 放到 PC 的下載資料夾，命名為 `imported-user_dict.csv`。以下 PowerShell 由使用者在 PC 執行；先停止使用新酷音的程式，再填入有效的 `$Target`、原始 SHA 與 Mac 合併結果的 SHA。**套用後，在任何程式用新酷音打字之前，先登出再登入或重新開機。** 原本沒有目標時才將 `$OriginalSha` 留空。

```powershell
# PC；一般使用者權限，先停止使用目標詞庫的程式，核對 SHA 後備份套用
$ErrorActionPreference = 'Stop'
$Target = Join-Path $env:APPDATA 'Chewing\v4\user_dict.csv'
$Incoming = Join-Path $env:USERPROFILE 'Downloads\imported-user_dict.csv'
$OriginalSha = 'FILL_IN_ORIGINAL_SHA256' # 原目標不存在才改成空字串
$ExpectedIncomingSha = 'FILL_IN_SHA256_FROM_MAC'
if ($ExpectedIncomingSha -notmatch '^[A-Fa-f0-9]{64}$' -or
    ($OriginalSha -and $OriginalSha -notmatch '^[A-Fa-f0-9]{64}$')) {
    throw 'Fill in the required SHA-256 values'
}
$Parent = Split-Path -Parent $Target
$Base = Split-Path -Parent $Parent
if ((Get-FileHash -LiteralPath $Incoming -Algorithm SHA256).Hash -ne $ExpectedIncomingSha) {
    throw 'Incoming SHA-256 mismatch'
}
if ($OriginalSha) {
    if (!(Test-Path -LiteralPath $Target -PathType Leaf) -or
        (Get-FileHash -LiteralPath $Target -Algorithm SHA256).Hash -ne $OriginalSha) {
        throw 'Target changed; export again'
    }
} elseif ((Test-Path -LiteralPath $Target) -or
          (Test-Path -LiteralPath (Join-Path $Base 'chewing.dat'))) {
    throw 'Existing target or unmigrated legacy; stop'
}
New-Item -ItemType Directory -Path $Parent -Force | Out-Null
$Temp = Join-Path $Parent ('import-' + [guid]::NewGuid().ToString() + '.tmp')
Copy-Item -LiteralPath $Incoming -Destination $Temp
if ($OriginalSha) {
    $Backup = Join-Path $Parent ('user_dict.csv.backup-' + [guid]::NewGuid().ToString())
    [IO.File]::Replace($Temp, $Target, $Backup)
    'backup=' + $Backup
} else {
    [IO.File]::Move($Temp, $Target) # 目標突然出現時失敗，不覆寫
    'backup=none_new_target'
}
if ((Get-FileHash -LiteralPath $Target -Algorithm SHA256).Hash -ne $ExpectedIncomingSha) {
    throw 'Final SHA-256 mismatch; keep backup and investigate'
}
```

PC 現況與原始 SHA 不符就停止，重新取副本並預覽，不強制覆寫。成功條件是最終 SHA 與 Mac 結果一致；**完成後先登出再登入或重新開機，再用新酷音打字並確認候選詞與排除生效。** 套用前已啟用新酷音的程式不會因磁碟檔案更新而重讀詞庫，之後打字觸發學習時可能把舊資料整份寫回，覆蓋匯入結果；關閉程式、登出或重開機本身不會觸發寫回。不需安裝、註冊或系統管理員操作。Windows 的匯出、套用、回復範例與 ACL 尚未在實機驗證，不能將條件編譯當成輸入法驗收。

## 回復

先停止使用目標詞庫的程式。若目標原本存在，保留匯入後檔案供比對，從本次輸出的**確切備份路徑**回復，不用萬用字元挑「最新」檔案。**回復後也要先登出再登入或重新開機，之後才在任何程式用新酷音打字。**

```powershell
# PC；一般使用者權限，先停止使用目標詞庫的程式，只回復指定備份
$ErrorActionPreference = 'Stop'
$Target = Join-Path $env:APPDATA 'Chewing\v4\user_dict.csv'
$Backup = Join-Path $env:APPDATA 'Chewing\v4\user_dict.csv.backup-EXACT_NAME'
Copy-Item -LiteralPath $Backup -Destination $Target -Force
if ((Get-FileHash -LiteralPath $Target -Algorithm SHA256).Hash -ne
    (Get-FileHash -LiteralPath $Backup -Algorithm SHA256).Hash) { throw 'Restore mismatch' }
```

目標原本不存在時，先保留匯入檔副本，確認沒有後續新增／學習資料後，移除這次新建的 `user_dict.csv`；不要刪掉整個 `v4/` 或 `history_dict.bin`。Mac 上同樣可用 `cp` 回復確切備份，並用 `cmp -s` 核對。任何資料漂移、備份不存在或驗證失敗都停止，不繼續覆寫。

Mac 暫存副本與 PC 實際目標是兩份不同檔案；Mac 備份不能代替 PC 套用前的備份。備份及匯出檔都含個人詞庫，保存與移除由使用者決定。

## 新酷音轉回唯音

`chewing-to-vchewing` 讀取明確指定的 `BASE/v4/user_dict.csv`，輸出唯音 `userdata-<variant>.txt` 與 `exclude-phrases-<variant>.txt`；不讀取 `history_dict.bin`。預設 `--variant cht`，只預覽；`--apply` 才寫入。`chs` 只改目標檔名，不轉換字形，也不把兩份唯音詞庫合併。

### 取回來源並預覽

先建立新的私人工作目錄，再執行上方 [PC 現有詞庫的唯讀取回](#pc-現有詞庫的唯讀取回) 的 PC 匯出、檔案複製與 Mac 驗證步驟。這次不需複製唯音資料作為來源。該區塊會保留 `original/user_dict.csv` 與原始 SHA，並建立內容相同的 `target/v4/user_dict.csv`。反向 CLI 讀取後者，不修改這兩份來源副本。

```sh
# Mac；一般使用者權限，建立供 PC 唯讀取回命令使用的私人目錄
export EXPORT_DIR
EXPORT_DIR=$(mktemp -d -t chewing-reverse)
```

確認唯音設定中實際使用的詞庫資料夾，明確填入 `VCH_TARGET`；工具不自動找容器或個人資料。

```sh
# Mac；一般使用者權限，僅預覽反向轉換，不寫唯音詞庫
VCH_TARGET='/path/to/your-vchewing-userdata'
./target/release/chewing-to-vchewing \
  --source "$EXPORT_DIR/target/v4/user_dict.csv" \
  --target-dir "$VCH_TARGET" --variant cht
```

轉換政策：

- **只有 boost -100 是排除詞**，寫入排除檔。libchewing `UserDict::lookup_banned` 只接受 `MIN = -100`；`lookup` 仍回傳其他負值。因此 -99 到 100 都轉成一般詞，不搬排序加成或抑制程度。來源的其他權重不寫入唯音檔案。
- 注音以同一個 `Syllable` 解析與顯示，再由空白改成 `-`；一聲不加標記，輕聲維持音節尾端 `˙`。輸出每個配對都再經共用唯音 parser 核對，避免以 `#` 開頭的候選被當成註解；不能忠實表示時報 `source_not_representable` 並停止。
- 來源採正向工具同樣的嚴格 CSV 無損檢查。來源壞行、不合格欄位、重複配對或無法重讀時停止，不略過部分來源後繼續。
- 唯音兩個目標檔先完整讀取驗證；註解與空行保留，標準注音配對與有限的既有權重可讀。壞行、非 UTF-8 或不支援的 key（含拼音／符號 key）會停止，不猜測其語意。目標缺少的檔案視為空，但 symlink／非一般檔案會拒絕。
- 唯音已有的配對，不論在哪一檔，都保留並計入 `target_exists`；不把既有一般詞改為排除，也不取消已有的排除。原本的每個位元組、權重、註解及行序不改，只在檔尾追加新配對。
- 有新增時會建立原本缺少的兩個目標檔，空的那份僅有格式標頭，確保只有排除詞的資料也能正向讀回。沒有任何新增時不建目錄、不寫檔、不備份；重跑也是 no-op。

### 套用與重新讀取

**套用前先切換到其他輸入法或結束唯音**，避免唯音同時寫入；保持停用直到兩檔處理完成。確認預覽後再執行：

```sh
# Mac；一般使用者權限，先切換到其他輸入法或結束唯音
./target/release/chewing-to-vchewing \
  --source "$EXPORT_DIR/target/v4/user_dict.csv" \
  --target-dir "$VCH_TARGET" --variant cht --apply
```

既有檔案先備份為同目錄唯一的 `userdata-cht.txt.backup-*`／`exclude-phrases-cht.txt.backup-*`（chs 同理）。兩份備份與暫存內容都準備完成、目標仍與讀入時相同後，才逐檔替換。沒有舊檔時會明報 `backup=none_new_target`；有舊檔但無新增的那份不寫入、不備份。

每個檔案各自以同目錄暫存檔替換，**兩檔不是共同原子交易**。若第一檔已替換、第二檔失敗，會報錯並保留備份，不自動覆寫回復。先保留現況、用確切備份回復；若選擇重跑，已存在的配對仍會略過。工具沒有跨程序鎖，最後一次檢查後的競爭仍須靠停止唯音避免。

成功後切回／重新啟動唯音。唯音啟用自動重讀使用者檔案時，會依既有詞庫目錄監看流程重新讀取變更；若已關閉自動重讀，請重新啟動唯音以載入。本次只做合成檔案驗證，沒有替真人啟用、切換或重啟輸入法，也沒有驗證 GUI 候選效果。

### 反向轉換的回復

保持唯音停用，採用本次輸出的確切備份路徑。以下範例假設兩個檔案原本都存在；只改一檔時只回復該檔，不用萬用字元猜最新備份。

```sh
# Mac；一般使用者權限，先保持唯音停用，只回復指定的兩份備份
(
  set -eu
  WORD_BACKUP='/path/to/userdata-cht.txt.backup-EXACT_NAME'
  BAN_BACKUP='/path/to/exclude-phrases-cht.txt.backup-EXACT_NAME'
  cp "$WORD_BACKUP" "$VCH_TARGET/userdata-cht.txt"
  cp "$BAN_BACKUP" "$VCH_TARGET/exclude-phrases-cht.txt"
  cmp -s "$WORD_BACKUP" "$VCH_TARGET/userdata-cht.txt"
  cmp -s "$BAN_BACKUP" "$VCH_TARGET/exclude-phrases-cht.txt"
  printf '%s\n' '兩檔已與指定備份逐位元組核對。'
)
```

檔案原本不存在時沒有備份；先保存匯入產物，確認沒有後續新資料後，才移除本次新建的單一檔案。不要刪除整個唯音資料目錄。任何失敗、備份缺少或資料漂移都停止，由使用者判斷後再操作。

## 驗證與依據

```sh
# Mac；一般使用者權限，在本專案目錄執行；Windows 項目只做條件編譯
cargo test --offline --locked
cargo fmt --check
cargo clippy --offline --locked --all-targets --no-deps -- -D warnings
cargo check --offline --locked --target x86_64-pc-windows-msvc
```

測試只用合成資料，涵蓋繁簡分次、多音字、排除衝突、壞行計數、無損讀取、dry-run、備份失敗與重複執行；反向另測兩種方向的往返皆 0 新增、負權重語意、既有兩檔保留與無法表示的候選停止。Windows target 的 `cargo check` 只驗證條件編譯；未驗證 Windows 執行檔連結、實機檔案操作、輸入法重啟或回復。

格式依 [chewing 0.14.0-alpha.4 原始碼](https://docs.rs/crate/chewing/0.14.0-alpha.4/source/) 的 `src/user/dict.rs`、`src/path.rs`、`src/zhuyin/syllable.rs`，以及唯音 `LXCoreEX`／`UserPhraseInsertable` 實作。[Unicode 第 18.3 節](https://www.unicode.org/versions/Unicode17.0.0/core-spec/chapter-18/) 用來核對聲調字元；[tempfile 的 persist 文件](https://docs.rs/tempfile/3.27.0/tempfile/struct.NamedTempFile.html#method.persist) 說明同檔案系統替換與同步限制。Unix 會額外同步目錄；Windows 的目錄耐斷電性沒有在本次驗證。詞庫權重不互轉，因模型分數與手動加成沒有已驗證的跨引擎換算；語言模型 log 機率的背景可見 [Jurafsky 與 Martin 第 3 章 §3.1.3](https://web.stanford.edu/~jurafsky/slp3/3.pdf)。

授權為 LGPL-2.1-or-later，見 [LICENSE](LICENSE)。
