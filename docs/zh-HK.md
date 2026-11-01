<h1 align="center">xmtr</h1>

<p align="center">
  同時追蹤多個目標，在地圖上查看每一跳。
</p>

<p align="center">
  <a href="https://github.com/Alice39s/xmtr/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/Alice39s/xmtr?style=flat-square"></a>
  <a href="https://github.com/Alice39s/xmtr/actions/workflows/release.yml"><img alt="Build" src="https://img.shields.io/github/actions/workflow/status/Alice39s/xmtr/release.yml?branch=main&style=flat-square"></a>
  <img alt="License" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue?style=flat-square">
</p>

<p align="center">
  <a href="../README.md">English</a> | <a href="zh-CN.md">簡體中文</a> | <a href="zh-TW.md">繁體中文（台灣）</a> | <a href="zh-HK.md">繁體中文（香港）</a> | <a href="zh-MO.md">繁體中文（澳門）</a>
</p>

`xmtr` 用 [trippy](https://github.com/fujiapple852/trippy) 同時追蹤多個目標，每條路由的路徑穩定後就停止，然後把路由上傳到 [xmsl.dev IPMap](https://xmsl.dev/ipmap)。IPMap 把每一跳標在地圖上，並回放整條路由。

https://github.com/user-attachments/assets/b2152647-3396-4f2c-86d4-267c2b11e529

運行結束時給出的鏈接會打開這張地圖：

https://github.com/user-attachments/assets/b675a8f5-dff1-460a-bb3c-ca7b3e4ff412

## 快速上手

```sh
curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | bash
sudo xmtr 1.1.1.1 cloudflare.com
```

第二條命令追蹤 1.1.1.1 和 cloudflare.com，主機同時有 IPv4 和 IPv6 時兩種都追蹤，最後輸出路由表和 IPMap 鏈接。Windows 與各包管理器的安裝方式見[安裝](#安裝)，各系統要以什麼身份運行見[運行身份](#運行身份)。

## 安裝

### 安裝腳本

**macOS、Linux、FreeBSD、Termux**

```sh
curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | bash
```

腳本只用 POSIX `sh` 語法，沒有 bash 的系統（例如 OpenWrt）可以改用 `| sh`。普通用户運行時，腳本通過 `sudo` 安裝到 `/usr/local/bin`，這樣 `sudo xmtr` 能找到它；sudo 可能會要求輸入密碼。不能使用 sudo 時安裝到 `~/.local/bin`。

**Windows**

```powershell
powershell -c "irm https://raw.githubusercontent.com/Alice39s/xmtr/main/install.ps1|iex"
```

兩個腳本都會挑出適合本機的發佈文件，用發佈附帶的 `SHA256SUMS` 校驗，再運行一個小安裝器。安裝器放好唯一的 `xmtr` 可執行文件後刪除自身。除了 Windows 上當前用户的 `PATH` 條目，不會寫入其他內容。下載走 HTTPS，只有 `XMTR_RELEASES` 指向 `http://` 鏡像時例外。

| 系統 | 默認位置 |
| --- | --- |
| Linux、macOS、FreeBSD | `/usr/local/bin`；不能使用 sudo 時為 `~/.local/bin` |
| Termux | `$PREFIX/bin` |
| Windows | `%LOCALAPPDATA%\Programs\xmtr` |

安裝器的選項寫在 `bash -s --` 後面：

```sh
curl -fsSL https://raw.githubusercontent.com/Alice39s/xmtr/main/install.sh | bash -s -- --dir ~/bin --version v0.3.4
```

`XMTR_INSTALL_DIR` 設置安裝目錄，`XMTR_RELEASES` 讓腳本和安裝器都從 releases 頁面的鏡像下載。在 Windows 上只能用這兩個環境變量傳選項。

### 包管理器

| | 包管理器 | 命令 |
| :-: | --- | --- |
| <img src="../.github/assets/homebrew.svg" width="20" alt=""> | Homebrew（macOS、Linux） | `brew install alice39s/tap/xmtr` |
| <img src="../.github/assets/scoop.png" width="20" alt=""> | Scoop（Windows） | 先 `scoop bucket add alice39s https://github.com/Alice39s/scoop-bucket`，再 `scoop install alice39s/xmtr` |
| <img src="../.github/assets/windows.svg" width="20" alt=""> | WinGet（Windows） | `winget install Alice39s.xmtr` |
| <img src="../.github/assets/chocolatey.svg" width="20" alt=""> | Chocolatey（Windows） | `choco install xmtr` |
| <img src="../.github/assets/npm.svg" width="20" alt=""> | npm | `npm install --global @xmtr/cli`，或 `npx @xmtr/cli 1.1.1.1` |
| <img src="../.github/assets/pypi.svg" width="20" alt=""> | PyPI | `uv tool install xmtr`、`pipx install xmtr` 或 `pip install xmtr` |
| <picture><source media="(prefers-color-scheme: dark)" srcset="../.github/assets/rust-dark.svg"><img src="../.github/assets/rust.svg" width="20" alt=""></picture> | Cargo | `cargo binstall xmtr` 下載發佈好的可執行文件，`cargo install xmtr` 從源碼編譯 |
| <img src="../.github/assets/mise.svg" width="20" alt=""> | mise | `mise use --global github:Alice39s/xmtr` |
| <img src="../.github/assets/nixos.svg" width="20" alt=""> | Nix | `nix run github:Alice39s/xmtr -- 1.1.1.1`，或 `nix profile install github:Alice39s/xmtr` |
| <img src="../.github/assets/docker.svg" width="20" alt=""> | Docker | `docker run --rm ghcr.io/alice39s/xmtr 1.1.1.1` |

npm 包和 PyPI wheel 裏的可執行文件與 release 壓縮包裏的相同；npm 會按本機系統選用對應的 `@xmtr/<platform>` 包。Docker 鏡像裏只有靜態鏈接的可執行文件，提供 amd64、arm64、armv7、armv6、386、riscv64 和 loong64 版本。Docker 默認允許容器使用原始套接字；Podman 需要加 `--cap-add NET_RAW`。

### Linux 軟件源

deb、rpm 和 apk 包把程序安裝為 `/usr/bin/xmtr`，並用 `setcap` 授予原始套接字能力，所有用户不用 root 就能追蹤。這些包附在每個 release 上，也由帶簽名的軟件源提供：

| | 系統 | 架構 |
| :-: | --- | --- |
| <img src="../.github/assets/debian.svg" width="20" alt=""> | Debian、Ubuntu | amd64、arm64、armhf、i386、riscv64、loong64 |
| <img src="../.github/assets/fedora.svg" width="20" alt=""> | Fedora、RHEL | x86_64、aarch64、armv7hl、i686、riscv64、loongarch64 |
| <img src="../.github/assets/alpinelinux.svg" width="20" alt=""> | Alpine | x86_64、aarch64、armv7、armhf、x86、riscv64、loongarch64 |

```sh
# Debian、Ubuntu
sudo curl -fsSL -o /usr/share/keyrings/xmtr.gpg https://alice39s.github.io/xmtr/xmtr.gpg
echo "deb [signed-by=/usr/share/keyrings/xmtr.gpg] https://alice39s.github.io/xmtr/deb stable main" | sudo tee /etc/apt/sources.list.d/xmtr.list
sudo apt update && sudo apt install xmtr

# Fedora、RHEL
sudo curl -fsSL -o /etc/yum.repos.d/xmtr.repo https://alice39s.github.io/xmtr/rpm/xmtr.repo
sudo dnf install xmtr

# Alpine
wget -qO /etc/apk/keys/xmtr.rsa.pub https://alice39s.github.io/xmtr/apk/xmtr.rsa.pub
echo https://alice39s.github.io/xmtr/apk >> /etc/apk/repositories
apk add xmtr
```

軟件源的簽名密鑰在 [`packaging/keys`](../packaging/keys)。上面所有渠道都由發佈流水線推送，mise 和 Nix 除外，它們直接讀取 GitHub release 和 flake。

### 更新與卸載

```sh
xmtr self update              # 用最新版本覆蓋當前程序
xmtr self update --tag v0.3.3 # 安裝指定版本
xmtr self uninstall           # 刪除當前程序
```

`--force` 會重新安裝當前已是最新的版本。通過包管理器或 Linux 軟件源安裝的，請用對應的包管理器更新。

在終端裏運行結束後，xmtr 會查詢最新版本，最多等 3 秒。有新版本時，它先問要不要安裝，再問要不要用新版本重新運行同一條命令；兩次都直接按 Enter 表示同意。如果程序由包管理器管理，或者所在目錄不可寫，xmtr 改為輸出對應的更新命令。帶 `--json`、`--quiet` 或 `--no-upload` 的運行，以及 stdin 和 stderr 不是終端的運行，都不會查詢。

### 校驗下載的文件

手動安裝用的壓縮包在 [releases 頁面](https://github.com/Alice39s/xmtr/releases)。release 不可修改：發佈之後，其中的文件和 tag 都不能再變。

```sh
gh release verify-asset <tag> <file> --repo Alice39s/xmtr   # 文件屬於該 release
gh attestation verify <file> --repo Alice39s/xmtr           # 文件由發佈流水線構建
```

第二條命令檢查的是[構建來源證明](https://github.com/Alice39s/xmtr/attestations)。

## 支持的平台

| | 系統 | 架構 |
| :-: | --- | --- |
| <img src="../.github/assets/linux.svg" width="20" alt=""> | Linux，glibc 與 musl | <img src="../.github/assets/amd.svg" width="14" alt=""> x86_64 · <img src="../.github/assets/intel.svg" width="14" alt=""> i686 · <img src="../.github/assets/arm.svg" width="14" alt=""> aarch64、armv7、armv6 · <img src="../.github/assets/riscv.svg" width="14" alt=""> riscv64 · <img src="../.github/assets/chip.svg" width="14" alt=""> loongarch64 |
| <img src="../.github/assets/linux.svg" width="20" alt=""> | Linux，glibc | <img src="../.github/assets/ibm.svg" width="24" alt=""> powerpc64le、powerpc64、s390x · <img src="../.github/assets/oracle.svg" width="14" alt=""> sparc64 |
| <picture><source media="(prefers-color-scheme: dark)" srcset="../.github/assets/apple-dark.svg"><img src="../.github/assets/apple.svg" width="20" alt=""></picture> | macOS | <img src="../.github/assets/arm.svg" width="14" alt=""> Apple silicon · <img src="../.github/assets/intel.svg" width="14" alt=""> x86_64 |
| <img src="../.github/assets/windows.svg" width="20" alt=""> | Windows 10 及以上 | <img src="../.github/assets/amd.svg" width="14" alt=""> x86_64 · <img src="../.github/assets/intel.svg" width="14" alt=""> i686 · <img src="../.github/assets/arm.svg" width="14" alt=""> ARM64 |
| <img src="../.github/assets/freebsd.svg" width="20" alt=""> | FreeBSD | <img src="../.github/assets/amd.svg" width="14" alt=""> x86_64 |
| <img src="../.github/assets/android.svg" width="20" alt=""> | Android，需要 root 的 Termux | <img src="../.github/assets/arm.svg" width="14" alt=""> aarch64、armv7 · <img src="../.github/assets/amd.svg" width="14" alt=""> x86_64 · <img src="../.github/assets/intel.svg" width="14" alt=""> i686 |

所有版本都在 Linux 上交叉編譯。每次發佈前，CI 會運行每一個版本並用它追蹤 `127.0.0.1`：Linux、macOS 和 Windows 的 x86_64 與 ARM64 版本跑在託管 runner 上，其餘 Linux CPU 架構跑在 QEMU 用户模式模擬裏，FreeBSD 跑在虛擬機裏，Android 跑在 Termux 鏡像裏。發佈之後，CI 在 Linux、macOS 和 Windows runner 上通過每個包管理渠道安裝一遍 xmtr。

| | 系統 | 沒有版本的原因 |
| :-: | --- | --- |
| <img src="../.github/assets/netbsd.svg" width="20" alt=""> | NetBSD | trippy 的套接字層在該系統上無法編譯 |
| <img src="../.github/assets/openbsd.svg" width="20" alt=""> | OpenBSD、DragonFly BSD | Rust 沒有為交叉編譯提供預編譯的標準庫 |
| <img src="../.github/assets/chip.svg" width="20" alt=""> | illumos、Solaris | trippy 的套接字層在該系統上無法編譯 |
| <img src="../.github/assets/webassembly.svg" width="20" alt=""> | iOS、WebAssembly | 命令行程序無法使用原始套接字 |

## 運行身份

追蹤需要原始套接字。

| 系統 | 以什麼身份運行 |
| --- | --- |
| Linux | root；或者執行一次 `sudo setcap cap_net_raw+ep $(command -v xmtr)`，deb、rpm 和 apk 包安裝時已經做過這一步 |
| macOS | 任意用户；ICMP 和 UDP 使用非特權套接字，TCP 需要 root |
| Windows | 管理員終端 |
| FreeBSD、Android | root |

`xmtr self update` 會替換可執行文件，所以更新後要重新執行一次 `setcap`。

## 用法

```sh
xmtr 1.1.1.1 cloudflare.com 9.9.9.9   # 同時追蹤三個目標
xmtr -4 --min-rounds 20 example.com    # 只追蹤 IPv4，至少跑 20 輪
xmtr -p tcp --port 443 example.com     # 用 TCP 探測 443 端口
xmtr --no-upload 192.0.2.1             # 只輸出路由，不上傳
xmtr --pin 2580 example.com            # 打開地圖前需要輸入 2580
xmtr -q 1.1.1.1                        # 只輸出 IPMap 鏈接
xmtr --json 1.1.1.1 > trace.json       # 以 JSON 輸出上傳內容，表格寫到 stderr
```

全部選項見[選項](#選項)。

### 一次運行的過程

一次運行分為五步，進度面板裏的編號與下面相同。

1. **Resolve（解析）**：解析每個目標。同時有 A 和 AAAA 記錄的主機名會同時走 IPv4 和 IPv6 追蹤，每種地址族按解析器返回的順序取第一個地址。本機沒有路由的地址族會被跳過。
2. **Trace（追蹤）**：最多 8 條路由並行（`-j`）。每個追蹤器有各自的 ICMP ID、UDP 源端口和序列號區間，不同路由的回包不會混在一起。一條路由跑滿 `--min-rounds` 輪、連續 3 輪沒有出現新的跳、並且目標有回應後就停止。始終沒有到達目標的路由會多跑 5 輪。`--max-rounds` 和 `--time-limit` 是硬上限，追蹤器跑的輪數不會超過時間限制內能跑完的輪數。

   使用 `--protocol auto` 時，如果 ICMP 跑了 4 輪還沒到達目標，xmtr 會再用 TCP 80 端口追蹤一次；使用非特權套接字時改用 UDP。第二條路由與 ICMP 路由同時進行，在進度面板裏單獨佔一行，也計入 `-j`；如果 ICMP 路由提前結束且沒有到達目標，第二條路由在它結束時開始。兩條路由都會上傳。沒有任何一跳回應的路由會作為錯誤報告，不上傳。
3. **Lookup（查詢）**：為公網地址的跳查詢反向解析名稱和所屬 AS 號（[Team Cymru](https://www.team-cymru.com/ip-asn-mapping)），同時最多查詢 32 個地址。這一步最多持續 5 秒，結果裏會註明有多少個地址沒有回應。私有地址永遠不會發給 `--dns` 指定的解析器。`--no-rdns` 跳過這一步。
4. **Upload（上傳）**：把路由上傳到 `--api`，詳見[上傳與分享](#上傳與分享)。`--no-upload` 跳過這一步。
5. **Result（結果）**：輸出 IPMap 鏈接、備註，以及每條路由的表格。

按 Ctrl-C 會終止運行，退出碼為 130。

### 進度與輸出

在終端裏，提示符下方的面板會實時顯示各個步驟：當前進行到哪一步、每條路由一根進度條，以及已有多少個反向解析得到回應。運行結束後，面板變成結果：先是 IPMap 鏈接，然後是備註，接着每條路由一張表格，連續不回應的跳摺疊成一行。面板畫在 stderr 上，同時要求 stdout 也連着終端。

`--progress` 選擇進度視圖，`auto` 根據終端自動選擇：

| 視圖 | `auto` 何時選它 | 顯示內容 |
| --- | --- | --- |
| `panel` | stdout 和 stderr 都連着終端，終端至少 44 列、8 行，並且能回報光標位置 | 上面描述的面板 |
| `line` | stdout 被重定向，或者終端更小、不能回報光標位置 | 已完成的步驟和路由各佔一行，下方一行狀態，最後輸出表格和鏈接 |
| `plain` | stderr 不是終端，或者 `TERM=dumb` | 每個步驟和每條完成的路由各一行，例如 `[2/5] 1.1.1.1 v4 ICMP: reached, 7 rounds, 5.2s`；沒有變化時每 10 秒輸出一次當前狀態 |

如果路由器在 MPLS 隧道里轉發了探測包，並在回包裏帶上標籤棧（[RFC 4950](https://www.rfc-editor.org/rfc/rfc4950)），它的地址下方會每個標籤佔一行，例如 `[MPLS: Lbl 24001 TC 0 S 1 TTL 1]`，與 `mtr -e` 的格式相同。隧道里每個路由器報告的往返時間都接近隧道最後一個路由器的往返時間，因為它的回包要先走到隧道終點再折返。

表格放不下終端寬度時，即使把長主機名換行也放不下，就改成每跳一行，只列主機、平均往返時間和丟包。`--quiet` 關閉進度顯示，只輸出鏈接。

`--theme` 設置面板和行視圖的配色。`auto` 會向終端查詢背景色，`COLORTERM` 為 `truecolor` 或 `24bit` 時使用 RGB 顏色；`dark`、`light`、`dark-ansi` 和 `light-ansi` 直接指定。

通過管道或重定向輸出時，xmtr 以純文本輸出每條路由，最後是 trace ID 和鏈接：

```text
$ sudo xmtr --min-rounds 20 121.55.201.201 | cat
121.55.201.201 -> 121.55.201.201 [ICMP IPv4] 20 rounds, 20.4s, reached
┌─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┐
│ Hop   Host                                                               ASN        Loss%   Snt    Last     Avg    Best    Wrst   StDev │
╞═════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════════╡
│   1   dgw.vyme.net (45.88.180.254)                                       AS199297     0.0    20     0.4     0.4     0.3     0.9     0.1 │
│   2   te0-0-0-2.nr61.b074095-0.rns01.atlas.cogentco.com (149.6.72.106)   AS174        0.0    20     1.3     1.1     0.8     1.6     0.2 │
│   3   be2928.rcr51.rns01.atlas.cogentco.com (154.25.12.25)               AS174        0.0    20     1.5     1.6     1.5     2.0     0.1 │
│   4   be7070.rcr61.uro01.atlas.cogentco.com (154.54.76.85)               AS174        0.0    20     6.0     5.7     5.5     6.1     0.2 │
│   5   be2480.ccr42.par01.atlas.cogentco.com (130.117.3.13)               AS174        0.0    20     7.3    10.9     7.0    44.4    11.2 │
│   6   port-channel2732.ccr92.lhr01.atlas.cogentco.com (154.54.75.138)    AS174        0.0    20    14.1    14.1    14.0    14.5     0.1 │
│   7   be3467.ccr32.bos01.atlas.cogentco.com (154.54.94.45)               AS174        0.0    20    76.4    76.3    76.2    76.5     0.1 │
│   8   be8030.ccr22.alb02.atlas.cogentco.com (154.54.169.226)             AS174        0.0    20    79.9    79.9    79.7    80.6     0.2 │
│   9   port-channel8023.ccr92.cle04.atlas.cogentco.com (154.54.169.197)   AS174        0.0    20    87.4    87.5    87.3    87.6     0.1 │
│  10   be2718.ccr42.ord01.atlas.cogentco.com (154.54.7.129)               AS174        0.0    20    99.3    99.0    98.8    99.3     0.1 │
│  11   be5068.ccr32.oma02.atlas.cogentco.com (154.54.166.73)              AS174        0.0    20   102.7   103.6   102.7   118.3     3.5 │
│  12   be8568.ccr82.den01.atlas.cogentco.com (154.54.95.109)              AS174       70.0    20   114.7   114.2   113.8   114.7     0.3 │
│  13   be9563.ccr82.slc03.atlas.cogentco.com (154.54.163.206)             AS174       55.0    20   125.1   124.5   123.5   125.1     0.7 │
│  14   be3906.ccr22.sfo01.atlas.cogentco.com (154.54.5.154)               AS174        0.0    20   138.5   138.6   138.5   138.9     0.1 │
│  15   be3670.ccr41.sjc03.atlas.cogentco.com (154.54.43.14)               AS174        0.0    19   140.7   140.6   140.4   140.8     0.1 │
│  16   38.104.138.31                                                      AS174        0.0    19   140.0   140.1   139.8   140.3     0.1 │
│  17   tky009bb10.IIJ.Net (58.138.88.225)                                 AS2497      73.7    19   240.2   241.9   240.2   242.9     1.1 │
│  18   tky009ix55.IIJ.Net (58.138.112.109)                                AS2497       0.0    19   241.4   241.3   241.2   241.4     0.1 │
│  19   210.173.173.82                                                     -            0.0    19   239.1   239.0   238.9   239.2     0.1 │
│  20   198.81.233.103.guam.net (198.81.233.103)                           AS3605       0.0    19   235.2   235.3   235.0   235.5     0.1 │
│  21   202.128.5.115.guam.net (202.128.5.115)                             AS3605       0.0    19   270.3   270.2   270.0   270.7     0.2 │
│  22   ae14-cgw-har-gu.guam.net (198.81.233.92)                           AS3605       0.0    19   264.4   264.5   264.1   266.4     0.5 │
│  23   ???                                                                -          100.0    19       -       -       -       -       - │
│  24   ???                                                                -          100.0    19       -       -       -       -       - │
│  25   121-55-201-201.d.c200.guam.net (121.55.201.201)                    AS3605       0.0    19   283.7   282.6   279.6   285.2     1.9 │
└─────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘

trace-id 103115083070058496-nBjPEnny
https://xmsl.dev/xmtr/103115083070058496-nBjPEnny
```

### 上傳與分享

上傳成功後會返回 trace ID 和地圖鏈接。trace ID 由一個 snowflake ID 和 8 個字母的密鑰組成。拿到鏈接的人都能打開這條記錄，密鑰讓別人無法猜出鏈接。

| 選項 | 作用 |
| --- | --- |
| `--pin 2580` | 打開鏈接後要輸入這個四位 PIN（數字 0-8，不含 4）才能查看 |
| `--hide-origin` | 上傳時去掉源地址；前幾跳仍可能暴露出發的網絡 |
| `--no-upload` | 路由只留在本機，不上傳 |

IPMap 把探測包離開時的公網地址作為每條路由的起點。如果本地地址不是公網地址（私有地址、CGNAT、ULA、鏈路本地地址或其他保留地址），xmtr 會從這個本地地址直接向網關發一個 HTTPS 請求，不經過代理設置，取網關看到的地址。這個請求在追蹤期間進行，最多等 3 秒；拿不到公網源地址的路由上傳時不帶源地址。

上傳內容中的源地址、目的地址，以及以 IP 地址形式給出的目標，都會截斷到所在網段：IPv4 取 /24，IPv6 取 /48，例如 `203.0.113.7` 變成 `203.0.113.0`，`2001:db8:1:2::7` 變成 `2001:db8:1::`。各跳保留完整地址，終端裏的表格也顯示完整的目的地址。每跳的每個地址還附帶它自己回包中最短的往返時間，以及它帶上的 MPLS 標籤棧。

### DNS

`--dns` 指定用於解析目標名稱、上傳、反向解析和 AS 查詢的解析器。

| 取值 | 解析器 |
| --- | --- |
| `auto` | 系統配置；目標名稱交給操作系統解析，hosts 文件生效 |
| <img src="../.github/assets/cloudflare.svg" width="14" alt=""> `cloudflare`、<img src="../.github/assets/google.svg" width="14" alt=""> `google`、<img src="../.github/assets/quad9.svg" width="14" alt=""> `quad9`、`9.9.9.9`、`192.0.2.53:5353` | 基於 UDP 和 TCP 的普通 DNS |
| `tcp://[2620:fe::fe]` | DNS over TCP |
| `tls://[2606:4700:4700::1111]`、`dot://dns.quad9.net` | DNS over TLS |
| `https://cloudflare`、`doh://dns.google/dns-query` | DNS over HTTPS |
| `quic://dns.adguard-dns.com` | DNS over QUIC |

`cloudflare`、`google` 和 `quad9` 也可以用作 URL 的主機部分，並且同時包含 IPv4 和 IPv6 服務器。URL 裏的主機名用系統解析器解析一次；證書按公共根證書校驗。DNS over HTTPS 需要主機名或 IPv4 地址。

## 選項

```text
用法: xmtr [OPTIONS] <TARGET>...
      xmtr self update [--force] [--tag <TAG>]
      xmtr self uninstall

-p, --protocol <auto|icmp|udp|tcp>   探測協議（默認 auto）
-4, --ipv4 / -6, --ipv6              只追蹤一種地址族
    --min-rounds <N>                 路由可以停止前至少跑的輪數（默認 5）
-c, --max-rounds <N>                 輪數上限（默認 30）
-t, --time-limit <DURATION>          每條路由的時間上限，至少 1s 且大於 -i（默認 45s）
-i, --interval <DURATION>            兩輪之間的最短間隔，至少 100ms（默認 1s）
-m, --max-ttl <N>                    最大 TTL（默認 30）
    --port <PORT>                    -p tcp 的目的端口（默認 443）
-j, --parallel <N>                   同時追蹤的路由數（默認等於路由總數，最多 8）
-u, --unprivileged                   使用非特權套接字（macOS）
    --dns <RESOLVER>                 見上文 DNS 一節（默認 auto）
    --no-rdns                        跳過反向解析和 AS 查詢
    --no-upload                      只輸出路由
    --hide-origin                    上傳時不帶路由的源地址
    --pin <PIN>                      四位數字，取 0-8，不含 4
    --api <URL>                      上傳網關，也可用 XMTR_API 設置
    --json                           在 stdout 輸出上傳內容，表格寫到 stderr
-q, --quiet                          只輸出 IPMap 鏈接
    --progress <VIEW>                auto、panel、line 或 plain（默認 auto）
    --theme <THEME>                  auto、dark、light、dark-ansi 或 light-ansi（默認 auto）
```

## 遙測

發佈版本會向 [GlitchTip](https://glitchtip.com) 上的 xmtr 項目發送報告。從源碼構建的版本（例如 `cargo install` 和 `nix build`）不發送任何內容，除非編譯時設置了 `XMTR_TELEMETRY_DSN`。

| 報告 | 何時發送 | 內容 |
| --- | --- | --- |
| 使用情況 | 每 100 次帶上傳的運行抽一次，以及每次 `xmtr self` 命令 | xmtr 版本、操作系統及版本（例如 Debian 13）、CPU 架構、命令、耗時、這次運行是否出錯 |
| 錯誤 | 運行出錯後，你回答 yes 時 | 錯誤信息、xmtr 版本、操作系統、CPU 架構 |
| 崩潰 | 發生 panic 後，你回答 yes 時 | panic 信息及其源碼位置、xmtr 版本、操作系統、CPU 架構 |

只有 stdin 和 stderr 都是終端，並且沒有設置 `--json` 或 `--quiet` 時，xmtr 才會詢問是否發送錯誤和崩潰報告。默認回答是 no，沒有終端時什麼都不發送。發送前，xmtr 把 IP 地址替換成 `[ip]`；把目標、以主機名給出的解析器或網關、代理主機替換成 `[host]`；把用户主目錄替換成 `~`；把路徑裏的用户名替換成 `[user]`；並去掉 URL 裏的憑據。報告不包含本機名稱和命令行。GlitchTip 會記錄報告的來源地址，並截斷到網絡前綴。

`DO_NOT_TRACK=1` 關閉所有報告和詢問（[Console Do Not Track](https://consoledonottrack.com)）。

## 從源碼構建

```sh
cargo build --release --workspace
```

工作區會構建 `xmtr` 和 `xmtr-installer`，需要 Rust 1.88 或更新版本。把 `XMTR_TELEMETRY_DSN` 設為 GlitchTip 或 Sentry 的 DSN，構建出的程序就帶[遙測](#遙測)。`nix build` 從 flake 構建 `xmtr`。

發佈構建用 [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) 編譯 Linux、Windows 和 macOS 版本，用 [cross](https://github.com/cross-rs/cross) 編譯 FreeBSD、powerpc64 和 sparc64 版本，用 [cargo-ndk](https://github.com/bbqsrc/cargo-ndk) 編譯 Android 版本；見 `.github/workflows/release.yml` 和 [`scripts/ci`](../scripts/ci/README.md)。工作區使用 `vendor/` 裏打過補丁的 trippy-core，讓 Android 能追蹤 IPv4；見 [vendor/README.md](../vendor/README.md)。

## 翻譯

本文是 README 的簡體中文版。繁體中文版由 [OpenCC](https://github.com/BYVoid/OpenCC) 從本文生成：`zh-TW.md` 使用台灣用詞（`s2twp`），`zh-HK.md` 和 `zh-MO.md` 使用香港字形（`s2hk`）。修改 `zh-CN.md` 後重新生成：

```sh
uv run --project scripts/ci xmtr-ci docs
```

## 發佈

發佈由 [knope](https://github.com/knope-dev/knope) 和變更文件管理：

1. 每個用户能感知到的改動，都運行 `knope document-change`，並提交它寫入 `.changeset/` 的文件。
2. 發佈時運行 `knope release`。它更新 `Cargo.toml`、`Cargo.lock`、README 及其各語言版本中的版本號，把變更文件合併進 [CHANGELOG.md](../CHANGELOG.md)，提交改動，再打上 `vX.Y.Z` tag。1.0 之前，`minor` 級別的變更文件只增加 patch 版本號；`knope release --override-version X.Y.0` 可以直接指定版本。
3. `git push --atomic origin main vX.Y.Z` 觸發發佈流水線。它構建並檢查所有目標平台，發佈 GitHub release（説明取自 CHANGELOG.md 中對應的一節），然後發佈到各個包管理器。

CI 會檢查 `knope.toml`，確認所有帶版本號的文件版本一致，並在還有待發布的變更文件時，把下一個版本的預覽寫進任務摘要。

## 許可證

按你的選擇，使用 [Apache License 2.0](../LICENSE-APACHE) 或 [MIT](../LICENSE-MIT) 許可證。
